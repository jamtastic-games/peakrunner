use std::num::NonZeroU64;

use bytemuck::{Pod, Zeroable};
use eframe::egui_wgpu::wgpu;
use eframe::egui_wgpu::{CallbackResources, CallbackTrait, ScreenDescriptor};
use glam::{Mat4, Vec3};

use crate::drawlist::{normal_columns, DrawFrame, EmitDraw, LitDraw, MeshId};
use crate::grass;
use crate::terrain::{self, MapId};

const SLOT: u64 = 512;

/// Anti-aliasing request from the settings (on by default). The scene
/// rebuilds its pipelines when this changes; unsupported adapters stay at 1x.
pub static MSAA_WANTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
const MSAA_SAMPLES: u32 = 4;
/// Bloom from the settings: 0 off, 1 low, 2 high (see `bloom_settings`).
pub static BLOOM_LEVEL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);
/// Blur levels allocated below the scene: 1/2, 1/4, ... 1/32 size.
const BLOOM_LEVELS: usize = 5;

/// Bloom level in effect: `QA_BLOOM=off|low|high` (QA captures, which run on
/// in-memory settings) overrides the saved setting.
fn bloom_level() -> u8 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        static QA: std::sync::OnceLock<Option<u8>> = std::sync::OnceLock::new();
        let qa = QA.get_or_init(|| match std::env::var("QA_BLOOM").ok()?.as_str() {
            "off" => Some(0),
            "low" => Some(1),
            "high" => Some(2),
            _ => None,
        });
        if let Some(level) = qa {
            return *level;
        }
    }
    BLOOM_LEVEL.load(std::sync::atomic::Ordering::Relaxed)
}

/// (blur levels used, composite intensity) for a bloom setting; 0 levels = off.
pub fn bloom_settings(level: u8) -> (usize, f32) {
    match level {
        0 => (0, 0.0),
        1 => (4, 0.7),
        _ => (5, 1.1),
    }
}
const WORLD_SIZE: u64 = 368;
const MODEL_SIZE: u64 = 464;
/// One skinned draw's bone palette: `model::MAX_BONES` matrices.
const PALETTE: u64 = (crate::model::MAX_BONES * 64) as u64;
const SKY_SIZE: u64 = 96;
const EMIT_SIZE: u64 = 112;

/// Weather drops: world position, the surface it stops on, whether a roof
/// hides it (then it isn't drawn), and a seed.
#[derive(Clone, Copy)]
struct Drop { pos: Vec3, floor: f32, covered: bool, seed: u32 }
const MAX_DROPS: usize = 640;

/// How many weather drops to draw, and the fall speed, size and colour
/// for the conditions (rain streaks, snowflakes); none without weather.
fn weather_drops(c: &peakrunner_core::conditions::Conditions) -> Option<(usize, f32, Vec3, Vec3)> {
    use peakrunner_core::conditions::Weather;
    match c.weather {
        Weather::Rain => Some((440, 19.0, Vec3::new(0.014, 0.62, 0.014), Vec3::new(0.62, 0.68, 0.76))),
        Weather::Storm => Some((MAX_DROPS, 24.0, Vec3::new(0.016, 0.8, 0.016), Vec3::new(0.58, 0.63, 0.72))),
        Weather::Snow => Some((420, 2.1, Vec3::splat(0.05), Vec3::splat(0.93))),
        _ => None,
    }
}

fn precipitation_count(map: MapId, available: usize) -> usize {
    if map == MapId::Valley { available } else { 0 }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct WorldUniform {
    mvp: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    n0: [f32; 4],
    n1: [f32; 4],
    n2: [f32; 4],
    cam: [f32; 3],
    emit: f32,
    sun: [f32; 3],
    mode: f32,
    fog: [f32; 3],
    pad0: f32,
    color: [f32; 3],
    pad1: f32,
    ground_cover: [f32; 4],
    ground_soil: [f32; 4],
    ground_rock: [f32; 4],
    ground_rules: [f32; 4],
    /// World to the sun's shadow map (`ShadowGpu`), for a caster's depth.
    light_vp: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkyUniform {
    inv_vp: [[f32; 4]; 4],
    cam: [f32; 3],
    pad0: f32,
    sun: [f32; 3],
    pad1: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EmitUniform {
    mvp: [[f32; 4]; 4],
    color: [f32; 4],
    fog: [f32; 4],
    params: [f32; 4],
}

/// `model.wgsl`'s uniforms.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ModelUniform {
    mvp: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    n0: [f32; 4],
    n1: [f32; 4],
    n2: [f32; 4],
    cam: [f32; 3],
    glow: f32,
    sun: [f32; 3],
    density: f32,
    fog: [f32; 3],
    /// How brightly the painted plates light themselves (`ModelDraw::shine`).
    shine: f32,
    team: [f32; 3],
    /// How much of the bare plates take `team` (`ModelDraw::plates`).
    plates: f32,
    base: [f32; 4],
    emissive: [f32; 4],
    /// Metalness, roughness, 1 with a normal map, and the map's exposure.
    surface: [f32; 4],
    /// The map's look (`EntityLight`).
    sun_color: [f32; 4],
    amb_sky: [f32; 4],
    amb_ground: [f32; 4],
    zenith: [f32; 4],
    horizon: [f32; 4],
    /// World to the sun's shadow map, and its texel size, bias, strength
    /// and whether there is one (`ShadowGpu`).
    light_vp: [[f32; 4]; 4],
    shadow: [f32; 4],
    /// x: 1 for a skinned part (bent by its palette, `ModelDraw::skin`).
    skin: [f32; 4],
}

/// How a frame lights its models: the map's look after the round's
/// conditions (`map.wgsl` lights the ground with the same), and its sun
/// shadows.
#[derive(Clone, Copy)]
struct EntityLight {
    sun: Vec3,
    sun_color: [f32; 4],
    amb_sky: [f32; 4],
    amb_ground: [f32; 4],
    zenith: [f32; 4],
    horizon: [f32; 4],
    exposure: f32,
    light_vp: Mat4,
    shadow: [f32; 4],
}

impl EntityLight {
    fn of(frame: &DrawFrame, shadows: bool) -> Self {
        let look = crate::map_scene::entity_look(frame);
        let sun = Vec3::from(look.sun_direction).normalize_or(Vec3::Y);
        let (sky, zenith, horizon) = match look.sky { Some(s) => (1.0, s.zenith, s.horizon), None => (0.0, [0.0; 3], [0.0; 3]) };
        let c = |v: [f32; 3], w: f32| [v[0], v[1], v[2], w];
        let light_vp = shadow_matrix(frame, sun);
        EntityLight { sun, sun_color: c(look.sun_color, 0.0), amb_sky: c(look.ambient_sky, sky), amb_ground: c(look.ambient_ground, 0.0),
            zenith: c(zenith, 0.0), horizon: c(horizon, 0.0), exposure: look.exposure, light_vp,
            shadow: [SHADOW_SPAN / SHADOW_SIZE as f32, SHADOW_BIAS, 1.0, if shadows { 1.0 } else { 0.0 }] }
    }
}

/// Sun shadows: a square of the world this wide (m) round the camera, seen
/// from the sun at this resolution (about 3 cm a texel), and the depth bias
/// a lookup leaves so surfaces don't shadow themselves.
pub(crate) const SHADOW_SIZE: u32 = 2048;
pub(crate) const SHADOW_SPAN: f32 = 64.0;
pub(crate) const SHADOW_BIAS: f32 = 0.0015;
/// How far from the camera a body casts a sun shadow (m).
pub(crate) const SHADOW_RANGE: f32 = 40.0;

/// World to the sun's shadow map for a frame: an orthographic box down the
/// sun over the ground ahead of the camera, its corner snapped to whole
/// texels so shadows don't crawl as the camera moves.
pub(crate) fn shadow_matrix(frame: &DrawFrame, sun: Vec3) -> Mat4 {
    let forward = frame.view.inverse().transform_vector3(Vec3::NEG_Z);
    let ahead = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let centre = frame.eye + ahead * SHADOW_SPAN * 0.25;
    let up = if sun.y.abs() > 0.98 { Vec3::Z } else { Vec3::Y };
    let turn = Mat4::look_at_rh(Vec3::ZERO, -sun, up);
    let at = turn.transform_point3(centre);
    let texel = SHADOW_SPAN / SHADOW_SIZE as f32;
    let (x, y) = ((at.x / texel).floor() * texel, (at.y / texel).floor() * texel);
    let half = SHADOW_SPAN * 0.5;
    Mat4::orthographic_rh(x - half, x + half, y - half, y + half, -at.z - 160.0, -at.z + 160.0) * turn
}

/// An authored model on the GPU: one vertex and index buffer, and a
/// texture group per material.
struct ModelGpu {
    vbo: wgpu::Buffer,
    ibo: wgpu::Buffer,
    groups: Vec<wgpu::BindGroup>,
    _textures: Vec<wgpu::Texture>,
}

/// The textured-model pass: its pipeline, layouts and loaded models.
struct Models {
    pipe: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    textures: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    /// 1x1 stand-ins for a material without a texture: white, no glow or
    /// team paint, and a flat normal.
    white: wgpu::Texture,
    black: wgpu::Texture,
    flat: wgpu::Texture,
    loaded: Vec<Option<ModelGpu>>,
    /// Depth from the sun, for the shadow map (`ShadowGpu`).
    shadow_pipe: wgpu::RenderPipeline,
    /// The frame's bone palettes (`DrawFrame::bones`), one bound per
    /// skinned draw at its offset (group 3; the shadow pass's 1).
    bones_layout: wgpu::BindGroupLayout,
    bones: wgpu::Buffer,
    bones_group: wgpu::BindGroup,
}

/// Sun shadows for the people and what they hold: a depth map drawn from
/// the sun over the ground round the camera (`shadow_matrix`), which the
/// map and the models read (`map.wgsl`, `model.wgsl` `sunlit`).
struct ShadowGpu {
    view: wgpu::TextureView,
    layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    /// Depth of the built-in parts (`shaders.wgsl` `vs_world_shadow`).
    lit_pipe: wgpu::RenderPipeline,
}

impl ShadowGpu {
    fn new(device: &wgpu::Device, shader: &wgpu::ShaderModule, world_layout: &wgpu::BindGroupLayout) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sun shadows"),
            size: wgpu::Extent3d { width: SHADOW_SIZE, height: SHADOW_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let layout = shadow_layout(device);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sun shadows"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sun shadows"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow lit"),
            bind_group_layouts: &[Some(world_layout)],
            immediate_size: 0,
        });
        let lit_pipe = depth_only_pipeline(device, &pipe_layout, shader, "vs_world_shadow", vertex_layout());
        ShadowGpu { view, layout, group, lit_pipe }
    }
}

/// The shadow map's bind group layout: the depth texture and its
/// comparison sampler (group 2 of the model pipeline, 1 of the map's).
pub(crate) fn shadow_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("sun shadows"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ],
    })
}

/// A pipeline that only writes depth into the shadow map: no culling (thin
/// plates cast from either side) and a slope bias against acne.
fn depth_only_pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, shader: &wgpu::ShaderModule, entry: &str,
    buffer: wgpu::VertexBufferLayout<'_>) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("shadow depth"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(buffer)],
        },
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 },
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: None,
        multiview_mask: None,
        cache: None,
    })
}

struct Mesh {
    vbo: wgpu::Buffer,
    ibo: wgpu::Buffer,
    count: u32,
}

pub struct SceneGpu {
    imported: Option<crate::map_scene::MapGpu>,
    world_pipe: wgpu::RenderPipeline,
    emit_pipe: wgpu::RenderPipeline,
    smoke_pipe: wgpu::RenderPipeline,
    sky_pipe: wgpu::RenderPipeline,
    blit_pipe: wgpu::RenderPipeline,
    world_layout: wgpu::BindGroupLayout,
    sky_layout: wgpu::BindGroupLayout,
    emit_layout: wgpu::BindGroupLayout,
    blit_layout: wgpu::BindGroupLayout,
    meshes: [Mesh; 10],
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    uniform_slots: u32,
    world_bg: wgpu::BindGroup,
    sky_bg: wgpu::BindGroup,
    emit_bg: wgpu::BindGroup,
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    blit_bg: wgpu::BindGroup,
    /// Multisampled colour target the 3D passes draw into; resolved into
    /// `color` (which stays single-sampled for the blit and captures).
    msaa: Option<(wgpu::Texture, wgpu::TextureView)>,
    samples: u32,
    max_samples: u32,
    size: (u32, u32),
    snow: Vec<Vec3>,
    /// Deathmatch rain and snow, around the camera.
    drops: Vec<Drop>,
    /// All visible drops as one world-space mesh, drawn in a single call:
    /// vertex buffer rewritten each frame, a static index buffer.
    weather_mesh: Option<(wgpu::Buffer, wgpu::Buffer)>,
    target_is_srgb: bool,
    terrain_map: MapId,
    grass_ready: bool,
    grass_albedo: wgpu::Texture,
    grass_normal: wgpu::Texture,
    grass_albedo_view: wgpu::TextureView,
    grass_normal_view: wgpu::TextureView,
    grass_sampler: wgpu::Sampler,
    bloom: Bloom,
    models: Models,
    shadow: ShadowGpu,
}

/// Glow pipelines and the half-to-1/32 blur chain. Level 0 (half size) ends up
/// holding the whole blurred glow, which the blit screen-blends over the scene.
struct Bloom {
    layout: wgpu::BindGroupLayout,
    prefilter: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    up: wgpu::RenderPipeline,
    params: wgpu::Buffer,
    /// Samples the resolved scene colour.
    scene_bg: wgpu::BindGroup,
    /// (texture, view, bind group sampling this level), largest first.
    levels: Vec<(wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
}

impl Bloom {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bloom"),
            source: wgpu::ShaderSource::Wgsl(include_str!("bloom.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom"),
            entries: &[texture_entry(0), wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            }],
        });
        let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let make = |entry: &str, additive: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipe_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_bloom"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: additive.then_some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::One,
                                dst_factor: wgpu::BlendFactor::One,
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent::REPLACE,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let prefilter = make("fs_prefilter", false);
        let down = make("fs_down", false);
        let up = make("fs_up", true);
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bloom params"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let placeholder = device.create_texture(&bloom_texture_desc(1, 1));
        let view = placeholder.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        let scene_bg = bloom_group(device, &layout, &view, &sampler);
        Self { layout, prefilter, down, up, params, scene_bg, levels: Vec::new() }
    }

    /// Rebuild the chain for a new scene size.
    fn resize(&mut self, device: &wgpu::Device, sampler: &wgpu::Sampler, scene: &wgpu::TextureView, width: u32, height: u32) {
        self.scene_bg = bloom_group(device, &self.layout, scene, sampler);
        self.levels = (0..BLOOM_LEVELS)
            .map(|i| {
                let texture = device.create_texture(&bloom_texture_desc((width >> (i + 1)).max(1), (height >> (i + 1)).max(1)));
                let view = texture.create_view(&Default::default());
                let bg = bloom_group(device, &self.layout, &view, sampler);
                (texture, view, bg)
            })
            .collect();
    }

    fn pass<'a>(encoder: &'a mut wgpu::CommandEncoder, target: &wgpu::TextureView, clear: bool) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bloom"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if clear { wgpu::LoadOp::Clear(wgpu::Color::BLACK) } else { wgpu::LoadOp::Load },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// Glow prefilter, downsample to `count` levels, then upsample back into
    /// level 0. With `count` 0 only level 0 is cleared (bloom off).
    fn run(&self, encoder: &mut wgpu::CommandEncoder, count: usize) {
        let count = count.min(self.levels.len());
        if count == 0 {
            if let Some(level) = self.levels.first() {
                Self::pass(encoder, &level.1, true);
            }
            return;
        }
        {
            let mut pass = Self::pass(encoder, &self.levels[0].1, true);
            pass.set_pipeline(&self.prefilter);
            pass.set_bind_group(0, &self.scene_bg, &[]);
            pass.draw(0..3, 0..1);
        }
        for i in 1..count {
            let mut pass = Self::pass(encoder, &self.levels[i].1, true);
            pass.set_pipeline(&self.down);
            pass.set_bind_group(0, &self.levels[i - 1].2, &[]);
            pass.draw(0..3, 0..1);
        }
        for i in (1..count).rev() {
            let mut pass = Self::pass(encoder, &self.levels[i - 1].1, false);
            pass.set_pipeline(&self.up);
            pass.set_bind_group(0, &self.levels[i].2, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

fn bloom_texture_desc(width: u32, height: u32) -> wgpu::TextureDescriptor<'static> {
    wgpu::TextureDescriptor {
        label: Some("bloom"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    }
}

fn bloom_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, view: &wgpu::TextureView, sampler: &wgpu::Sampler) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bloom"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

impl SceneGpu {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("peakrunner"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.wgsl").into()),
        });
        let (grass_albedo, grass_albedo_view, grass_normal, grass_normal_view, grass_sampler) =
            grass_textures(device);
        let world_layout = world_bind_layout(device);
        let sky_layout = uniform_layout(device, "sky", SKY_SIZE);
        let emit_layout = uniform_layout(device, "emit", EMIT_SIZE);
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture_entry(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(16),
                    },
                    count: None,
                },
            ],
        });

        let world_pipe = lit_pipeline(device, &shader, &world_layout, false, target_format, 1);
        let emit_pipe = emit_pipeline(device, &shader, &emit_layout, false, 1);
        let smoke_pipe = emit_pipeline(device, &shader, &emit_layout, true, 1);
        let sky_pipe = sky_pipeline(device, &shader, &sky_layout, 1);
        let blit_entry = if target_format.is_srgb() {
            "fs_blit_srgb"
        } else {
            "fs_blit"
        };
        let blit_pipe = blit_pipeline(device, &shader, &blit_layout, target_format, blit_entry);

        let meshes = [
            upload(device, "terrain", &terrain::sample_mesh_of(MapId::Valley)),
            upload(device, "cube", &cube_mesh()),
            upload(device, "sphere", &sphere_mesh(10, 16)),
            upload(device, "disc", &disc_mesh()),
            upload(device, "beveled housing", &bevel_mesh()),
            upload(device, "chamfered armor", &armor_mesh()),
            upload(device, "decal", &decal_mesh()),
            upload(device, "head", &crate::character_mesh::head_mesh()),
            upload(device, "dome", &crate::character_mesh::dome_mesh()),
            upload(device, "capsule", &crate::character_mesh::capsule_mesh()),
        ];
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scene"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform_slots = 512;
        let uniform = make_uniform(device, uniform_slots);
        let world_bg = world_bind_group(
            device,
            &world_layout,
            &uniform,
            &grass_albedo_view,
            &grass_normal_view,
            &grass_sampler,
        );
        let sky_bg = uniform_group(device, &sky_layout, &uniform, SKY_SIZE);
        let emit_bg = uniform_group(device, &emit_layout, &uniform, EMIT_SIZE);
        let (color, color_view, depth, depth_view, msaa) = make_target(device, 4, 4, 1);
        let mut bloom = Bloom::new(device);
        bloom.resize(device, &sampler, &color_view, 4, 4);
        let blit_bg = blit_group(device, &blit_layout, &color_view, &bloom, &sampler);
        let shadow = ShadowGpu::new(device, &shader, &world_layout);
        let models = Models::new(device, &uniform, 1, &shadow.layout);

        let mut snow = Vec::new();
        let mut rng = 0x91A2u32;
        for _ in 0..90 {
            rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let x = (rng >> 8) as f32 / 16_777_216.0;
            rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let y = (rng >> 8) as f32 / 16_777_216.0;
            rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let z = (rng >> 8) as f32 / 16_777_216.0;
            snow.push(Vec3::new(x * 40.0 - 20.0, y * 22.0, z * 40.0 - 20.0));
        }

        Self {
            imported: None,
            world_pipe,
            emit_pipe,
            smoke_pipe,
            sky_pipe,
            blit_pipe,
            world_layout,
            sky_layout,
            emit_layout,
            blit_layout,
            meshes,
            sampler,
            uniform,
            uniform_slots,
            world_bg,
            sky_bg,
            emit_bg,
            color,
            color_view,
            depth,
            depth_view,
            blit_bg,
            msaa,
            samples: 1,
            max_samples: 1,
            size: (4, 4),
            snow,
            drops: Vec::new(),
            weather_mesh: None,
            target_is_srgb: target_format.is_srgb(),
            terrain_map: MapId::Valley,
            grass_ready: false,
            grass_albedo,
            grass_normal,
            grass_albedo_view,
            grass_normal_view,
            grass_sampler,
            bloom,
            models,
            shadow,
        }
    }

    /// Highest MSAA sample count the adapter can render and resolve for the
    /// scene formats. Tests and headless captures stay at 1x unless set.
    pub fn set_max_samples(&mut self, samples: u32) {
        self.max_samples = samples.max(1);
    }

    fn set_samples(&mut self, device: &wgpu::Device, samples: u32) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("peakrunner"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.wgsl").into()),
        });
        let format = wgpu::TextureFormat::Rgba8Unorm;
        self.world_pipe = lit_pipeline(device, &shader, &self.world_layout, false, format, samples);
        self.emit_pipe = emit_pipeline(device, &shader, &self.emit_layout, false, samples);
        self.smoke_pipe = emit_pipeline(device, &shader, &self.emit_layout, true, samples);
        self.sky_pipe = sky_pipeline(device, &shader, &self.sky_layout, samples);
        self.models.pipe = model_pipeline(device, &self.models.layout, &self.models.textures, &self.shadow.layout, &self.models.bones_layout, samples);
        self.samples = samples;
        // Force new targets and map pipelines at the new sample count.
        self.size = (0, 0);
        if self.imported.is_some() {
            self.imported = crate::map_scene::MapGpu::new(device, self.terrain_map, samples, &self.shadow.layout);
        }
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        width: u32,
        height: u32,
        frame: &DrawFrame,
    ) {
        self.upload_grass(queue);
        let want = if MSAA_WANTED.load(std::sync::atomic::Ordering::Relaxed) && self.max_samples >= MSAA_SAMPLES { MSAA_SAMPLES } else { 1 };
        if want != self.samples {
            self.set_samples(device, want);
        }
        if self.terrain_map != frame.map {
            self.imported = crate::map_scene::MapGpu::new(device,frame.map,self.samples,&self.shadow.layout);
            self.meshes[0] = upload(device, "terrain", &terrain::sample_mesh_of(frame.map));
            self.terrain_map = frame.map;
        }
        if let Some(map)=&mut self.imported {map.update(queue,frame);}

        let width = width.max(1);
        let height = height.max(1);
        if self.size != (width, height) {
            let (color, color_view, depth, depth_view, msaa) =
                make_target(device, width, height, self.samples);
            self.bloom.resize(device, &self.sampler, &color_view, width, height);
            self.blit_bg = blit_group(device, &self.blit_layout, &color_view, &self.bloom, &self.sampler);
            self.color = color;
            self.color_view = color_view;
            self.depth = depth;
            self.depth_view = depth_view;
            self.msaa = msaa;
            self.size = (width, height);
        }

        let mut snow_draws = Vec::with_capacity(self.snow.len());
        let eye = frame.eye;
        let dt = frame.dt;
        let precipitation_count = precipitation_count(frame.map, self.snow.len());
        // Old Holler's precipitation is off, for both gameplay and the flyby.
        // Do not reinterpret the sparse Valley snow pool as patchy rain.
        for s in self.snow.iter_mut().take(precipitation_count) {
            s.y -= 4.2*dt;
            s.x += 0.4 * dt;
            let w = eye + *s;
            if w.y < terrain::height_on(frame.map, w.x, w.z) + 0.4 || (w - eye).length() > 24.0 {
                *s = Vec3::new(
                    ((s.x + 17.0) % 40.0) - 20.0,
                    10.0 + ((s.z.abs() * 3.0) % 8.0),
                    ((s.z + 11.0) % 40.0) - 20.0,
                );
            }
            snow_draws.push(LitDraw {
                mesh: MeshId::Cube,
                model: Mat4::from_translation(eye + *s) * Mat4::from_scale(Vec3::splat(0.045)),
                color: Vec3::splat(0.92),
                emit: 0.15,
                mode: 0.0,
            });
        }

        let mut weather_models: Vec<Mat4> = Vec::new();
        let mut weather_draw: Option<LitDraw> = None;
        // Weather: drops fall with the wind in a box around the camera and
        // stop on the first surface below where they started (roofs too).
        if let Some((count, fall, size, color)) = weather_drops(&frame.conditions) {
            let wind = Vec3::new(frame.conditions.wind[0], 0.0, frame.conditions.wind[1]);
            let vel = wind - Vec3::Y * fall;
            let turn = glam::Quat::from_rotation_arc(Vec3::Y, -vel.normalize_or(Vec3::NEG_Y));
            let snow = fall < 5.0;
            let spawn = |seed: &mut u32, eye: Vec3, spread_y: f32| -> Drop {
                let mut r = || { *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223); (*seed >> 8) as f32 / 16_777_216.0 };
                let pos = eye + Vec3::new(r() * 44.0 - 22.0, 2.0 + r() * spread_y, r() * 44.0 - 22.0) - wind * 0.4;
                // Stop on the first surface below (a roof too); a drop with
                // anything solid above it is indoors and never drawn.
                let pack = peakrunner_core::map_pack::on(frame.map);
                let down = pos - Vec3::Y * 60.0;
                let floor = pack.and_then(|p| p.sweep(pos, down, 0.0)).map_or(terrain::support_on(frame.map, pos).0,
                    |(t, _)| pos.y - 60.0 * t).max(terrain::height_on(frame.map, pos.x, pos.z));
                let covered = pack.is_some_and(|p| p.sweep(pos, pos + Vec3::Y * 80.0, 0.0).is_some());
                Drop { pos, floor, covered, seed: *seed }
            };
            // Placing a drop costs two map sweeps, so only so many are placed
            // per frame; the rest wait unseen (after a respawn or the first
            // frame of weather, the sky fills over a few frames).
            const PLACE_PER_FRAME: usize = 96;
            let mut place = PLACE_PER_FRAME;
            if self.drops.len() != MAX_DROPS {
                self.drops = (0..MAX_DROPS as u32).map(|i| Drop { pos: eye, floor: f32::INFINITY, covered: true,
                    seed: 0x5EED_u32.wrapping_add(i.wrapping_mul(2_654_435_761)) }).collect();
            }
            for d in self.drops.iter_mut().take(count) {
                if d.floor == f32::INFINITY {
                    if place == 0 { continue; }
                    place -= 1;
                    let mut seed = d.seed;
                    *d = spawn(&mut seed, eye, 16.0);
                }
                let sway = if snow { Vec3::new((frame.time * 1.3 + d.seed as f32 * 1e-6).sin(), 0.0, (frame.time * 1.1 + d.seed as f32 * 3e-6).cos()) * 0.6 } else { Vec3::ZERO };
                d.pos += (vel + sway) * dt;
                let off = d.pos - eye;
                if d.pos.y < d.floor || off.x.abs() > 24.0 || off.z.abs() > 24.0 || off.y > 22.0 || off.y < -12.0 {
                    if place == 0 { d.floor = f32::INFINITY; continue; }
                    place -= 1;
                    let mut seed = d.seed;
                    *d = spawn(&mut seed, eye, 14.0);
                }
                // Covered drops, and any right at the lens (a pole-sized smear).
                if d.covered || (d.pos - eye).length() < 1.5 { continue; }
                let model = if snow { Mat4::from_translation(d.pos) * Mat4::from_scale(size) }
                    else { Mat4::from_scale_rotation_translation(size, turn, d.pos) };
                weather_models.push(model);
            }
            weather_draw = Some(LitDraw { mesh: MeshId::Cube, model: Mat4::IDENTITY, color, emit: if snow { 0.4 } else { 0.3 }, mode: 0.0 });
        }
        let weather_indices = if weather_models.is_empty() { weather_draw = None; 0 } else {
            self.weather_upload(device, queue, &weather_models).1
        };

        for d in frame.models.iter().chain(&frame.vm_models) { self.models.ensure(device, queue, d.id); }
        self.models.upload_bones(device, queue, &frame.bones);
        let slots = 3 + frame.lit.len() + snow_draws.len() + frame.emit.len() + frame.smoke.len() + frame.viewmodel.len()
            + frame.models.len() + frame.vm_models.len();
        self.ensure_slots(device, slots as u32 + 4);

        let mut staging = vec![0u8; self.uniform_slots as usize * SLOT as usize];
        let mut cursor = 0u32;
        let push = |staging: &mut [u8], cursor: &mut u32, bytes: &[u8]| -> u32 {
            let offset = *cursor;
            let start = offset as usize;
            staging[start..start + bytes.len()].copy_from_slice(bytes);
            *cursor += SLOT as u32;
            offset
        };

        // A map that sets `look.sun_direction` lights entities from it too;
        // otherwise the frame's historical sun is kept.
        let sun = peakrunner_core::map_pack::on(frame.map)
            .and_then(|p| p.manifest.look.sun_direction.map(|_| Vec3::from(p.manifest.look.resolved().sun_direction)))
            .unwrap_or(frame.sun);
        let sky_off = push(
            &mut staging,
            &mut cursor,
            bytemuck::bytes_of(&SkyUniform {
                inv_vp: frame.inv_vp.to_cols_array_2d(),
                cam: frame.eye.to_array(),
                pad0: 0.0,
                sun: sun.to_array(),
                pad1: 0.0,
            }),
        );

        // The map's light for the models, and the sun's shadow map over the
        // ground round the camera.
        let light = EntityLight::of(frame, true);
        let mut lit_offs = Vec::with_capacity(frame.lit.len() + snow_draws.len());
        for draw in frame.lit.iter().chain(snow_draws.iter()) {
            let mut u = world_uniform(draw, frame.proj * frame.view, frame.eye, sun * frame.conditions.entity_light(), frame.fog, frame.fog_density, frame.time, frame.map);
            u.light_vp = light.light_vp.to_cols_array_2d();
            lit_offs.push(push(&mut staging, &mut cursor, bytemuck::bytes_of(&u)));
        }
        let weather_off = weather_draw.as_ref().map(|draw| push(&mut staging, &mut cursor,
            bytemuck::bytes_of(&world_uniform(draw, frame.proj * frame.view, frame.eye, sun * frame.conditions.entity_light(), frame.fog, frame.fog_density, frame.time, frame.map))));
        let mut emit_offs = Vec::with_capacity(frame.emit.len());
        for draw in &frame.emit {
            emit_offs.push(push(
                &mut staging,
                &mut cursor,
                bytemuck::bytes_of(&emit_uniform(draw, frame.proj * frame.view, [0.0; 4])),
            ));
        }
        let mut smoke_offs = Vec::with_capacity(frame.smoke.len());
        for draw in &frame.smoke {
            let dist = draw.model.w_axis.truncate().distance(frame.eye);
            let fog = (1.0 - (-dist * frame.fog_density).exp()).clamp(0.0, 0.92);
            smoke_offs.push(push(
                &mut staging,
                &mut cursor,
                bytemuck::bytes_of(&emit_uniform(draw, frame.proj * frame.view,
                    [frame.fog.x, frame.fog.y, frame.fog.z, fog])),
            ));
        }
        let vm_sun = Vec3::new(0.2, 0.8, 0.5).normalize();
        let mut vm_offs = Vec::with_capacity(frame.viewmodel.len());
        for draw in &frame.viewmodel {
            vm_offs.push(push(
                &mut staging,
                &mut cursor,
                bytemuck::bytes_of(&world_uniform(draw, frame.vm_proj, Vec3::ZERO, vm_sun, Vec3::ZERO, 0.0, 0.0, frame.map)),
            ));
        }

        let model_offs: Vec<u32> = frame.models.iter().map(|d| push(&mut staging, &mut cursor, bytemuck::bytes_of(
            &model_uniform(d, frame.proj * frame.view, frame.eye, light.sun, frame.fog, frame.fog_density, &light)))).collect();
        // First person: the map's light from the view's own sun, no shadows.
        let vm_light = EntityLight { shadow: [0.0; 4], ..light };
        let vm_model_offs: Vec<u32> = frame.vm_models.iter().map(|d| push(&mut staging, &mut cursor, bytemuck::bytes_of(
            &model_uniform(d, frame.vm_proj, Vec3::ZERO, vm_sun, Vec3::ZERO, 0.0, &vm_light)))).collect();

        let used = cursor as u64;
        queue.write_buffer(&self.uniform, 0, &staging[..used as usize]);

        // The sun's shadow map: depth of the bodies near the camera, seen
        // from the sun (`DrawFrame::casters`).
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun shadows"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow.view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let near = |at: &Vec3| at.distance_squared(frame.eye) < SHADOW_RANGE * SHADOW_RANGE;
            pass.set_pipeline(&self.shadow.lit_pipe);
            for (lit, _, _) in frame.casters.iter().filter(|c| near(&c.2)) {
                for i in lit.clone() {
                    let (Some(draw), Some(&off)) = (frame.lit.get(i), lit_offs.get(i)) else { continue };
                    let mesh = &self.meshes[draw.mesh as usize];
                    pass.set_bind_group(0, &self.world_bg, &[off]);
                    pass.set_vertex_buffer(0, mesh.vbo.slice(..));
                    pass.set_index_buffer(mesh.ibo.slice(..), wgpu::IndexFormat::Uint16);
                    pass.draw_indexed(0..mesh.count, 0, 0..1);
                }
            }
            pass.set_pipeline(&self.models.shadow_pipe);
            for (_, models, _) in frame.casters.iter().filter(|c| near(&c.2)) {
                self.models.draw_depth(&mut pass, &frame.models, &model_offs, models.clone());
            }
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("world"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.msaa.as_ref().map_or(&self.color_view, |m| &m.1),
                depth_slice: None,
                resolve_target: self.msaa.as_ref().map(|_| &self.color_view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.07,
                        g: 0.10,
                        b: 0.16,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.sky_pipe);
        pass.set_bind_group(1, &self.sky_bg, &[sky_off]);
        pass.draw(0..3, 0..1);

        if frame.map != MapId::Valley {
            if let Some(map)=&self.imported {map.draw(&mut pass,&self.shadow.group);}
        }

        pass.set_pipeline(&self.world_pipe);
        let lit_count = frame.lit.len();
        for (i, draw) in frame.lit.iter().chain(snow_draws.iter()).enumerate() {
            let mesh = &self.meshes[draw.mesh as usize];
            pass.set_bind_group(0, &self.world_bg, &[lit_offs[i]]);
            pass.set_vertex_buffer(0, mesh.vbo.slice(..));
            pass.set_index_buffer(mesh.ibo.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..mesh.count, 0, 0..1);
            let _ = lit_count;
        }
        if let (Some(off), Some((vbo, ibo))) = (weather_off, &self.weather_mesh) {
            pass.set_bind_group(0, &self.world_bg, &[off]);
            pass.set_vertex_buffer(0, vbo.slice(..));
            pass.set_index_buffer(ibo.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..weather_indices, 0, 0..1);
        }
        // Authored models last among the opaque draws: they switch pipelines.
        self.models.draw(&mut pass, &frame.models, &model_offs, &self.shadow.group);
        // Darkening effects first, back to front (drawlist sorts them), then
        // the additive pass so flames and sparks glow through the smoke.
        if !frame.smoke.is_empty() {
            pass.set_pipeline(&self.smoke_pipe);
            for (i, draw) in frame.smoke.iter().enumerate() {
                let mesh = &self.meshes[draw.mesh as usize];
                pass.set_bind_group(2, &self.emit_bg, &[smoke_offs[i]]);
                pass.set_vertex_buffer(0, mesh.vbo.slice(..));
                pass.set_index_buffer(mesh.ibo.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
        }
        if !frame.emit.is_empty() {
            pass.set_pipeline(&self.emit_pipe);
            for (i, draw) in frame.emit.iter().enumerate() {
                let mesh = &self.meshes[draw.mesh as usize];
                pass.set_bind_group(2, &self.emit_bg, &[emit_offs[i]]);
                pass.set_vertex_buffer(0, mesh.vbo.slice(..));
                pass.set_index_buffer(mesh.ibo.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
        }
        drop(pass);

        // The first-person pass: built-in parts, models, or both (an authored
        // weapon held in the armor's own hands has no built-in parts).
        if !frame.viewmodel.is_empty() || !frame.vm_models.is_empty() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewmodel"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.msaa.as_ref().map_or(&self.color_view, |m| &m.1),
                    depth_slice: None,
                    resolve_target: self.msaa.as_ref().map(|_| &self.color_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.world_pipe);
            for (i, draw) in frame.viewmodel.iter().enumerate() {
                let mesh = &self.meshes[draw.mesh as usize];
                pass.set_bind_group(0, &self.world_bg, &[vm_offs[i]]);
                pass.set_vertex_buffer(0, mesh.vbo.slice(..));
                pass.set_index_buffer(mesh.ibo.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
            self.models.draw(&mut pass, &frame.vm_models, &vm_model_offs, &self.shadow.group);
        }

        let (levels, intensity) = bloom_settings(bloom_level());
        queue.write_buffer(&self.bloom.params, 0, bytemuck::cast_slice(&[intensity, 0.0f32, 0.0, 0.0]));
        self.bloom.run(encoder, levels);
    }

    /// Draw the scene (with bloom) into the pass the HUD will draw over.
    pub fn composite(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.blit_pipe);
        pass.set_bind_group(3, &self.blit_bg, &[]);
        pass.draw(0..3, 0..1);
    }

    fn upload_grass(&mut self, queue: &wgpu::Queue) {
        if self.grass_ready {
            return;
        }
        let (albedo, normal) = grass::bake_textures();
        upload_rgba_mips(queue, &self.grass_albedo, &albedo, grass::TEX_SIZE);
        upload_rgba_mips(queue, &self.grass_normal, &normal, grass::TEX_SIZE);
        self.grass_ready = true;
    }

    /// Write this frame's drops (each a cube posed by its model) into the
    /// shared weather mesh; returns the index count to draw.
    fn weather_upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, models: &[Mat4]) -> ((), u32) {
        let (cube_v, cube_i) = cube_mesh();
        let per_v = cube_v.len() / 6;
        if self.weather_mesh.is_none() {
            use wgpu::util::DeviceExt;
            let vbo = device.create_buffer(&wgpu::BufferDescriptor { label: Some("weather"),
                size: (MAX_DROPS * cube_v.len() * 4) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            let indices: Vec<u16> = (0..MAX_DROPS).flat_map(|k| cube_i.iter().map(move |&i| i + (k * per_v) as u16)).collect();
            let ibo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("weather"),
                contents: bytemuck::cast_slice(&indices), usage: wgpu::BufferUsages::INDEX });
            self.weather_mesh = Some((vbo, ibo));
        }
        let n = models.len().min(MAX_DROPS);
        let mut verts = Vec::with_capacity(n * cube_v.len());
        for m in &models[..n] {
            let normal = glam::Mat3::from_mat4(*m).inverse().transpose();
            for v in cube_v.chunks_exact(6) {
                let p = m.transform_point3(Vec3::new(v[0], v[1], v[2]));
                let q = (normal * Vec3::new(v[3], v[4], v[5])).normalize_or_zero();
                verts.extend_from_slice(&[p.x, p.y, p.z, q.x, q.y, q.z]);
            }
        }
        if let Some((vbo, _)) = &self.weather_mesh { queue.write_buffer(vbo, 0, bytemuck::cast_slice(&verts)); }
        ((), (n * cube_i.len()) as u32)
    }

    fn ensure_slots(&mut self, device: &wgpu::Device, need: u32) {
        if need <= self.uniform_slots {
            return;
        }
        self.uniform_slots = need.next_power_of_two().max(self.uniform_slots * 2);
        self.uniform = make_uniform(device, self.uniform_slots);
        self.world_bg = world_bind_group(
            device,
            &self.world_layout,
            &self.uniform,
            &self.grass_albedo_view,
            &self.grass_normal_view,
            &self.grass_sampler,
        );
        self.sky_bg = uniform_group(device, &self.sky_layout, &self.uniform, SKY_SIZE);
        self.emit_bg = uniform_group(device, &self.emit_layout, &self.uniform, EMIT_SIZE);
        self.models.group = uniform_group(device, &self.models.layout, &self.uniform, MODEL_SIZE);
    }
}

impl Models {
    fn new(device: &wgpu::Device, uniform: &wgpu::Buffer, samples: u32, shadow: &wgpu::BindGroupLayout) -> Self {
        let layout = uniform_layout(device, "model", MODEL_SIZE);
        let bones_layout = bones_layout(device);
        let (bones, bones_group) = bones_buffer(device, &bones_layout, 4);
        let textures = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model textures"),
            entries: &[texture_entry(0), texture_entry(1), wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            }, texture_entry(3), texture_entry(4), texture_entry(5)],
        });
        let pipe = model_pipeline(device, &layout, &textures, shadow, &bones_layout, samples);
        let shadow_pipe = {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("model"),
                source: wgpu::ShaderSource::Wgsl(include_str!("model.wgsl").into()),
            });
            let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("model shadow"),
                bind_group_layouts: &[Some(&layout), None, None, Some(&bones_layout)],
                immediate_size: 0,
            });
            depth_only_pipeline(device, &pipe_layout, &shader, "vs_model_shadow", model_vertex_layout())
        };
        let group = uniform_group(device, &layout, uniform, MODEL_SIZE);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("model"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let pixel = |label: &str| device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        Models { pipe, layout, textures, group, sampler, white: pixel("model white"), black: pixel("model black"),
            flat: pixel("model flat normal"), loaded: Vec::new(), shadow_pipe, bones_layout, bones, bones_group }
    }

    /// Upload model `id` the first time it's drawn.
    fn ensure(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, id: u16) {
        let id = id as usize;
        if self.loaded.len() <= id { self.loaded.resize_with(id + 1, || None); }
        if self.loaded[id].is_some() { return; }
        let Some(model) = crate::model::by_id(id as u16) else { return };
        if self.loaded.iter().all(Option::is_none) {
            // The stand-in pixels, written once.
            for (t, px) in [(&self.white, [255u8, 255, 255, 255]), (&self.black, [0u8, 0, 0, 0]), (&self.flat, [128u8, 128, 255, 255])] {
                upload_rgba_mips(queue, t, &px, 1);
            }
        }
        use wgpu::util::DeviceExt;
        let vbo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(&model.key),
            contents: bytemuck::cast_slice(&model.vertices), usage: wgpu::BufferUsages::VERTEX });
        let ibo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(&model.key),
            contents: bytemuck::cast_slice(&model.indices), usage: wgpu::BufferUsages::INDEX });
        let textures: Vec<wgpu::Texture> = model.images.iter().map(|img| {
            // Square power-of-two images get a full mip chain.
            let square = img.width == img.height && img.width.is_power_of_two();
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&model.key),
                size: wgpu::Extent3d { width: img.width, height: img.height, depth_or_array_layers: 1 },
                mip_level_count: if square { img.width.trailing_zeros() + 1 } else { 1 },
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            if square { upload_rgba_mips(queue, &texture, &img.rgba, img.width); }
            else {
                queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All }, &img.rgba,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(img.width * 4), rows_per_image: Some(img.height) },
                    wgpu::Extent3d { width: img.width, height: img.height, depth_or_array_layers: 1 });
            }
            texture
        }).collect();
        let view = |t: Option<usize>, fallback: &wgpu::Texture| t.and_then(|i| textures.get(i)).unwrap_or(fallback)
            .create_view(&wgpu::TextureViewDescriptor::default());
        let groups = model.materials.iter().map(|m| {
            let (albedo, glow) = (view(m.albedo, &self.white), view(m.glow, &self.black));
            let (surface, occlusion, normal) = (view(m.surface, &self.white), view(m.occlusion, &self.white), view(m.normal, &self.flat));
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("model material"),
                layout: &self.textures,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&albedo) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&glow) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&surface) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&occlusion) },
                    wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&normal) },
                ],
            })
        }).collect();
        self.loaded[id] = Some(ModelGpu { vbo, ibo, groups, _textures: textures });
    }

    /// Upload the frame's bone palettes, growing the buffer to fit.
    fn upload_bones(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, bones: &[Mat4]) {
        let palettes = bones.len().div_ceil(crate::model::MAX_BONES).max(1) as u64;
        if palettes * PALETTE > self.bones.size() {
            (self.bones, self.bones_group) = bones_buffer(device, &self.bones_layout, palettes.next_power_of_two());
        }
        if !bones.is_empty() {
            let data: Vec<[[f32; 4]; 4]> = bones.iter().map(|m| m.to_cols_array_2d()).collect();
            queue.write_buffer(&self.bones, 0, bytemuck::cast_slice(&data));
        }
    }

    /// A draw's palette offset in the bones buffer (any for a rigid part).
    fn palette(d: &crate::drawlist::ModelDraw) -> u32 {
        if d.skin == crate::drawlist::RIGID { 0 } else { d.skin * PALETTE as u32 }
    }

    /// Draw the parts in `range` of `draws` into the shadow map (depth only).
    fn draw_depth(&self, pass: &mut wgpu::RenderPass<'_>, draws: &[crate::drawlist::ModelDraw], offs: &[u32], range: std::ops::Range<usize>) {
        let mut bound = None;
        for k in range {
            let (Some(d), Some(&off)) = (draws.get(k), offs.get(k)) else { continue };
            let (Some(Some(gpu)), Some(model)) = (self.loaded.get(d.id as usize), crate::model::by_id(d.id)) else { continue };
            let Some(part) = model.parts.get(d.part as usize) else { continue };
            if bound != Some(d.id) {
                pass.set_vertex_buffer(0, gpu.vbo.slice(..));
                pass.set_index_buffer(gpu.ibo.slice(..), wgpu::IndexFormat::Uint32);
                bound = Some(d.id);
            }
            pass.set_bind_group(0, &self.group, &[off]);
            pass.set_bind_group(3, &self.bones_group, &[Self::palette(d)]);
            pass.draw_indexed(part.start..part.start + part.count, 0, 0..1);
        }
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, draws: &[crate::drawlist::ModelDraw], offs: &[u32], shadow: &wgpu::BindGroup) {
        if draws.is_empty() { return; }
        pass.set_pipeline(&self.pipe);
        pass.set_bind_group(2, shadow, &[]);
        let mut bound = None;
        for (d, &off) in draws.iter().zip(offs) {
            let (Some(Some(gpu)), Some(model)) = (self.loaded.get(d.id as usize), crate::model::by_id(d.id)) else { continue };
            let Some(part) = model.parts.get(d.part as usize) else { continue };
            if bound != Some(d.id) {
                pass.set_vertex_buffer(0, gpu.vbo.slice(..));
                pass.set_index_buffer(gpu.ibo.slice(..), wgpu::IndexFormat::Uint32);
                bound = Some(d.id);
            }
            pass.set_bind_group(0, &self.group, &[off]);
            pass.set_bind_group(1, &gpu.groups[part.material], &[]);
            pass.set_bind_group(3, &self.bones_group, &[Self::palette(d)]);
            pass.draw_indexed(part.start..part.start + part.count, 0, 0..1);
        }
    }
}

fn model_uniform(draw: &crate::drawlist::ModelDraw, vp: Mat4, cam: Vec3, sun: Vec3, fog: Vec3, density: f32, light: &EntityLight) -> ModelUniform {
    let material = crate::model::by_id(draw.id).and_then(|m| m.parts.get(draw.part as usize).map(|p| &m.materials[p.material]));
    let (base, emissive) = material.map_or(([1.0; 4], [0.0; 4]), |m| (m.color, [m.emissive[0], m.emissive[1], m.emissive[2], if m.team || draw.paint { 1.0 } else { 0.0 }]));
    let surface = material.map_or([0.0, 0.8, 0.0, light.exposure], |m| [m.metallic, m.roughness, if m.normal.is_some() { 1.0 } else { 0.0 }, light.exposure]);
    let n = normal_columns(draw.model);
    ModelUniform {
        mvp: (vp * draw.model).to_cols_array_2d(),
        model: draw.model.to_cols_array_2d(),
        n0: n[0], n1: n[1], n2: n[2],
        cam: cam.to_array(),
        glow: draw.glow,
        sun: sun.to_array(),
        density,
        fog: fog.to_array(),
        shine: draw.shine,
        team: draw.team.to_array(),
        plates: draw.plates,
        base,
        emissive,
        surface,
        sun_color: light.sun_color,
        amb_sky: light.amb_sky,
        amb_ground: light.amb_ground,
        zenith: light.zenith,
        horizon: light.horizon,
        light_vp: light.light_vp.to_cols_array_2d(),
        shadow: light.shadow,
        skin: [if draw.skin == crate::drawlist::RIGID { 0.0 } else { 1.0 }, 0.0, 0.0, 0.0],
    }
}

/// The bone palettes' layout: one palette of uniform matrices at a dynamic
/// offset.
fn bones_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("model bones"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(PALETTE),
            },
            count: None,
        }],
    })
}

/// A bones buffer for `palettes` palettes, and its bind group.
fn bones_buffer(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, palettes: u64) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("model bones"),
        size: palettes * PALETTE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("model bones"),
        layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer: &buffer, offset: 0, size: NonZeroU64::new(PALETTE) }) }],
    });
    (buffer, group)
}

/// An authored model's vertices: position, normal, texture coordinate and
/// tangent (`model::VERTEX_FLOATS`).
fn model_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4,
        4 => Float32x4, 5 => Float32x4];
    wgpu::VertexBufferLayout { array_stride: (crate::model::VERTEX_FLOATS * 4) as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS }
}

fn model_pipeline(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, textures: &wgpu::BindGroupLayout, shadow: &wgpu::BindGroupLayout,
    bones: &wgpu::BindGroupLayout, samples: u32) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("model"),
        source: wgpu::ShaderSource::Wgsl(include_str!("model.wgsl").into()),
    });
    let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("model"),
        bind_group_layouts: &[Some(layout), Some(textures), Some(shadow), Some(bones)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("model"),
        layout: Some(&pipe_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_model"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(model_vertex_layout())],
        },
        primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
        depth_stencil: Some(depth_state(true)),
        multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_model"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn world_uniform(draw: &LitDraw, vp: Mat4, cam: Vec3, sun: Vec3, fog: Vec3, density: f32, time: f32, map: MapId) -> WorldUniform {
    let n = normal_columns(draw.model);
    let style = terrain::surface_style(map);
    WorldUniform {
        mvp: (vp * draw.model).to_cols_array_2d(),
        model: draw.model.to_cols_array_2d(),
        n0: n[0],
        n1: n[1],
        n2: n[2],
        cam: cam.to_array(),
        emit: draw.emit,
        sun: sun.to_array(),
        mode: draw.mode,
        fog: fog.to_array(),
        pad0: density,
        color: draw.color.to_array(),
        pad1: time,
        ground_cover: style.cover,
        ground_soil: style.soil,
        ground_rock: style.rock,
        ground_rules: style.rules,
        light_vp: Mat4::IDENTITY.to_cols_array_2d(),
    }
}

fn emit_uniform(draw: &EmitDraw, vp: Mat4, fog: [f32; 4]) -> EmitUniform {
    EmitUniform {
        mvp: (vp * draw.model).to_cols_array_2d(),
        color: draw.color,
        fog,
        params: [if draw.mesh == MeshId::Decal { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
    }
}

fn world_bind_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("world"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(WORLD_SIZE),
                },
                count: None,
            },
            texture_entry(1),
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            texture_entry(3),
        ],
    })
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn world_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    albedo: &wgpu::TextureView,
    normal: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("world"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: 0,
                    size: NonZeroU64::new(WORLD_SIZE),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(albedo),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(normal),
            },
        ],
    })
}

fn grass_textures(
    device: &wgpu::Device,
) -> (
    wgpu::Texture,
    wgpu::TextureView,
    wgpu::Texture,
    wgpu::TextureView,
    wgpu::Sampler,
) {
    let size = grass::TEX_SIZE;
    let mips = size.trailing_zeros() + 1;
    let make = |label: &str| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    };
    let albedo = make("grass-albedo");
    let normal = make("grass-normal");
    let albedo_view = albedo.create_view(&wgpu::TextureViewDescriptor::default());
    let normal_view = normal.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("grass"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: 8,
        ..Default::default()
    });
    (albedo, albedo_view, normal, normal_view, sampler)
}

fn upload_rgba_mips(queue: &wgpu::Queue, texture: &wgpu::Texture, base: &[u8], size: u32) {
    let mut mip = base.to_vec();
    let mut dim = size;
    let mut level = 0u32;
    loop {
        let row = dim * 4;
        let padded = row.div_ceil(256) * 256;
        let mut packed = vec![0u8; (padded * dim) as usize];
        for y in 0..dim as usize {
            let src = y * row as usize;
            let dst = y * padded as usize;
            packed[dst..dst + row as usize].copy_from_slice(&mip[src..src + row as usize]);
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &packed,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(dim),
            },
            wgpu::Extent3d {
                width: dim,
                height: dim,
                depth_or_array_layers: 1,
            },
        );
        if dim == 1 {
            break;
        }
        mip = downsample_rgba(&mip, dim);
        dim /= 2;
        level += 1;
    }
}

fn downsample_rgba(src: &[u8], size: u32) -> Vec<u8> {
    let n = size / 2;
    let mut dst = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            for c in 0..4u32 {
                let mut sum = 0u32;
                for dy in 0..2u32 {
                    for dx in 0..2u32 {
                        let sx = x * 2 + dx;
                        let sy = y * 2 + dy;
                        sum += src[((sy * size + sx) * 4 + c) as usize] as u32;
                    }
                }
                dst[((y * n + x) * 4 + c) as usize] = (sum / 4) as u8;
            }
        }
    }
    dst
}

fn uniform_layout(device: &wgpu::Device, label: &str, size: u64) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(size),
            },
            count: None,
        }],
    })
}

fn uniform_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    size: u64,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("uniform"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: NonZeroU64::new(size),
            }),
        }],
    })
}

fn make_uniform(device: &wgpu::Device, slots: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("uniforms"),
        size: slots as u64 * SLOT,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn blit_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    color_view: &wgpu::TextureView,
    bloom: &Bloom,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blit"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(color_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&bloom.levels[0].1) },
            wgpu::BindGroupEntry { binding: 3, resource: bloom.params.as_entire_binding() },
        ],
    })
}

fn make_target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    samples: u32,
) -> (
    wgpu::Texture,
    wgpu::TextureView,
    wgpu::Texture,
    wgpu::TextureView,
    Option<(wgpu::Texture, wgpu::TextureView)>,
) {
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene-color"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene-depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let msaa = (samples > 1).then(|| {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene-color-msaa"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    });
    (color, color_view, depth, depth_view, msaa)
}

fn lit_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    _emit: bool,
    _target: wgpu::TextureFormat,
    samples: u32,
) -> wgpu::RenderPipeline {
    let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("world"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("world"),
        layout: Some(&pipe_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_world"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vertex_layout())],
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(depth_state(true)),
        multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_world"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Effect pipelines share the emit uniform. `smoke` is the alpha-blended,
/// fogged variant that can darken the scene; the other stays additive.
fn emit_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    smoke: bool,
    samples: u32,
) -> wgpu::RenderPipeline {
    let blend = if smoke {
        wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        }
    } else {
        wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            // Alpha carries the bloom mask (glow = 1 - alpha): every additive
            // effect (flames, flashes, plasma, sparks) adds its strength to it.
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::ReverseSubtract,
            },
        }
    };
    let label = if smoke { "smoke" } else { "emit" };
    let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[None, None, Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipe_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_emit"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vertex_layout())],
        },
        primitive: wgpu::PrimitiveState {
            // No culling: the sphere mesh winds inward, and culling either
            // side leaves a hollow ring where a puff meets a wall.
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(depth_state(false)),
        multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(if smoke { "fs_smoke" } else { "fs_emit" }),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn sky_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    samples: u32,
) -> wgpu::RenderPipeline {
    let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sky"),
        bind_group_layouts: &[None, Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sky"),
        layout: Some(&pipe_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_fullscreen"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_sky"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn blit_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    target: wgpu::TextureFormat,
    entry: &str,
) -> wgpu::RenderPipeline {
    let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("blit"),
        bind_group_layouts: &[None, None, None, Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("blit"),
        layout: Some(&pipe_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_fullscreen"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn depth_state(write: bool) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth32Float,
        depth_write_enabled: Some(write),
        depth_compare: Some(wgpu::CompareFunction::Greater),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: 24,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 12,
                shader_location: 1,
            },
        ],
    }
}

fn upload(device: &wgpu::Device, label: &str, mesh: &(Vec<f32>, Vec<u16>)) -> Mesh {
    use wgpu::util::DeviceExt;
    let vbo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(&mesh.0),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ibo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(&mesh.1),
        usage: wgpu::BufferUsages::INDEX,
    });
    Mesh {
        vbo,
        ibo,
        count: mesh.1.len() as u32,
    }
}

fn cube_mesh() -> (Vec<f32>, Vec<u16>) {
    let faces: [([f32; 3], [f32; 3], [f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5], [0.0, 0.0, 1.0]),
        ([0.5, -0.5, -0.5], [-0.5, -0.5, -0.5], [-0.5, 0.5, -0.5], [0.5, 0.5, -0.5], [0.0, 0.0, -1.0]),
        ([-0.5, -0.5, -0.5], [-0.5, -0.5, 0.5], [-0.5, 0.5, 0.5], [-0.5, 0.5, -0.5], [-1.0, 0.0, 0.0]),
        ([0.5, -0.5, 0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [0.5, 0.5, 0.5], [1.0, 0.0, 0.0]),
        ([-0.5, 0.5, 0.5], [0.5, 0.5, 0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5], [0.0, 1.0, 0.0]),
        ([-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, -0.5, 0.5], [-0.5, -0.5, 0.5], [0.0, -1.0, 0.0]),
    ];
    let mut v = Vec::new();
    let mut i = Vec::new();
    for (fi, (a, b, c, d, n)) in faces.iter().enumerate() {
        let base = (fi * 4) as u16;
        for p in [a, b, c, d] {
            v.extend_from_slice(&[p[0], p[1], p[2], n[0], n[1], n[2]]);
        }
        i.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (v, i)
}

fn sphere_mesh(lat: u32, lon: u32) -> (Vec<f32>, Vec<u16>) {
    let mut v = Vec::new();
    let mut idx = Vec::new();
    for y in 0..=lat {
        let p = y as f32 / lat as f32;
        let th = p * std::f32::consts::PI;
        let sy = th.cos();
        let r = th.sin();
        for x in 0..=lon {
            let q = x as f32 / lon as f32;
            let ph = q * std::f32::consts::TAU;
            let px = r * ph.cos();
            let pz = r * ph.sin();
            v.extend_from_slice(&[px, sy, pz, px, sy, pz]);
        }
    }
    let stride = lon + 1;
    for y in 0..lat {
        for x in 0..lon {
            let i = (y * stride + x) as u16;
            let s = stride as u16;
            idx.extend_from_slice(&[i, i + s, i + 1, i + 1, i + s, i + s + 1]);
        }
    }
    (v, idx)
}

/// Chamfered rectangular extrusion, unit size, facing down local -Z.
fn bevel_mesh() -> (Vec<f32>, Vec<u16>) {
    let outline = [(-0.35, -0.5), (0.35, -0.5), (0.5, -0.3), (0.5, 0.3),
        (0.35, 0.5), (-0.35, 0.5), (-0.5, 0.3), (-0.5, -0.3)];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for i in 0..8 {
        let (x, y) = outline[i];
        let (xx, yy) = outline[(i + 1) % 8];
        let n = Vec3::new(yy - y, x - xx, 0.0).normalize();
        let b = (vertices.len() / 6) as u16;
        for p in [[x, y, -0.5], [xx, yy, -0.5], [xx, yy, 0.5], [x, y, 0.5]] {
            vertices.extend_from_slice(&[p[0], p[1], p[2], n.x, n.y, n.z]);
        }
        indices.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    for sign in [-1.0, 1.0] {
        let b = (vertices.len() / 6) as u16;
        vertices.extend_from_slice(&[0.0, 0.0, sign * 0.5, 0.0, 0.0, sign]);
        for (x, y) in outline {
            vertices.extend_from_slice(&[x, y, sign * 0.5, 0.0, 0.0, sign]);
        }
        for i in 0..8u16 {
            let a = b + 1 + i;
            let c = b + 1 + (i + 1) % 8;
            indices.extend_from_slice(&if sign > 0.0 { [b, a, c] } else { [b, c, a] });
        }
    }
    (vertices, indices)
}

/// Shared original armor primitive: inset end caps and bevels on all axes.
/// Flat face normals keep the low-poly silhouette crisp without block edges.
fn armor_mesh() -> (Vec<f32>, Vec<u16>) {
    let outline=[(-0.32,-0.5),(0.32,-0.5),(0.5,-0.32),(0.5,0.32),
        (0.32,0.5),(-0.32,0.5),(-0.5,0.32),(-0.5,-0.32)];
    let rings=[(-0.5,0.76),(-0.32,1.0),(0.32,1.0),(0.5,0.76)];
    let mut vertices=Vec::new(); let mut indices=Vec::new();
    let mut face=|points:&[Vec3]| {
        let n=(points[1]-points[0]).cross(points[2]-points[0]).normalize();
        let base=(vertices.len()/6) as u16;
        for p in points {vertices.extend_from_slice(&[p.x,p.y,p.z,n.x,n.y,n.z]);}
        for i in 1..points.len()-1 {indices.extend_from_slice(&[base,base+i as u16,base+i as u16+1]);}
    };
    for pair in rings.windows(2) {
        for i in 0..8 {
            let (x,y)=outline[i]; let (u,v)=outline[(i+1)%8];
            let (z,a)=pair[0]; let (zz,b)=pair[1];
            face(&[Vec3::new(x*a,y*a,z),Vec3::new(u*a,v*a,z),
                Vec3::new(u*b,v*b,zz),Vec3::new(x*b,y*b,zz)]);
        }
    }
    for sign in [-1.,1.] {
        let points:Vec<Vec3>=(0..8).map(|i| {
            let (x,y)=outline[if sign<0. {7-i} else {i}];
            Vec3::new(x*0.76,y*0.76,sign*0.5)
        }).collect();
        face(&points);
    }
    (vertices,indices)
}

/// Unit flat disc in the XZ plane facing +Y, top face only: a ground mark
/// must be a single layer, or the alpha-blended pass darkens it twice.
fn decal_mesh() -> (Vec<f32>, Vec<u16>) {
    let n = 24u16;
    let mut v = vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        v.extend_from_slice(&[a.cos(), 0.0, a.sin(), 0.0, 1.0, 0.0]);
    }
    let mut idx = Vec::new();
    for i in 0..n {
        idx.extend_from_slice(&[0, 1 + (i + 1) % n, 1 + i]);
    }
    (v, idx)
}

fn disc_mesh() -> (Vec<f32>, Vec<u16>) {
    let n = 24u16;
    let mut v = vec![0.0, 0.5, 0.0, 0.0, 1.0, 0.0];
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        v.extend_from_slice(&[a.cos(), 0.5, a.sin(), 0.0, 1.0, 0.0]);
    }
    let mut idx = Vec::new();
    for i in 0..n {
        let a = 1 + i;
        let b = 1 + (i + 1) % n;
        idx.extend_from_slice(&[0, b, a]);
    }
    let base = (n + 1) as usize;
    v.extend_from_slice(&[0.0, -0.5, 0.0, 0.0, -1.0, 0.0]);
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        v.extend_from_slice(&[a.cos(), -0.5, a.sin(), 0.0, -1.0, 0.0]);
    }
    let b0 = base as u16;
    for i in 0..n {
        let a = b0 + 1 + i;
        let c = b0 + 1 + (i + 1) % n;
        idx.extend_from_slice(&[b0, a, c]);
    }
    // Give the projectile a visible rim when seen edge-on.
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let b = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let base = (v.len() / 6) as u16;
        for (angle, y) in [(a, -0.5), (b, -0.5), (b, 0.5), (a, 0.5)] {
            v.extend_from_slice(&[angle.cos(), y, angle.sin(), angle.cos(), 0.0, angle.sin()]);
        }
        idx.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
    // A raised ridge so the spin is visible. A plain disc is a circle from every angle.
    let ridge = v.len() / 6;
    let bar = [
        [-0.12, 0.02, -0.16],
        [0.95, 0.02, -0.16],
        [0.95, 0.02, 0.16],
        [-0.12, 0.02, 0.16],
        [-0.12, 0.16, -0.1],
        [0.95, 0.16, -0.1],
        [0.95, 0.16, 0.1],
        [-0.12, 0.16, 0.1],
    ];
    for p in bar {
        v.extend_from_slice(&[p[0], p[1] + 0.5, p[2], 0.0, 1.0, 0.0]);
    }
    let r0 = ridge as u16;
    idx.extend_from_slice(&[
        r0, r0 + 1, r0 + 2, r0, r0 + 2, r0 + 3,
        r0 + 4, r0 + 6, r0 + 5, r0 + 4, r0 + 7, r0 + 6,
        r0, r0 + 4, r0 + 5, r0, r0 + 5, r0 + 1,
        r0 + 3, r0 + 2, r0 + 6, r0 + 3, r0 + 6, r0 + 7,
    ]);
    (v, idx)
}

pub struct SceneCallback {
    pub frame: DrawFrame,
    pub pixels: [u32; 2],
}

impl CallbackTrait for SceneCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(scene) = resources.get_mut::<SceneGpu>() {
            scene.render(device, queue, encoder, self.pixels[0], self.pixels[1], &self.frame);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        let Some(scene) = resources.get::<SceneGpu>() else {
            return;
        };
        scene.composite(pass);
        let _ = scene.target_is_srgb;
    }
}

/// 4x when the adapter can multisample and resolve the scene colour format and
/// multisample the depth format (WebGPU guarantees this; WebGL2 and some
/// native adapters may not), otherwise 1x.
fn supported_samples(adapter: &wgpu::Adapter) -> u32 {
    let color = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba8Unorm).flags;
    let depth = adapter.get_texture_format_features(wgpu::TextureFormat::Depth32Float).flags;
    if color.sample_count_supported(MSAA_SAMPLES)
        && color.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
        && depth.sample_count_supported(MSAA_SAMPLES)
    {
        MSAA_SAMPLES
    } else {
        1
    }
}

pub fn install(cc: &eframe::CreationContext<'_>) -> Result<(), String> {
    let state = cc
        .wgpu_render_state
        .as_ref()
        .ok_or_else(|| "PeakRunner needs the wgpu renderer".to_string())?;
    let mut scene = SceneGpu::new(&state.device, state.target_format);
    scene.set_max_samples(supported_samples(&state.adapter));
    state.renderer.write().callback_resources.insert(scene);
    Ok(())
}

const _: () = assert!(std::mem::size_of::<WorldUniform>() == WORLD_SIZE as usize);
const _: () = assert!(std::mem::size_of::<SkyUniform>() == SKY_SIZE as usize);
const _: () = assert!(std::mem::size_of::<EmitUniform>() == EMIT_SIZE as usize);

#[cfg(test)]
mod shader_check {
    #[test]
    fn old_holler_rain_is_off_and_valley_snow_is_preserved() {
        for count in [0, 128, 1024] {
            assert_eq!(super::precipitation_count(super::MapId::OldHoller, count), 0);
            assert_eq!(super::precipitation_count(super::MapId::Valley, count), count);
        }
    }
    /// Incoming fire in real bot fights, seen from the target: rendering the
    /// same frame with and without the rounds must never turn pixels dark.
    #[test]
    #[ignore = "requires a GPU adapter"]
    fn incoming_rounds_never_darken_the_view() {
        use super::*;
        use crate::drawlist::build_frame_with;
        use crate::effects::Effects;
        pollster::block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("GPU adapter");
            let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("GPU device");
            let mut scene = SceneGpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            let (width, height) = (640u32, 400u32);
            let read = |scene: &mut SceneGpu, frame: &crate::drawlist::DrawFrame| -> Vec<u8> {
                let mut encoder = device.create_command_encoder(&Default::default());
                scene.render(&device, &queue, &mut encoder, width, height, frame);
                let stride = (width * 4).div_ceil(256) * 256;
                let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("probe"), size: (stride * height) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
                encoder.copy_texture_to_buffer(scene.color.as_image_copy(),
                    wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout {
                        offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(height) } },
                    wgpu::Extent3d { width, height, depth_or_array_layers: 1 });
                queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| { tx.send(r).unwrap(); });
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                rx.recv().unwrap().unwrap();
                let mapped = buffer.slice(..).get_mapped_range().unwrap();
                mapped.chunks(stride as usize).flat_map(|row| row[..width as usize * 4].iter().copied()).collect()
            };
            let luma = |p: &[u8]| 0.3 * p[0] as f32 + 0.59 * p[1] as f32 + 0.11 * p[2] as f32;
            let label = std::env::var("PEAKRUNNER_CAPTURE_LABEL").unwrap_or_else(|_| "probe".into());
            std::fs::create_dir_all("screenshots").unwrap();
            let mut worst_total = 0usize;
            for map in [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold, MapId::Frostline, MapId::Dustreach] {
                let mut w = crate::sim::World::new();
                w.set_map(map);
                w.start_match(true);
                w.players[0].is_bot = true;
                let (mut shots, mut dark_frames, mut worst) = (0, 0, 0usize);
                for step in 0..(200.0 / 0.05) as u32 {
                    w.tick(0.05);
                    if step % 3 != 0 || shots >= 40 { continue; }
                    let mut chosen = None;
                    'find: for (pid, p) in w.players.iter().enumerate() {
                        if !p.alive { continue; }
                        let eye = p.pos + Vec3::Y * 1.6;
                        let look = Vec3::new(-p.yaw.sin() * p.pitch.cos(), p.pitch.sin(), -p.yaw.cos() * p.pitch.cos());
                        for d in &w.discs {
                            if d.team == p.team || peakrunner_core::registry::registry().look_kind(d.kind) == 2 { continue; }
                            let to = d.pos - eye;
                            let dist = to.length();
                            if dist < 2.0 || dist > 45.0 { continue; }
                            if d.vel.dot(-to) <= 0.0 || look.dot(to / dist) < 0.55 { continue; }
                            chosen = Some(pid); break 'find;
                        }
                    }
                    let Some(pid) = chosen else { continue; };
                    let keep = w.player_id;
                    w.player_id = pid;
                    let a = build_frame_with(&w, width as f32 / height as f32, 1.0 / 60.0, &mut Effects::new());
                    let discs = std::mem::take(&mut w.discs);
                    let b = build_frame_with(&w, width as f32 / height as f32, 1.0 / 60.0, &mut Effects::new());
                    w.discs = discs;
                    w.player_id = keep;
                    let (pa, pb) = (read(&mut scene, &a), read(&mut scene, &b));
                    let dark = pa.chunks_exact(4).zip(pb.chunks_exact(4)).filter(|(x, y)| luma(x) < 45.0 && luma(y) > 90.0).count();
                    shots += 1;
                    if shots == 1 || (map == MapId::TowerComplex && shots == 30) {
                        let path = format!("screenshots/{label}-{map:?}-sample{shots}.png");
                        let opaque: Vec<u8> = pa.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                        let mut enc = png::Encoder::new(std::fs::File::create(&path).unwrap(), width, height);
                        enc.set_color(png::ColorType::Rgba);
                        enc.set_depth(png::BitDepth::Eight);
                        enc.write_header().unwrap().write_image_data(&opaque).unwrap();
                    }
                    if dark > 0 {
                        dark_frames += 1;
                        if dark > worst {
                            worst = dark;
                            for (tag, px) in [("with", &pa), ("without", &pb)] {
                                let path = format!("screenshots/{label}-{map:?}-{tag}.png");
                                let opaque: Vec<u8> = px.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                                let mut enc = png::Encoder::new(std::fs::File::create(&path).unwrap(), width, height);
                                enc.set_color(png::ColorType::Rgba);
                                enc.set_depth(png::BitDepth::Eight);
                                enc.write_header().unwrap().write_image_data(&opaque).unwrap();
                            }
                        }
                    }
                }
                println!("{map:?}: incoming-fire frames {shots}, frames with darkened pixels {dark_frames}, worst {worst} px");
                worst_total = worst_total.max(worst);
            }
            println!("worst darkened pixels in any frame: {worst_total}");
            assert_eq!(worst_total, 0, "incoming rounds darkened the view; see screenshots/{label}-*");
        });
    }

    /// Real GPU render of the production scene, without opening a game window.
    /// Run with --ignored --nocapture; captures are for visual review, not golden tests.
    #[test]
    #[ignore = "requires a GPU adapter"]
    fn render_gameplay_captures() {
        use super::*;
        pollster::block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default())
                .await.expect("GPU adapter");
            let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default())
                .await.expect("GPU device");
            let mut scene = SceneGpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            let mut world = crate::sim::World::new();
            world.set_map(MapId::OldHoller);
            world.start_match(true);
            world.players.truncate(1);
            world.players[0].yaw = std::f32::consts::PI;
            world.players[0].pitch = -0.12;
            if let Ok(camera)=std::env::var("PEAKRUNNER_CAPTURE_CAMERA") {
                let v:Vec<f32>=camera.split(',').map(|s|s.parse().expect("camera number")).collect();
                assert_eq!(v.len(),5);
                world.players[0].pos=Vec3::new(v[0],v[1],v[2]);
                world.players[0].yaw=v[3];world.players[0].pitch=v[4];
            }
            let label = std::env::var("PEAKRUNNER_CAPTURE_LABEL").unwrap_or_else(|_| "current".into());
            std::fs::create_dir_all("screenshots").unwrap();
            for (name, width, height, cooldown) in [
                ("characters", 1280u32, 800u32, 0.0),
                ("characters-flight", 1280, 800, 0.0),
                ("desktop", 1280u32, 800u32, 0.0),
                ("portrait", 390, 844, 0.0),
                ("chat", 1280, 800, 0.0),
                ("chat-portrait", 390, 844, 0.0),
                ("menu-start", 1280, 800, 0.0),
                ("menu-quarter", 1280, 800, 0.0),
                ("menu-half", 1280, 800, 0.0),
                ("plasma", 1280, 800, 0.0),
                ("plasma-portrait", 390, 844, 0.0),
                ("disc-blast", 1280, 800, 0.0),
                ("grenade-blast", 1280, 800, 0.0),
                ("grenade-blast-portrait", 390, 844, 0.0),
                ("firing", 1280, 800, 0.98),
                ("reload-open", 1280, 800, 0.72),
                ("reload-feed", 1280, 800, 0.25),
                ("projectile", 1280, 800, 0.0),
                ("projectile-flight", 1280, 800, 0.0),
                ("chaingun", 1280, 800, 0.0),
                ("chaingun-portrait", 390, 844, 0.0),
                ("grenade", 1280, 800, 0.0),
                ("grenade-portrait", 390, 844, 0.0),
                ("chaingun-shot", 1280, 800, 0.0),
                ("grenade-shot", 1280, 800, 0.0),
            ] {
                if std::env::var_os("QA_CHARACTERS").is_some() && !name.starts_with("characters") { continue; }
                world.players.truncate(1);
                world.state=crate::sim::MatchState::Playing;
                if name.starts_with("menu") {
                    world.state=crate::sim::MatchState::Flyby;
                    world.flyby=if name.ends_with("half") {17.45} else if name.ends_with("quarter") {8.73} else {0.};
                }
                world.players[0].cooldown = cooldown;
                world.discs.clear();
                world.explosions.clear();
                if name.contains("blast") {
                    let kind=if name.starts_with("grenade") {2} else {0};
                    let profile=peakrunner_core::combat::player_weapon_blast(kind).unwrap();
                    let (eye,dir,_)=world.camera();
                    world.explosions.push(crate::sim::Explosion {pos:eye+dir*22.-Vec3::Y*5.,
                        age:0.16,kind,max_r:profile.radius});
                }
                if name.starts_with("plasma") {
                    let (eye,dir,_)=world.camera();
                    world.discs.push(crate::sim::Disc {pos:eye+dir*7.,vel:-dir*80.,
                        owner:usize::MAX,team:crate::sim::Team::Ember,kind:3,life:2.9,spin:0., target: 0, power: 1.0 });
                }
                world.players[0].weapon = if name.starts_with("chaingun") { 1 }
                    else if name.starts_with("grenade") { 2 } else { 0 };
                world.input.weapon = world.players[0].weapon;
                if name.starts_with("projectile") || name.ends_with("shot") {
                    // Clear the base flag so its pickup model cannot obscure
                    // the launch sequence when exercising the real game tick.
                    world.players[0].pos.y += 10.0;
                    world.input.fire = true;
                    world.tick(1.0 / 60.0);
                    world.input.fire = false;
                    if name == "projectile-flight" { world.tick(0.05); }
                    if name == "grenade-shot" {
                        for _ in 0..18 { world.tick(1.0 / 60.0); }
                    }
                    assert!(!world.discs.is_empty(), "capture a real fired round");
                }
                if name.starts_with("characters") {
                    let origin=world.players[0].pos;
                    for i in 0..3 {
                        let mut p=world.players[0].clone();
                        p.pos=origin+Vec3::new((i as f32-1.)*1.7,0.,0.);
                        p.yaw=[0.35,-0.6,2.8][i];
                        p.team=if i==1 {crate::sim::Team::Glacier} else {crate::sim::Team::Ember};
                        p.jetting=name.ends_with("flight"); p.skiing=i==1;
                        p.on_ground=!p.jetting;
                        world.players.push(p);
                    }
                }
                // Develop client-side effects (smoke, debris, sparks, barrel
                // spin, heat) up to the captured moment, as a live client would.
                let mut fx = crate::effects::Effects::new();
                let aspect = width as f32 / height as f32;
                if name.contains("blast") {
                    let target = world.explosions[0].age;
                    for k in 0..12 {
                        world.explosions[0].age = target * k as f32 / 12.0;
                        crate::drawlist::build_frame_with(&world, aspect, target / 12.0, &mut fx);
                    }
                    world.explosions[0].age = target;
                }
                if name == "chaingun-shot" {
                    let keep = world.players[0].cooldown;
                    for _ in 0..70 {
                        world.players[0].cooldown = 0.05;
                        crate::drawlist::build_frame_with(&world, aspect, 1.0 / 60.0, &mut fx);
                    }
                    world.players[0].cooldown = keep;
                }
                let mut frame = crate::drawlist::build_frame_with(&world, aspect, 0.0, &mut fx);
                if name.starts_with("characters") {
                    let target=world.players[0].pos+Vec3::Y*0.95;
                    frame.eye=target+Vec3::new(0.,0.5,-5.3);
                    frame.view=Mat4::look_at_rh(frame.eye,target,Vec3::Y);
                    frame.proj=crate::drawlist::clip_correct(Mat4::perspective_rh(49f32.to_radians(),width as f32/height as f32,0.1,2500.));
                    frame.inv_vp=(frame.proj*frame.view).inverse();
                    frame.viewmodel.clear();
                    frame.vm_models.clear();
                }
                let mut encoder = device.create_command_encoder(&Default::default());
                scene.render(&device, &queue, &mut encoder, width, height, &frame);
                if name.starts_with("chat") {
                    let ctx=egui::Context::default();
                    let mut app=crate::app::PeakRunnerApp::comms_fixture();
                    let mut renderer=eframe::egui_wgpu::Renderer::new(&device,wgpu::TextureFormat::Rgba8Unorm,Default::default());
                    // Areas need a sizing pass before their first visible frame.
                    for _ in 0..2 {
                        let mut output=ctx.run_ui(egui::RawInput { screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(width as f32,height as f32))), ..Default::default() }, |ui|app.capture_comms(ui.ctx()));
                        for (id,deltas) in &output.textures_delta.set {for delta in deltas {renderer.update_texture(&device,&queue,*id,delta);}}
                        output.textures_delta.clear();
                        let jobs=ctx.tessellate(output.shapes,1.0);
                        let screen=eframe::egui_wgpu::ScreenDescriptor {size_in_pixels:[width,height],pixels_per_point:1.0};
                        let commands=renderer.update_buffers(&device,&queue,&mut encoder,&jobs,&screen);
                        assert!(commands.is_empty());
                        let view=scene.color.create_view(&Default::default());
                        let pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label:Some("chat capture"),color_attachments:&[Some(wgpu::RenderPassColorAttachment {
                                view:&view,resolve_target:None,depth_slice:None,
                                ops:wgpu::Operations {load:wgpu::LoadOp::Load,store:wgpu::StoreOp::Store}
                            })],depth_stencil_attachment:None,timestamp_writes:None,occlusion_query_set:None,multiview_mask:None,
                        });
                        renderer.render(&mut pass.forget_lifetime(),&jobs,&screen);
                    }
                }
                let stride = (width * 4).div_ceil(256) * 256;
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("capture"), size: (stride * height) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                encoder.copy_texture_to_buffer(
                    scene.color.as_image_copy(),
                    wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout {
                        offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(height),
                    } },
                    wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                );
                queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| { tx.send(r).unwrap(); });
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                rx.recv().unwrap().unwrap();
                let mapped = buffer.slice(..).get_mapped_range().unwrap();
                let pixels: Vec<u8> = mapped.chunks(stride as usize)
                    .flat_map(|row| row[..width as usize * 4].iter().copied()).collect::<Vec<u8>>()
                    // Alpha carries the bloom mask; captures are opaque images.
                    .chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                assert!(pixels.chunks_exact(4).any(|p| p[0] > 80 && p[1] > 80), "blank render");
                let path = format!("screenshots/{label}-{name}.png");
                let mut encoder = png::Encoder::new(std::fs::File::create(&path).unwrap(), width, height);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
                println!("Rendered {path}");
            }
        });
    }

    #[test]
    fn bloom_settings_scale_with_the_level_and_off_is_off() {
        use super::{bloom_settings, BLOOM_LEVELS};
        assert_eq!(bloom_settings(0), (0, 0.0));
        let (low_levels, low) = bloom_settings(1);
        let (high_levels, high) = bloom_settings(2);
        assert!(low_levels > 0 && low_levels <= high_levels && high_levels <= BLOOM_LEVELS);
        assert!(low > 0.0 && low < high);
        // The blit reads the intensity as one vec4 uniform; the HUD is drawn
        // after the blit, so it is never part of the glow.
        let wgsl = include_str!("shaders.wgsl");
        assert!(wgsl.contains("var<uniform> bloom_u: vec4<f32>;"));
        assert!(wgsl.contains("fn composite("));
    }

    #[test]
    fn shaders_validate() {
        for source in [include_str!("shaders.wgsl"), include_str!("map.wgsl"), include_str!("bloom.wgsl"), include_str!("model.wgsl")] {
        let module = naga::front::wgsl::parse_str(source)
            .expect("wgsl parse");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator.validate(&module).expect("wgsl validate");
        }
    }

    /// A runner through the client's animation tracker over time, as
    /// numbered frames for a video: standing, a sprint, a strafe, a
    /// backpedal, a jump and its landing, a ski run and a stop, on Old
    /// Holler, the camera following (`CLIP_VIEW=side` tracks alongside
    /// instead of behind). `CLIP_DIR` (default `screenshots/clip`) gets
    /// `frame-0000.png`...; `CLIP_ARMOR` picks the armor. Run with --ignored
    /// --nocapture, then e.g. `ffmpeg -framerate 30 -i DIR/frame-%04d.png
    /// -pix_fmt yuv420p clip.mp4`.
    #[test]
    #[ignore = "requires a GPU adapter"]
    fn render_motion_clip() {
        use super::*;
        use crate::sim::{MatchState, World};
        pollster::block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("GPU adapter");
            let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("GPU device");
            let mut scene = SceneGpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            let (cw, ch) = (1280u32, 720u32);
            let dir = std::env::var("CLIP_DIR").unwrap_or_else(|_| "screenshots/clip".into());
            std::fs::create_dir_all(&dir).unwrap();
            let side = std::env::var("CLIP_VIEW").is_ok_and(|v| v == "side");
            let armor = std::env::var("CLIP_ARMOR").unwrap_or_else(|_| "medium".into());
            let mut w = World::new();
            w.set_map(MapId::OldHoller);
            w.start_match(true);
            w.state = MatchState::Playing;
            w.players.truncate(1);
            let start = crate::terrain::info(w.map).ember + Vec3::new(-40.0, 0.0, -70.0);
            w.players[0].pos = start + Vec3::new(0.0, 60.0, 600.0);
            let mut p = w.players[0].clone();
            p.net_id += 1;
            p.alive = true; p.carrying = None; p.cooldown = 0.0; p.weapon = 0; p.jetting = false;
            p.armor = crate::sim::ArmorClass::by_id(&armor).expect("CLIP_ARMOR");
            p.yaw = -std::f32::consts::FRAC_PI_2;
            w.players.push(p);
            let ground = |x: f32, z: f32| crate::terrain::height_on(MapId::OldHoller, x, z) + crate::terrain::PLAYER_RADIUS;
            let mut fx = crate::effects::Effects::new();
            let dt = 1.0 / 30.0;
            let (mut pos, mut vel, mut air_y, mut vy) = (start, Vec3::ZERO, 0.0f32, 0.0f32);
            let mut eye = start + Vec3::new(-4.0, 2.0, 2.0);
            for frame in 0..300 {
                let t = frame as f32 * dt;
                // The script: (target velocity in the world, skiing).
                let (want, ski) = match t {
                    t if t < 0.8 => (Vec3::ZERO, false),
                    t if t < 3.0 => (Vec3::new(11.0, 0.0, 0.0), false),
                    t if t < 4.2 => (Vec3::new(1.5, 0.0, 10.5), false),
                    t if t < 5.4 => (Vec3::new(-7.0, 0.0, 0.0), false),
                    t if t < 7.0 => (Vec3::new(11.0, 0.0, 0.0), false),
                    t if t < 8.6 => (Vec3::new(24.0, 0.0, 0.0), true),
                    _ => (Vec3::ZERO, false),
                };
                let accel = if ski { 30.0 } else { 78.0 };
                let dv = want - Vec3::new(vel.x, 0.0, vel.z);
                let step = dv.clamp_length_max(accel * dt);
                vel.x += step.x; vel.z += step.z;
                // A jump at 5.6 s: up, falling back under gravity.
                if (t - 5.6).abs() < dt * 0.5 { vy = 8.0; }
                if vy != 0.0 || air_y > 0.0 { air_y += vy * dt; vy -= 20.0 * dt; if air_y <= 0.0 { air_y = 0.0; vy = 0.0; } }
                pos += Vec3::new(vel.x, 0.0, vel.z) * dt;
                pos.y = ground(pos.x, pos.z) + air_y;
                let p = &mut w.players[1];
                p.pos = pos; p.vel = Vec3::new(vel.x, if air_y > 0.0 { vy } else { 0.0 }, vel.z);
                p.on_ground = air_y <= 0.0; p.skiing = ski && p.on_ground; p.yaw = -std::f32::consts::FRAC_PI_2;
                w.time = t;
                let mut f = crate::drawlist::build_frame_with(&w, cw as f32 / ch as f32, dt, &mut fx);
                // The body's middle, boots on the ground to the crown.
                let body = pos + Vec3::Y * 0.75;
                let want_eye = if side { body + Vec3::new(0.5, 0.5, 4.8) } else { body + Vec3::new(-3.9, 1.0, 1.25) };
                eye = if frame == 0 { want_eye } else { eye + (want_eye - eye) * 0.18 };
                f.eye = eye;
                f.view = Mat4::look_at_rh(eye, body + Vec3::new(if side { 0.0 } else { 2.0 }, 0.0, 0.0), Vec3::Y);
                f.proj = crate::drawlist::clip_correct(Mat4::perspective_rh(55f32.to_radians(), cw as f32 / ch as f32, 0.1, 2500.0));
                f.inv_vp = (f.proj * f.view).inverse();
                f.viewmodel.clear();
                f.vm_models.clear();
                let mut encoder = device.create_command_encoder(&Default::default());
                scene.render(&device, &queue, &mut encoder, cw, ch, &f);
                let stride = (cw * 4).div_ceil(256) * 256;
                let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("clip"), size: (stride * ch) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
                encoder.copy_texture_to_buffer(scene.color.as_image_copy(),
                    wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout {
                        offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(ch) } },
                    wgpu::Extent3d { width: cw, height: ch, depth_or_array_layers: 1 });
                queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| { tx.send(r).unwrap(); });
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                rx.recv().unwrap().unwrap();
                let mapped = buffer.slice(..).get_mapped_range().unwrap();
                let pixels: Vec<u8> = mapped.chunks(stride as usize).flat_map(|row| row[..cw as usize * 4].iter().copied())
                    .collect::<Vec<u8>>().chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                let path = format!("{dir}/frame-{frame:04}.png");
                let mut png = png::Encoder::new(std::fs::File::create(&path).unwrap(), cw, ch);
                png.set_color(png::ColorType::Rgba);
                png.set_depth(png::BitDepth::Eight);
                png.write_header().unwrap().write_image_data(&pixels).unwrap();
            }
            println!("wrote {dir}");
        });
    }

    /// One runner's stride as a film strip: eight moments an eighth of a
    /// stride apart, in reading order, facing right. `RUN_SPEED` sets the
    /// speed (8 m/s by default); `RUN_VIEW=front` the front three-quarter
    /// view and `RUN_VIEW=back` the over-the-shoulder view from behind
    /// instead of side-on; `RUN_MOVE=back`, `strafe` or `ski` backpedals,
    /// strafes right (toward the camera) or skis instead of running ahead;
    /// `RUN_ARMOR=light` or `heavy` changes the armor (medium by default).
    /// The body stands where the game puts it, its sphere on the ground.
    /// Writes `screenshots/players-run-cycle.png` (or `RUN_OUT`). Run with
    /// --ignored --nocapture.
    #[test]
    #[ignore = "requires a GPU adapter"]
    fn render_run_cycle() {
        use super::*;
        use crate::sim::{MatchState, World};
        pollster::block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("GPU adapter");
            let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("GPU device");
            let mut scene = SceneGpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            std::fs::create_dir_all("screenshots").unwrap();
            let (cw, ch) = (480u32, 600u32);
            let front = std::env::var("RUN_VIEW").is_ok_and(|v| v == "front");
            let mut render = |frame: &crate::drawlist::DrawFrame| -> Vec<u8> {
                let mut encoder = device.create_command_encoder(&Default::default());
                scene.render(&device, &queue, &mut encoder, cw, ch, frame);
                let stride = (cw * 4).div_ceil(256) * 256;
                let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("capture"), size: (stride * ch) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
                encoder.copy_texture_to_buffer(scene.color.as_image_copy(),
                    wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout {
                        offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(ch) } },
                    wgpu::Extent3d { width: cw, height: ch, depth_or_array_layers: 1 });
                queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| { tx.send(r).unwrap(); });
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                rx.recv().unwrap().unwrap();
                let mapped = buffer.slice(..).get_mapped_range().unwrap();
                mapped.chunks(stride as usize).flat_map(|row| row[..cw as usize * 4].iter().copied()).collect::<Vec<u8>>()
                    .chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect()
            };
            let mut w = World::new();
            w.set_map(MapId::OldHoller);
            w.start_match(true);
            w.state = MatchState::Playing;
            w.players.truncate(1);
            let spot = crate::terrain::info(w.map).ember + Vec3::new(0.0, 0.0, -70.0);
            let ground = crate::terrain::height_on(w.map, spot.x, spot.z);
            let origin = Vec3::new(spot.x, ground + crate::terrain::PLAYER_RADIUS, spot.z);
            w.players[0].pos = origin + Vec3::new(0.0, 30.0, 400.0);
            let template = w.players[0].clone();
            let speed: f32 = std::env::var("RUN_SPEED").ok().and_then(|s| s.parse().ok()).unwrap_or(8.0);
            let back_view = std::env::var("RUN_VIEW").is_ok_and(|v| v == "back");
            let moving = std::env::var("RUN_MOVE").unwrap_or_default();
            let armor = std::env::var("RUN_ARMOR").unwrap_or_else(|_| "medium".into());
            let (frames, cols) = (8u32, 4u32);
            let rows = frames / cols;
            let mut sheet = vec![0u8; (cw * cols * ch * rows * 4) as usize];
            // The runner's own id: one shared with the camera's player gets
            // the animation memory's stride instead of the time's.
            let runner = template.net_id + 1;
            let seed = runner as f32 * 0.7;
            {
                for k in 0..frames {
                    let (row, col) = (k / cols, k % cols);
                    w.players.truncate(1);
                    let mut p = template.clone();
                    p.pos = origin;
                    p.net_id = runner;
                    // Facing +X (screen right from the side camera).
                    p.yaw = -std::f32::consts::FRAC_PI_2; p.pitch = 0.0; p.weapon = 0;
                    p.alive = true; p.skiing = moving == "ski"; p.jetting = false; p.carrying = None; p.cooldown = 0.0;
                    p.armor = crate::sim::ArmorClass::by_id(&armor).expect("RUN_ARMOR");
                    p.vel = match moving.as_str() { "back" => Vec3::new(-speed, 0.0, 0.0), "strafe" => Vec3::new(0.0, 0.0, speed),
                        _ => Vec3::new(speed, 0.0, 0.0) };
                    p.on_ground = true;
                    w.players.push(p);
                    // The stride phase k/8 of the way round: untracked poses
                    // run at `CADENCE` from the player's seed.
                    let phase = k as f32 / frames as f32 * std::f32::consts::TAU;
                    w.time = ((phase - seed).rem_euclid(std::f32::consts::TAU) + std::f32::consts::TAU) / crate::player_model::CADENCE;
                    let mut frame = crate::drawlist::build_frame_with(&w, cw as f32 / ch as f32, 0.0, &mut crate::effects::Effects::new());
                    // The whole body in view, boots on the ground to the crown.
                    let target = origin + Vec3::Y * 0.6;
                    frame.eye = target + if front { Vec3::new(2.8, 0.7, 2.8) } else if back_view { Vec3::new(-3.8, 1.2, 0.7) } else { Vec3::new(0.0, 0.7, 4.0) };
                    frame.view = Mat4::look_at_rh(frame.eye, target, Vec3::Y);
                    frame.proj = crate::drawlist::clip_correct(Mat4::perspective_rh(40f32.to_radians(), cw as f32 / ch as f32, 0.1, 2500.0));
                    frame.inv_vp = (frame.proj * frame.view).inverse();
                    frame.viewmodel.clear();
                    frame.vm_models.clear();
                    let cell = render(&frame);
                    for y in 0..ch {
                        let to = (((row * ch + y) * cw * cols + col * cw) * 4) as usize;
                        let from = (y * cw * 4) as usize;
                        sheet[to..to + (cw * 4) as usize].copy_from_slice(&cell[from..from + (cw * 4) as usize]);
                    }
                }
            }
            let path = std::env::var("RUN_OUT").unwrap_or_else(|_| "screenshots/players-run-cycle.png".into());
            let path = path.as_str();
            let mut png = png::Encoder::new(std::fs::File::create(path).unwrap(), cw * cols, ch * rows);
            png.set_color(png::ColorType::Rgba);
            png.set_depth(png::BitDepth::Eight);
            png.write_header().unwrap().write_image_data(&sheet).unwrap();
            println!("wrote {path}");
        });
    }

    /// Tenfold Tower and the blade (`sim::blade`): the tower from the plaza,
    /// every floor, the blade in first person (ready, mid-slash, on guard,
    /// charging) and two fighting on the crown. Writes
    /// `$PEAKRUNNER_CAPTURE_DIR/blade-*.png` (default `screenshots/`). Run
    /// with --ignored --nocapture.
    #[test]
    #[ignore = "requires a GPU adapter"]
    fn render_blade_captures() {
        use super::*;
        use peakrunner_core::sim::blade::{self, State};
        let dir = std::env::var("PEAKRUNNER_CAPTURE_DIR").unwrap_or_else(|_| "screenshots".into());
        std::fs::create_dir_all(&dir).unwrap();
        let only = std::env::var("QA_ONLY").ok();
        pollster::block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("GPU adapter");
            let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("GPU device");
            let mut scene = SceneGpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            let map = peakrunner_core::sim::duel::lobby_map().expect("Tenfold Tower is installed");
            let zones = peakrunner_core::sim::features::zones(map);
            let axis = Vec3::from(zones[0].at);
            let floor = |k: usize| zones[k - 1].at[1] - zones[k - 1].size[1] + 0.3;
            let polar = |k: usize, r: f32, a: f32| {
                let a = a.to_radians();
                let at = Vec3::new(axis.x + r * a.sin(), floor(k) + 1.5, axis.z + r * a.cos());
                Vec3::new(at.x, peakrunner_core::terrain::support_on(map, at).0 + peakrunner_core::terrain::PLAYER_RADIUS, at.z)
            };
            let yaw_to = |from: Vec3, to: Vec3| { let d = to - from; (-d.x).atan2(-d.z) };
            let (width, height) = (1280u32, 800u32);
            let shoot = |scene: &mut SceneGpu, world: &crate::sim::World, camera: Option<(Vec3, Vec3)>, name: &str| {
                let mut fx = crate::effects::Effects::new();
                let aspect = width as f32 / height as f32;
                let mut frame = crate::drawlist::build_frame_with(world, aspect, 0.0, &mut fx);
                if let Some((eye, target)) = camera {
                    frame.eye = eye;
                    frame.view = Mat4::look_at_rh(eye, target, Vec3::Y);
                    frame.proj = crate::drawlist::clip_correct(Mat4::perspective_rh(68f32.to_radians(), aspect, 0.1, 2600.));
                    frame.inv_vp = (frame.proj * frame.view).inverse();
                    frame.viewmodel.clear();
                    frame.vm_models.clear();
                }
                let mut encoder = device.create_command_encoder(&Default::default());
                scene.render(&device, &queue, &mut encoder, width, height, &frame);
                let stride = (width * 4).div_ceil(256) * 256;
                let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("capture"), size: (stride * height) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
                encoder.copy_texture_to_buffer(scene.color.as_image_copy(),
                    wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(height) } },
                    wgpu::Extent3d { width, height, depth_or_array_layers: 1 });
                queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| { tx.send(r).unwrap(); });
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                rx.recv().unwrap().unwrap();
                let mapped = buffer.slice(..).get_mapped_range().unwrap();
                let pixels: Vec<u8> = mapped.chunks(stride as usize).flat_map(|row| row[..width as usize * 4].iter().copied()).collect::<Vec<u8>>()
                    .chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                let path = format!("{dir}/blade-{name}.png");
                let mut png = png::Encoder::new(std::fs::File::create(&path).unwrap(), width, height);
                png.set_color(png::ColorType::Rgba);
                png.set_depth(png::BitDepth::Eight);
                png.write_header().unwrap().write_image_data(&pixels).unwrap();
                println!("Rendered {path}");
            };
            let fresh = || {
                let mut w = crate::sim::World::new();
                w.set_map(map);
                w.set_mode(peakrunner_core::map_catalog::SupportedMode::DuelLobby);
                w.start_match(true);
                w.players.truncate(1);
                w.state = crate::sim::MatchState::Playing;
                w.players[0].net_id = 1;
                w
            };
            let wanted = |name: &str| only.as_deref().is_none_or(|o| name.contains(o));
            // The tower from the plaza.
            if wanted("outside") {
                let mut w = fresh();
                w.players[0].pos = axis + Vec3::new(0.0, -100.0, 0.0);
                w.third_person = true;
                shoot(&mut scene, &w, Some((axis + Vec3::new(-62.0, 18.0, 70.0), axis + Vec3::new(0.0, 38.0, 0.0))), "outside");
                shoot(&mut scene, &w, Some((axis + Vec3::new(26.0, 2.0, 30.0), axis + Vec3::new(0.0, 6.0, 0.0))), "entrance");
            }
            // Every floor, three ways: from near the wall across toward the
            // core (a), from another side (b), and from high by the wall (c).
            let at = |k: usize, r: f32, a: f32, up: f32| {
                let a = a.to_radians();
                Vec3::new(axis.x + r * a.sin(), floor(k) + up, axis.z + r * a.cos())
            };
            for k in 1..=10 {
                for (view, eye, target) in [
                    ("", at(k, if k == 7 { 15.0 } else { 16.5 }, 200.0, 2.6), at(k, 6.0, 330.0, 1.2)),
                    ("b", at(k, if k == 7 { 15.0 } else { 16.5 }, 80.0, 2.0), at(k, 6.0, 210.0, 1.4)),
                    ("c", at(k, 17.0, 300.0, if k == 10 { 8.0 } else { 6.6 }), at(k, 8.0, 130.0, 0.0)),
                ] {
                    let name = format!("floor{k:02}{view}");
                    if !wanted(&name) { continue; }
                    let mut w = fresh();
                    w.players[0].pos = axis + Vec3::new(0.0, -100.0, 0.0);
                    w.third_person = true;
                    shoot(&mut scene, &w, Some((eye, target)), &name);
                }
            }
            // The stair, looking up the spiral from the ground floor's landing.
            if wanted("stair") {
                let mut w = fresh();
                w.players[0].pos = axis + Vec3::new(0.0, -100.0, 0.0);
                w.third_person = true;
                shoot(&mut scene, &w, Some((Vec3::new(axis.x + 0.5, floor(1) + 1.7, axis.z + 9.0), Vec3::new(axis.x - 2.0, floor(1) + 5.5, axis.z))), "stair");
            }
            // First person: the blade at rest, mid-slash, on guard and charged.
            let def = blade::swing_def(blade::SLASH_LEFT).unwrap();
            let steps = |s: f32| (s / peakrunner_core::sim::STEP).round() as u16;
            for (name, b) in [
                ("fp-ready", State::default()),
                ("fp-slash", State { swing: blade::SLASH_LEFT, t: (steps(def.cut[0]) + steps(def.cut[1])) / 2, ..State::default() }),
                ("fp-chop", State { swing: blade::CHOP, t: steps(def.cut[0]) + 1, ..State::default() }),
                ("fp-guard", State { guard: true, ..State::default() }),
                ("fp-charge", State { press: 3, power: 230, held: 60, ..State::default() }),
            ] {
                if !wanted(name) { continue; }
                let mut w = fresh();
                w.players[0].pos = polar(10, 14.0, 90.0);
                w.players[0].yaw = yaw_to(w.players[0].pos, polar(10, 12.0, 140.0));
                w.players[0].pitch = -0.05;
                w.players[0].blade = b;
                shoot(&mut scene, &w, None, name);
            }
            // Two fighting on the crown: one slashing, one on guard; a third
            // charging behind them; seen from the side.
            if wanted("duel") {
                let mut w = fresh();
                w.players[0].pos = axis + Vec3::new(0.0, -100.0, 0.0);
                w.third_person = true;
                let (a, b) = (polar(10, 13.5, 120.0), polar(10, 13.5, 128.0));
                for (k, (pos, other, state)) in [(a, b, State { swing: blade::POWER, t: steps(blade::swing_def(blade::POWER).unwrap().cut[0]) + 4, power: 200, ..State::default() }),
                    (b, a, State { guard: true, ..State::default() }),
                    (polar(10, 11.0, 140.0), a, State { press: 3, power: 255, held: 80, ..State::default() })].into_iter().enumerate() {
                    let mut p = w.players[0].clone();
                    p.net_id = 10 + k as u32;
                    p.pos = pos;
                    p.yaw = yaw_to(pos, other);
                    p.team = if k % 2 == 0 { crate::sim::Team::Ember } else { crate::sim::Team::Glacier };
                    p.blade = state;
                    p.on_ground = true;
                    w.players.push(p);
                }
                let mid = (a + b) * 0.5 + Vec3::Y * 0.6;
                let side = (b - a).cross(Vec3::Y).normalize();
                shoot(&mut scene, &w, Some((mid + side * 4.2 + Vec3::Y * 0.9, mid)), "duel");
            }
            // A staged shot (the website's picture of the tower):
            // QA_STAGE="floor,r,a,eye r,eye a,eye up,look r,look a,look up"
            // puts two fighters mid-clash at (r, a) on that floor, one
            // slashing and one on guard, and the camera at the eye looking at
            // the look point (polar, up from the floor), as the QA fly-camera
            // so the frame sees them from there. Writes blade-stage.png.
            if let Some(stage) = std::env::var("QA_STAGE").ok().filter(|_| wanted("stage")) {
                let v: Vec<f32> = stage.split(',').map(|x| x.trim().parse().expect("QA_STAGE numbers")).collect();
                let k = v[0] as usize;
                let mut w = fresh();
                w.players[0].pos = axis + Vec3::new(0.0, -100.0, 0.0);
                w.third_person = true;
                let (a, b) = (polar(k, v[1], v[2] - 5.0), polar(k, v[1], v[2] + 5.0));
                let slash = blade::swing_def(blade::SLASH_RIGHT).unwrap();
                for (n, (pos, other, state)) in [(a, b, State { swing: blade::SLASH_RIGHT, t: (steps(slash.cut[0]) + steps(slash.cut[1])) / 2, ..State::default() }),
                    (b, a, State { guard: true, ..State::default() })].into_iter().enumerate() {
                    let mut p = w.players[0].clone();
                    p.net_id = 10 + n as u32;
                    p.pos = pos;
                    p.yaw = yaw_to(pos, other);
                    p.team = if n == 0 { crate::sim::Team::Ember } else { crate::sim::Team::Glacier };
                    p.blade = state;
                    p.on_ground = true;
                    w.players.push(p);
                }
                let at = |r: f32, deg: f32, up: f32| { let t = deg.to_radians(); Vec3::new(axis.x + r * t.sin(), floor(k) + up, axis.z + r * t.cos()) };
                let (eye, look) = (at(v[3], v[4], v[5]), at(v[6], v[7], v[8]));
                let d = (look - eye).normalize();
                std::env::set_var("QA_FLYCAM", format!("{},{},{},{},{},68", eye.x, eye.y, eye.z, (-d.x).atan2(-d.z).to_degrees(), d.y.asin().to_degrees()));
                shoot(&mut scene, &w, None, "stage");
                std::env::remove_var("QA_FLYCAM");
            }
        });
    }

    /// Articulated player poses and first-person hands. Writes
    /// `screenshots/players-*.png`. Run with --ignored --nocapture.
    #[test]
    #[ignore = "requires a GPU adapter"]
    fn render_player_captures() {
        use super::*;
        use crate::sim::{MatchState, Team, World};
        pollster::block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("GPU adapter");
            let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("GPU device");
            let mut scene = SceneGpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            std::fs::create_dir_all("screenshots").unwrap();
            let (width, height) = (1280u32, 800u32);
            let aspect = width as f32 / height as f32;
            let mut save = |frame: &crate::drawlist::DrawFrame, name: &str| {
                let mut encoder = device.create_command_encoder(&Default::default());
                scene.render(&device, &queue, &mut encoder, width, height, frame);
                let stride = (width * 4).div_ceil(256) * 256;
                let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("capture"), size: (stride * height) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
                encoder.copy_texture_to_buffer(scene.color.as_image_copy(),
                    wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout {
                        offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(height) } },
                    wgpu::Extent3d { width, height, depth_or_array_layers: 1 });
                queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| { tx.send(r).unwrap(); });
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                rx.recv().unwrap().unwrap();
                let mapped = buffer.slice(..).get_mapped_range().unwrap();
                let pixels: Vec<u8> = mapped.chunks(stride as usize).flat_map(|row| row[..width as usize * 4].iter().copied()).collect::<Vec<u8>>()
                    // Alpha carries the bloom mask; captures are opaque images.
                    .chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                let path = format!("screenshots/players-{name}.png");
                let mut png = png::Encoder::new(std::fs::File::create(&path).unwrap(), width, height);
                png.set_color(png::ColorType::Rgba);
                png.set_depth(png::BitDepth::Eight);
                png.write_header().unwrap().write_image_data(&pixels).unwrap();
                println!("wrote {path}");
            };
            let mut w = World::new();
            w.set_map(MapId::OldHoller);
            w.start_match(true);
            w.state = MatchState::Playing;
            w.players.truncate(1);
            // Stage on open ground in front of the Ember base.
            let spot = crate::terrain::info(w.map).ember + Vec3::new(0.0, 0.0, -70.0);
            let ground = crate::terrain::height_on(w.map, spot.x, spot.z);
            // Where play puts a standing player: the collision sphere on the ground.
            let origin = Vec3::new(spot.x, ground + crate::terrain::PLAYER_RADIUS, spot.z);
            w.players[0].pos = origin + Vec3::new(0.0, 30.0, 40.0);
            let template = w.players[0].clone();
            // (name, [(offset, yaw, setup)]) and a camera offset.
            type Setup = fn(&mut crate::sim::Player);
            let stand: Setup = |p| { p.vel = Vec3::ZERO; p.on_ground = true; };
            let run: Setup = |p| { p.vel = Vec3::new(0.0, 0.0, -8.0); p.on_ground = true; };
            let ski: Setup = |p| { p.vel = Vec3::new(9.0, -3.0, -28.0); p.on_ground = true; p.skiing = true; };
            let jet: Setup = |p| { p.vel = Vec3::new(0.0, 6.0, -18.0); p.on_ground = false; p.jetting = true; };
            let air: Setup = |p| { p.vel = Vec3::new(0.0, -9.0, -12.0); p.on_ground = false; };
            let fire: Setup = |p| { p.on_ground = true; p.vel = Vec3::ZERO; p.cooldown = crate::sim::weapon_reload(p.weapon) - 0.03; };
            let carry: Setup = |p| { p.on_ground = true; p.vel = Vec3::new(0.0, 0.0, -9.0); p.carrying = Some(Team::Glacier); };
            let dead: Setup = |p| { p.alive = false; p.respawn = 3.4 - 1.2; };
            let shots: Vec<(&str, Vec<(Vec3, f32, u8, Team, Setup)>, f32)> = vec![
                ("lineup", vec![(Vec3::new(-1.8, 0.0, 0.0), 0.35, 0, Team::Ember, stand),
                    (Vec3::ZERO, -0.2, 1, Team::Glacier, stand), (Vec3::new(1.8, 0.0, 0.0), 0.5, 2, Team::Ember, stand)], 0.0),
                ("run", vec![(Vec3::ZERO, -1.4, 0, Team::Ember, run), (Vec3::new(2.0, 0.0, 0.5), 1.3, 1, Team::Glacier, run)], 0.12),
                ("ski", vec![(Vec3::ZERO, -1.3, 0, Team::Glacier, ski)], 0.0),
                ("jet", vec![(Vec3::new(0.0, 1.2, 0.0), -1.2, 0, Team::Ember, jet)], 0.0),
                ("airborne", vec![(Vec3::new(0.0, 0.8, 0.0), 1.4, 2, Team::Glacier, air)], 0.0),
                ("fire-disc", vec![(Vec3::ZERO, 0.9, 0, Team::Ember, fire)], 0.0),
                ("fire-chaingun", vec![(Vec3::ZERO, 0.9, 1, Team::Glacier, fire)], 0.0),
                ("fire-grenade", vec![(Vec3::ZERO, 0.9, 2, Team::Ember, fire)], 0.0),
                ("flag-carrier", vec![(Vec3::ZERO, 2.4, 1, Team::Ember, carry)], 0.3),
                ("death", vec![(Vec3::ZERO, -0.8, 0, Team::Glacier, dead)], 0.0),
            ];
            for (name, cast, time) in shots {
                w.players.truncate(1);
                w.time = time;
                for (offset, yaw, weapon, team, setup) in cast {
                    let mut p = template.clone();
                    p.pos = origin + offset;
                    p.yaw = yaw; p.pitch = 0.0; p.weapon = weapon; p.team = team;
                    p.alive = true; p.skiing = false; p.jetting = false; p.carrying = None; p.cooldown = 0.0;
                    setup(&mut p);
                    w.players.push(p);
                }
                let mut frame = crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut crate::effects::Effects::new());
                let target = origin + Vec3::Y * 0.75;
                // Front three-quarter for the lineup and fire poses, side-on
                // (from +Z) for movement poses.
                let front = name == "lineup" || name.starts_with("fire") || name == "death";
                frame.eye = target + if front { Vec3::new(-1.6, 0.6, -4.3) } else { Vec3::new(0.6, 0.55, 4.6) };
                frame.view = Mat4::look_at_rh(frame.eye, target, Vec3::Y);
                frame.proj = crate::drawlist::clip_correct(Mat4::perspective_rh(48f32.to_radians(), aspect, 0.1, 2500.0));
                frame.inv_vp = (frame.proj * frame.view).inverse();
                frame.viewmodel.clear();
                frame.vm_models.clear();
                save(&frame, name);
            }
            // Close-ups of the head and face: Ember and Glacier side by side,
            // front three-quarter and profile.
            for (name, eye) in [("portrait", Vec3::new(-0.75, 0.08, -1.5)), ("profile", Vec3::new(-1.7, 0.05, -0.2)),
                ("face", Vec3::new(-0.22, 0.12, -0.62))] {
                w.players.truncate(1);
                w.time = 0.0;
                // The level of detail follows the game camera: bring it close.
                w.players[0].pos = origin + Vec3::new(0.0, 0.0, -6.0);
                for (i, team) in [Team::Ember, Team::Glacier].into_iter().enumerate() {
                    let mut p = template.clone();
                    p.pos = origin + Vec3::new(i as f32 * 0.9, 0.0, i as f32 * 0.5);
                    p.net_id = 7 + i as u32 * 5;
                    p.yaw = -0.25; p.pitch = 0.0; p.weapon = 0; p.team = team;
                    p.alive = true; p.carrying = None; p.cooldown = 0.0;
                    stand(&mut p);
                    w.players.push(p);
                }
                let mut frame = crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut crate::effects::Effects::new());
                let target = origin + if name == "face" { Vec3::new(0.02, 1.68, -0.1) } else { Vec3::new(0.2, 1.55, 0.0) };
                frame.eye = target + eye;
                frame.view = Mat4::look_at_rh(frame.eye, target, Vec3::Y);
                frame.proj = crate::drawlist::clip_correct(Mat4::perspective_rh(40f32.to_radians(), aspect, 0.05, 2500.0));
                frame.inv_vp = (frame.proj * frame.view).inverse();
                frame.viewmodel.clear();
                frame.vm_models.clear();
                save(&frame, name);
            }
            // Rabbit: the rabbit in bright gold between two hunters, one of
            // each hidden team, both in the hunters' steel.
            {
                let mode = w.mode;
                let r = &mut w;
                r.set_mode(peakrunner_core::map_catalog::SupportedMode::Rabbit);
                r.state = MatchState::Playing;
                r.players.truncate(1);
                for (k, (offset, yaw, weapon, team)) in [(Vec3::new(-1.8, 0.0, 0.0), 0.35, 0u8, Team::Ember), (Vec3::ZERO, -0.2, 1, Team::Glacier),
                    (Vec3::new(1.8, 0.0, 0.0), 0.5, 2, Team::Glacier)].into_iter().enumerate() {
                    let mut p = template.clone();
                    p.pos = origin + offset;
                    p.yaw = yaw; p.pitch = 0.0; p.weapon = weapon; p.team = team;
                    p.alive = true; p.on_ground = true; p.vel = Vec3::ZERO; p.carrying = None; p.cooldown = 0.0;
                    p.net_id = 30 + k as u32;
                    r.players.push(p);
                }
                r.rabbit.active = true;
                r.rabbit.carrier = Some(2);
                let mut frame = crate::drawlist::build_frame_with(r, aspect, 0.0, &mut crate::effects::Effects::new());
                let target = origin + Vec3::Y * 0.75;
                frame.eye = target + Vec3::new(-1.6, 0.6, -4.3);
                frame.view = Mat4::look_at_rh(frame.eye, target, Vec3::Y);
                frame.proj = crate::drawlist::clip_correct(Mat4::perspective_rh(48f32.to_radians(), aspect, 0.1, 2500.0));
                frame.inv_vp = (frame.proj * frame.view).inverse();
                frame.viewmodel.clear();
                frame.vm_models.clear();
                save(&frame, "rabbit-lineup");
                // And the same from 110 m, behind them.
                for p in r.players.iter_mut().skip(1) { p.yaw += std::f32::consts::PI; }
                r.players[0].pos = template.pos;
                let mut frame = crate::drawlist::build_frame_with(r, aspect, 0.0, &mut crate::effects::Effects::new());
                frame.eye = target + Vec3::new(12.0, 55.0, -95.0);
                frame.view = Mat4::look_at_rh(frame.eye, target, Vec3::Y);
                frame.proj = crate::drawlist::clip_correct(Mat4::perspective_rh(8f32.to_radians(), aspect, 0.1, 2500.0));
                frame.inv_vp = (frame.proj * frame.view).inverse();
                frame.viewmodel.clear();
                frame.vm_models.clear();
                save(&frame, "rabbit-far");
                r.rabbit.active = false;
                r.rabbit.carrier = None;
                r.set_mode(mode);
                r.state = MatchState::Playing;
            }
            // Football, empty-handed: a carrier with the ball tucked under
            // the right arm, a runner with free hands, a thrower just after
            // the release, and a body still down after its player respawned.
            {
                use crate::sim::MatchState;
                let mode = w.mode;
                let r = &mut w;
                r.set_mode(peakrunner_core::map_catalog::SupportedMode::Football);
                r.state = MatchState::Playing;
                r.players.truncate(1);
                r.players[0].pos = template.pos;
                let cast: [(Vec3, f32, Vec3, f32); 4] = [(Vec3::new(-2.6, 0.0, 0.0), -1.4, Vec3::new(0.0, 0.0, -8.0), 0.0),
                    (Vec3::new(0.0, 0.0, -0.4), -1.4, Vec3::new(0.0, 0.0, -8.0), 0.0), (Vec3::new(2.4, 0.0, 0.3), -1.2, Vec3::ZERO, 0.4),
                    (Vec3::new(0.6, 0.0, 2.6), 0.4, Vec3::ZERO, 0.0)];
                for (k, (offset, yaw, vel, cooldown)) in cast.into_iter().enumerate() {
                    let mut p = template.clone();
                    p.pos = origin + offset; p.yaw = yaw; p.pitch = 0.0; p.vel = vel; p.on_ground = true;
                    p.alive = true; p.cooldown = cooldown; p.team = if k == 1 { Team::Glacier } else { Team::Ember };
                    p.net_id = 40 + k as u32;
                    r.players.push(p);
                }
                r.ball.active = true;
                r.ball.in_play = true;
                r.ball.carrier = Some(1);
                let mut fx = crate::effects::Effects::new();
                let body = r.players.len() - 1;
                for f in 0..150 {
                    // The last goes down, lies a second and a half, then respawns far off.
                    if f == 10 { r.players[body].alive = false; r.players[body].respawn = 1.0; }
                    if f == 100 { r.players[body].alive = true; r.players[body].pos = origin + Vec3::new(0.0, 0.0, -80.0); }
                    for p in r.players.iter_mut().skip(1) { if p.alive { p.cooldown = if p.net_id == 42 { 0.4 } else { 0.0 }; } }
                    crate::drawlist::build_frame_with(r, aspect, 1.0 / 60.0, &mut fx);
                }
                let mut frame = crate::drawlist::build_frame_with(r, aspect, 0.0, &mut fx);
                let target = origin + Vec3::new(0.0, 0.75, 0.6);
                frame.eye = target + Vec3::new(1.2, 0.9, 7.2);
                frame.view = Mat4::look_at_rh(frame.eye, target, Vec3::Y);
                frame.proj = crate::drawlist::clip_correct(Mat4::perspective_rh(50f32.to_radians(), aspect, 0.1, 2500.0));
                frame.inv_vp = (frame.proj * frame.view).inverse();
                frame.viewmodel.clear();
                frame.vm_models.clear();
                save(&frame, "football-empty-hands");
                r.ball = Default::default();
                r.set_mode(mode);
                r.state = MatchState::Playing;
            }
            // Far LOD: the same lineup from 110 m.
            w.players.truncate(1);
            w.players[0].pos = template.pos;
            for (i, weapon) in [0u8, 1, 2].into_iter().enumerate() {
                let mut p = template.clone();
                p.pos = origin + Vec3::new((i as f32 - 1.0) * 1.8, 0.0, 0.0);
                p.yaw = 2.6; p.weapon = weapon; p.alive = true; p.on_ground = true; p.vel = Vec3::new(0.0, 0.0, -8.0);
                p.carrying = if i == 1 { Some(Team::Glacier) } else { None };
                w.players.push(p);
            }
            let mut frame = crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut crate::effects::Effects::new());
            let target = origin + Vec3::Y * 0.75;
            frame.eye = target + Vec3::new(12.0, 55.0, -95.0);
            frame.view = Mat4::look_at_rh(frame.eye, target, Vec3::Y);
            frame.proj = crate::drawlist::clip_correct(Mat4::perspective_rh(8f32.to_radians(), aspect, 0.1, 2500.0));
            frame.inv_vp = (frame.proj * frame.view).inverse();
            frame.viewmodel.clear();
            frame.vm_models.clear();
            save(&frame, "far-lod");
            // First-person viewmodels with hands.
            w.players.truncate(1);
            w.players[0].pos = origin;
            w.players[0].yaw = 0.2;
            w.players[0].pitch = -0.05;
            for (weapon, name) in [(0u8, "viewmodel-disc"), (1, "viewmodel-chaingun"), (2, "viewmodel-grenade")] {
                w.players[0].weapon = weapon;
                w.input.weapon = weapon;
                w.players[0].cooldown = 0.0;
                let mut fx = crate::effects::Effects::new();
                // Settle the switch animation.
                for _ in 0..40 { crate::drawlist::build_frame_with(&w, aspect, 1.0 / 60.0, &mut fx); }
                let frame = crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut fx);
                save(&frame, name);
            }
        });
    }

    /// Authored models (`assets/models/`): the three armor sizes on both
    /// teams, close and from range, and every weapon that has a model, in
    /// the hands and in first person. Writes `screenshots/models-*.png`.
    /// Run with --ignored --nocapture; `PEAKRUNNER_MODELS=0` for the
    /// built-in parts, `PEAKRUNNER_CAPTURE_MOD=sparkfall` for a mod's
    /// weapons too (from `examples/mods`; run this test alone then: a mod
    /// changes the whole process's definitions).
    #[test]
    #[ignore = "requires a GPU adapter"]
    fn render_model_captures() {
        use super::*;
        use crate::sim::{ArmorClass, MatchState, Team, World};
        // A mod's content root goes in before anything reads the roots.
        let mode = std::env::var("PEAKRUNNER_CAPTURE_MOD").ok().map(|id| {
            peakrunner_core::catalog::add_root(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples"));
            peakrunner_core::mods::activate_home(&[id.clone()]).unwrap();
            peakrunner_core::map_catalog::SupportedMode::parse(&format!("{id}-ctf")).expect("the mod's CTF mode")
        });
        pollster::block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("GPU adapter");
            let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("GPU device");
            let mut scene = SceneGpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            std::fs::create_dir_all("screenshots").unwrap();
            let (width, height) = (1280u32, 800u32);
            let aspect = width as f32 / height as f32;
            let tag = if crate::model::enabled() { "models" } else { "models-off" };
            // A crosshair drawn into the next capture, at offsets from the
            // center in half screen heights (the HUD's third-person aim).
            let marker: std::cell::Cell<Option<[f32; 2]>> = std::cell::Cell::new(None);
            let mut save = |frame: &crate::drawlist::DrawFrame, name: &str| {
                let mut encoder = device.create_command_encoder(&Default::default());
                scene.render(&device, &queue, &mut encoder, width, height, frame);
                let stride = (width * 4).div_ceil(256) * 256;
                let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("capture"), size: (stride * height) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
                encoder.copy_texture_to_buffer(scene.color.as_image_copy(),
                    wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout {
                        offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(height) } },
                    wgpu::Extent3d { width, height, depth_or_array_layers: 1 });
                queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| { tx.send(r).unwrap(); });
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                rx.recv().unwrap().unwrap();
                let mapped = buffer.slice(..).get_mapped_range().unwrap();
                let pixels: Vec<u8> = mapped.chunks(stride as usize).flat_map(|row| row[..width as usize * 4].iter().copied()).collect::<Vec<u8>>()
                    .chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                let mut pixels = pixels;
                if let Some([x, y]) = marker.take() {
                    let (cx, cy) = (width as f32 * 0.5 + x * height as f32 * 0.5, height as f32 * 0.5 - y * height as f32 * 0.5);
                    for d in -14i32..=14 {
                        if d.abs() < 5 { continue; }
                        for (px, py, w) in [(cx as i32 + d, cy as i32, 1i32), (cx as i32, cy as i32 + d, 1)] {
                            for (ox, oy, color) in [(0, -2 * w, [0u8, 0, 0]), (0, 2 * w, [0, 0, 0]), (-2, 0, [0, 0, 0]), (2, 0, [0, 0, 0]),
                                (0, -1, [255, 255, 255]), (0, 0, [255, 255, 255]), (0, 1, [255, 255, 255]), (-1, 0, [255, 255, 255]), (1, 0, [255, 255, 255])] {
                                let (qx, qy) = (px + ox, py + oy);
                                if qx < 0 || qy < 0 || qx >= width as i32 || qy >= height as i32 { continue; }
                                let k = (qy as usize * width as usize + qx as usize) * 4;
                                pixels[k..k + 3].copy_from_slice(&color);
                            }
                        }
                    }
                }
                let path = format!("screenshots/{tag}-{name}.png");
                let mut png = png::Encoder::new(std::fs::File::create(&path).unwrap(), width, height);
                png.set_color(png::ColorType::Rgba);
                png.set_depth(png::BitDepth::Eight);
                png.write_header().unwrap().write_image_data(&pixels).unwrap();
                println!("wrote {path}");
            };
            let mut w = World::new();
            w.set_map(MapId::OldHoller);
            if let Some(mode) = mode { w.set_mode(mode); }
            w.start_match(true);
            w.state = MatchState::Playing;
            w.players.truncate(1);
            let spot = crate::terrain::info(w.map).ember + Vec3::new(0.0, 0.0, -70.0);
            let ground = crate::terrain::height_on(w.map, spot.x, spot.z);
            // Where play puts a standing player: the collision sphere on the ground.
            let origin = Vec3::new(spot.x, ground + crate::terrain::PLAYER_RADIUS, spot.z);
            w.players[0].pos = origin + Vec3::new(0.0, 30.0, 40.0);
            let template = w.players[0].clone();
            let registry = peakrunner_core::registry::registry();
            let weapon = |id: &str| registry.weapons.index(id).unwrap() as u8;
            let sizes = ["light", "medium", "heavy"];
            let stage = |w: &mut World, team_of: &dyn Fn(usize) -> Team, gun: u8| {
                w.players.truncate(1);
                for (i, size) in sizes.iter().enumerate() {
                    let mut p = template.clone();
                    p.pos = origin + Vec3::new((i as f32 - 1.0) * 1.9, 0.0, 0.0);
                    p.armor = ArmorClass::by_id(size).unwrap();
                    p.net_id = 3 + i as u32 * 4;
                    p.yaw = 0.25; p.pitch = 0.0; p.weapon = gun; p.team = team_of(i);
                    p.loadout = crate::sim::arsenal::Loadout::of(&[gun]);
                    p.alive = true; p.on_ground = true; p.vel = Vec3::ZERO; p.carrying = None; p.cooldown = 0.0;
                    w.players.push(p);
                }
            };
            let shoot = |w: &World, eye: Vec3, target: Vec3, fov: f32| {
                let mut frame = crate::drawlist::build_frame_with(w, aspect, 0.0, &mut crate::effects::Effects::new());
                frame.eye = eye;
                frame.view = Mat4::look_at_rh(eye, target, Vec3::Y);
                frame.proj = crate::drawlist::clip_correct(Mat4::perspective_rh(fov.to_radians(), aspect, 0.1, 2500.0));
                frame.inv_vp = (frame.proj * frame.view).inverse();
                frame.viewmodel.clear();
                frame.vm_models.clear();
                frame
            };
            let target = origin + Vec3::Y * 0.75;
            // The three sizes, front and back, each team.
            stage(&mut w, &|_| Team::Ember, weapon("chaingun"));
            save(&shoot(&w, target + Vec3::new(-1.2, 0.5, -5.6), target, 46.0), "armor-front-ember");
            stage(&mut w, &|_| Team::Glacier, weapon("chaingun"));
            save(&shoot(&w, target + Vec3::new(1.4, 0.8, 5.6), target, 46.0), "armor-back-glacier");
            // Each size close: front three-quarter and from behind.
            for (i, size) in sizes.iter().enumerate() {
                let at = origin + Vec3::new((i as f32 - 1.0) * 1.9, 1.2, 0.0);
                stage(&mut w, &|_| Team::Ember, weapon("chaingun"));
                save(&shoot(&w, at + Vec3::new(-1.0, 0.3, -2.6), at, 38.0), &format!("armor-{size}-front"));
                save(&shoot(&w, at + Vec3::new(1.2, 0.5, 2.6), at, 38.0), &format!("armor-{size}-back"));
                save(&shoot(&w, at + Vec3::new(0.35, 0.45, -1.0), at + Vec3::Y * 0.45, 30.0), &format!("armor-{size}-head"));
            }
            // In the rain: the weather's draws share the pass with the models.
            w.conditions.weather = peakrunner_core::conditions::Weather::Rain;
            stage(&mut w, &|i| if i == 1 { Team::Glacier } else { Team::Ember }, weapon("chaingun"));
            save(&shoot(&w, target + Vec3::new(-1.2, 0.5, -5.6), target, 46.0), "armor-rain");
            w.conditions.weather = Default::default();
            // Mixed teams from 40 and 110 m: can you tell size and side?
            stage(&mut w, &|i| if i == 1 { Team::Glacier } else { Team::Ember }, weapon("disc"));
            save(&shoot(&w, target + Vec3::new(5.0, 20.0, -34.0), target, 10.0), "armor-40m");
            save(&shoot(&w, target + Vec3::new(12.0, 55.0, -95.0), target, 6.0), "armor-110m");
            // Glacier from the front; the three running, then jetting (the
            // flames leave the armor's own nozzles).
            stage(&mut w, &|_| Team::Glacier, weapon("chaingun"));
            save(&shoot(&w, target + Vec3::new(-1.2, 0.5, -5.6), target, 46.0), "armor-front-glacier");
            stage(&mut w, &|_| Team::Ember, weapon("chaingun"));
            for p in w.players.iter_mut().skip(1) {
                p.vel = Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos()) * 7.5;
            }
            save(&shoot(&w, target + Vec3::new(-4.2, 0.6, -3.4), target, 46.0), "armor-run");
            stage(&mut w, &|i| if i == 1 { Team::Ember } else { Team::Glacier }, weapon("chaingun"));
            for p in w.players.iter_mut().skip(1) {
                p.on_ground = false; p.jetting = true; p.pos.y += 1.2;
                p.vel = Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos()) * 6.0 + Vec3::Y * 3.0;
            }
            save(&shoot(&w, target + Vec3::new(3.6, 1.0, 5.2), target + Vec3::Y * 0.6, 46.0), "armor-jet");
            save(&shoot(&w, target + Vec3::new(-3.4, 1.6, -4.6), target + Vec3::Y * 0.8, 46.0), "armor-jet-front");
            // The medium's hands on the weapon, close (its gloves).
            stage(&mut w, &|_| Team::Ember, weapon("chaingun"));
            save(&shoot(&w, origin + Vec3::new(-0.75, 1.45, -1.25), origin + Vec3::new(-0.08, 1.1, -0.3), 34.0), "armor-hands");
            // Every round that flies, frozen in a grid seen from the side,
            // moving right; a Tether's hook on its cable from its shooter.
            {
                let r = peakrunner_core::registry::registry();
                use peakrunner_core::registry::ProjectileClass;
                let shown: Vec<(u8, &str)> = r.projectiles.iter()
                    .filter(|(_, _, p)| !matches!(p.class, ProjectileClass::Beam | ProjectileClass::Stream))
                    .map(|(k, id, _)| (k as u8, id)).collect();
                w.players.truncate(1);
                let row_len = 8;
                let mid = origin + Vec3::new(0.0, 3.0, 0.0);
                let mut shooter = template.clone();
                shooter.pos = mid + Vec3::new(-7.5, -0.8, 0.0);
                shooter.yaw = -std::f32::consts::FRAC_PI_2;
                shooter.alive = true;
                w.players.push(shooter);
                w.discs.clear();
                for (n, (kind, id)) in shown.iter().enumerate() {
                    let (row, col) = (n / row_len, n % row_len);
                    let pos = mid + Vec3::new((col as f32 - (row_len as f32 - 1.0) / 2.0) * 1.5, row as f32 * 1.3, 0.0);
                    let tether = r.projectiles.get(*kind as usize).is_some_and(|p| p.pull != 0.0 || p.grapple > 0.0);
                    w.discs.push(crate::sim::Disc { pos, vel: Vec3::new(30.0, 0.0, 0.0), team: Team::Ember,
                        owner: if tether { 1 } else { crate::sim::MAX_PLAYERS }, life: 1.0, kind: *kind, spin: 0.7, target: 0, power: 1.0 });
                    println!("round {n}: {id}");
                }
                let rows = shown.len().div_ceil(row_len) as f32;
                let at = mid + Vec3::Y * (rows - 1.0) * 0.65;
                save(&shoot(&w, at + Vec3::new(0.0, 0.5, 10.5), at, 50.0), "rounds");
                save(&shoot(&w, mid + Vec3::new(-3.0, 1.0, 4.5), mid + Vec3::new(-3.0, 0.9, 0.0), 50.0), "rounds-close");
                w.discs.clear();
                w.players.truncate(1);
            }
            // Trails (`registry::Trail`): a disc and a mortar shell flown a
            // second up to here, so the smoke has built up frame by frame.
            {
                let r = peakrunner_core::registry::registry();
                let mid = origin + Vec3::new(0.0, 6.0, 0.0);
                let flights = [(r.projectile_kind("disc"), mid + Vec3::new(2.0, 2.0, 0.0), Vec3::new(95.0, 0.0, 0.0)),
                    (r.projectile_kind("mortar-shell"), mid + Vec3::new(-2.0, 6.0, -3.0), Vec3::new(58.0, 9.0, 0.0))];
                // And the target marker's laser held on the ground ahead.
                let mut laser = template.clone();
                laser.pos = origin + Vec3::new(4.0, 0.0, 3.0);
                laser.yaw = 0.9; laser.pitch = -0.25; laser.alive = true; laser.on_ground = true;
                laser.weapon = r.weapons.index("target-marker").unwrap() as u8;
                let look = Vec3::new(-laser.yaw.sin() * laser.pitch.cos(), laser.pitch.sin(), -laser.yaw.cos() * laser.pitch.cos());
                let t = (laser.pos.y + crate::terrain::EYE - origin.y) / -look.y;
                laser.stream = Some(laser.pos + Vec3::Y * crate::terrain::EYE + look * t);
                w.players.truncate(1);
                w.players.push(laser);
                let mut fx = crate::effects::Effects::new();
                let mut frame = None;
                for k in (0..=60).rev() {
                    let back = k as f32 / 60.0;
                    w.discs = flights.iter().map(|&(kind, end, vel)| crate::sim::Disc { pos: end - vel * back, vel, team: Team::Ember,
                        owner: crate::sim::MAX_PLAYERS, life: r.projectiles.get(kind as usize).unwrap().life - 1.0 + back, kind, spin: back * 40.0,
                        target: 0, power: 1.0 }).collect();
                    frame = Some(crate::drawlist::build_frame_with(&w, aspect, 1.0 / 60.0, &mut fx));
                }
                for (name, eye, at, fov) in [("trails", mid + Vec3::new(-8.0, 3.0, 26.0), mid + Vec3::new(-8.0, 0.0, 0.0), 55.0_f32),
                    ("trails-close", mid + Vec3::new(-1.0, 3.0, 9.0), mid + Vec3::new(-1.0, 3.5, -1.0), 55.0),
                    ("trails-sky", mid + Vec3::new(-14.0, -4.5, 14.0), mid + Vec3::new(-14.0, 9.0, -3.0), 60.0)] {
                    let mut f = frame.clone().unwrap();
                    f.eye = eye;
                    f.view = Mat4::look_at_rh(eye, at, Vec3::Y);
                    f.proj = crate::drawlist::clip_correct(Mat4::perspective_rh(fov.to_radians(), aspect, 0.1, 2500.0));
                    f.inv_vp = (f.proj * f.view).inverse();
                    f.viewmodel.clear();
                    f.vm_models.clear();
                    save(&f, name);
                }
                w.discs.clear();
                w.players.truncate(1);
            }
            // The CTF centres' turrets on their towers, held by Ember.
            for map in [MapId::Frostline, MapId::TowerComplex, MapId::Cairnhold] {
                let mut c = World::new();
                c.set_map(map);
                c.set_mode(peakrunner_core::map_catalog::SupportedMode::Ctf);
                c.start_match(true);
                c.state = MatchState::Playing;
                c.players.truncate(1);
                c.players[0].pos = Vec3::new(0.0, 900.0, 0.0);
                let Some(k) = c.points.iter().position(|p| p.active && p.turret.is_some()) else { panic!("{map:?}: no centre turret") };
                c.points[k].owner = Some(0);
                c.points[k].turret_aim = Vec3::new(0.6, -0.25, -0.75).normalize();
                let top = c.points[k].turret_at;
                save(&shoot(&c, top + Vec3::new(-7.0, 3.0, -9.0), top, 45.0), &format!("centre-{}", map.key()));
                save(&shoot(&c, top + Vec3::new(-40.0, 4.0, -55.0), c.points[k].pos + Vec3::Y * 8.0, 30.0), &format!("centre-{}-far", map.key()));
            }
            // Over the shoulder (the third-person view, its own camera): in the
            // open, then in the Ember hall with a wall 4 m ahead (the crosshair
            // marks where the shot lands, off center) and with one close behind
            // (the camera pulled in). Each with the HUD's crosshair drawn in.
            {
                let hall = |x: f32, z: f32| {
                    let at = Vec3::new(x, 0.0, z);
                    Vec3::new(x, peakrunner_core::terrain::support_on(w.map, at + Vec3::Y * 104.0).0 + crate::terrain::PLAYER_RADIUS + 0.02, z)
                };
                for (name, pos, yaw, pitch) in [("third-open", origin, 0.25_f32, -0.05_f32),
                    ("third-wall-ahead", hall(1185.0, 480.0), -std::f32::consts::FRAC_PI_2, 0.0),
                    ("third-wall-behind", hall(1188.2, 480.0), std::f32::consts::FRAC_PI_2, 0.0)] {
                    w.players.truncate(1);
                    let p = &mut w.players[0];
                    p.pos = pos; p.yaw = yaw; p.pitch = pitch; p.alive = true; p.on_ground = true; p.vel = Vec3::ZERO;
                    p.team = Team::Ember; p.armor = ArmorClass::by_id("medium").unwrap();
                    p.weapon = weapon("chaingun"); p.loadout = crate::sim::arsenal::Loadout::of(&[p.weapon]); p.cooldown = 0.0;
                    w.third_person = true;
                    marker.set(crate::app::third_person_aim(&w));
                    save(&crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut crate::effects::Effects::new()), name);
                    w.third_person = false;
                }
                w.players[0] = template.clone();
            }
            // First person in each armor: the support arm is the armor's own.
            for size in sizes {
                let k = weapon("chaingun");
                w.players.truncate(1);
                w.players[0].pos = origin; w.players[0].yaw = 0.2; w.players[0].pitch = -0.05;
                w.players[0].armor = ArmorClass::by_id(size).unwrap();
                w.players[0].weapon = k; w.players[0].loadout = crate::sim::arsenal::Loadout::of(&[k]);
                w.input.weapon = k; w.players[0].cooldown = 0.0;
                let mut fx = crate::effects::Effects::new();
                for _ in 0..40 { crate::drawlist::build_frame_with(&w, aspect, 1.0 / 60.0, &mut fx); }
                save(&crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut fx), &format!("vm-armor-{size}"));
            }
            // The repair tool held up instead of the weapon, and Football's
            // ball carried in the right hand.
            for (name, ball) in [("repair-tool", false), ("football-carry", true)] {
                w.players.truncate(1);
                w.players[0].pos = origin; w.players[0].yaw = 0.2; w.players[0].pitch = -0.05;
                w.players[0].armor = ArmorClass::by_id("medium").unwrap();
                w.players[0].repair_out = !ball;
                w.ball.active = ball;
                w.ball.carrier = ball.then_some(0);
                let mut fx = crate::effects::Effects::new();
                for _ in 0..40 { crate::drawlist::build_frame_with(&w, aspect, 1.0 / 60.0, &mut fx); }
                save(&crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut fx), &format!("vm-{name}"));
                w.players[0].repair_out = false;
                w.ball.active = false;
                w.ball.carrier = None;
            }
            // A rider in the Ridgerunner's open seat, from the side and behind.
            {
                use crate::sim::vehicles::{Craft, MAX_SEATS};
                let kind = registry.vehicles.index("cycle").expect("the stock Ridgerunner");
                let def = registry.vehicles.get(kind).unwrap();
                w.players.truncate(1);
                let mut p = template.clone();
                p.alive = true; p.vel = Vec3::ZERO; p.yaw = 0.0; p.pitch = 0.0; p.riding = 1;
                p.armor = ArmorClass::by_id("medium").unwrap();
                p.weapon = weapon("chaingun"); p.loadout = crate::sim::arsenal::Loadout::of(&[p.weapon]);
                w.players.push(p);
                let mut riders = [0u8; MAX_SEATS];
                riders[0] = 2;
                let at = origin + Vec3::Y * 0.6;
                w.vehicles = vec![Craft { kind: kind as u8, team: Team::Ember, pos: at, vel: Vec3::ZERO, yaw: 0.0, pitch: 0.0,
                    hull: def.hull, energy: def.energy_max, riders, cooldowns: [0.0; MAX_SEATS], idle: 0.0 }];
                w.players[1].pos = w.vehicles[0].seat_at(0);
                save(&shoot(&w, at + Vec3::new(4.2, 0.6, 0.4), at + Vec3::Y * 0.4, 45.0), "rider-side");
                save(&shoot(&w, at + Vec3::new(1.6, 1.4, 4.0), at + Vec3::Y * 0.4, 45.0), "rider-back");
                w.vehicles.clear();
                w.players.truncate(1);
            }
            w.players[0] = template.clone();
            // Every weapon with a model, in the hands and in first person.
            let ids: Vec<String> = registry.weapons.iter().map(|(_, id, _)| id.to_string()).collect();
            for id in ids {
                let k = weapon(&id);
                if crate::player_model::weapon_model(k).is_none() { continue; }
                stage(&mut w, &|_| Team::Ember, k);
                w.players.truncate(2);
                w.players[1].armor = ArmorClass::by_id("medium").unwrap();
                w.players[1].pos = origin;
                save(&shoot(&w, target + Vec3::new(-1.7, 0.4, -2.2), target + Vec3::new(0.0, 0.3, -0.2), 40.0), &format!("held-{id}"));
                w.players.truncate(1);
                w.players[0].pos = origin;
                w.players[0].yaw = 0.2;
                w.players[0].pitch = -0.05;
                w.players[0].weapon = k;
                w.players[0].loadout = crate::sim::arsenal::Loadout::of(&[k]);
                w.input.weapon = k;
                w.players[0].cooldown = 0.0;
                let mut fx = crate::effects::Effects::new();
                for _ in 0..40 { crate::drawlist::build_frame_with(&w, aspect, 1.0 / 60.0, &mut fx); }
                save(&crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut fx), &format!("vm-{id}"));
                w.players[0].pos = template.pos;
            }
        });
    }

    /// Weapon visuals: every viewmodel state, third-person models, rounds in
    /// flight, chaingun impacts and layered explosions on three maps. Writes
    /// `screenshots/weapons-after-*.png`. Run with --ignored --nocapture.
    #[test]
    #[ignore = "requires a GPU adapter"]
    fn render_weapon_captures() {
        use super::*;
        use crate::sim::{Disc, Explosion, MatchState, Team, World};
        pollster::block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("GPU adapter");
            let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("GPU device");
            let mut scene = SceneGpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            std::fs::create_dir_all("screenshots").unwrap();
            let (width, height) = (1280u32, 800u32);
            let aspect = width as f32 / height as f32;
            let mut save = |frame: &crate::drawlist::DrawFrame, name: &str| {
                let mut encoder = device.create_command_encoder(&Default::default());
                scene.render(&device, &queue, &mut encoder, width, height, frame);
                let stride = (width * 4).div_ceil(256) * 256;
                let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("capture"), size: (stride * height) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
                encoder.copy_texture_to_buffer(scene.color.as_image_copy(),
                    wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout {
                        offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(height) } },
                    wgpu::Extent3d { width, height, depth_or_array_layers: 1 });
                queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| { tx.send(r).unwrap(); });
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                rx.recv().unwrap().unwrap();
                let mapped = buffer.slice(..).get_mapped_range().unwrap();
                let pixels: Vec<u8> = mapped.chunks(stride as usize).flat_map(|row| row[..width as usize * 4].iter().copied()).collect::<Vec<u8>>()
                    // Alpha carries the bloom mask; captures are opaque images.
                    .chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                let path = format!("screenshots/weapons-after-{name}.png");
                let mut png = png::Encoder::new(std::fs::File::create(&path).unwrap(), width, height);
                png.set_color(png::ColorType::Rgba);
                png.set_depth(png::BitDepth::Eight);
                png.write_header().unwrap().write_image_data(&pixels).unwrap();
                println!("wrote {path}");
            };
            let fresh = |map: MapId| {
                let mut w = World::new();
                w.set_map(map);
                w.start_match(true);
                w.players.truncate(1);
                w.state = MatchState::Playing;
                w.players[0].yaw = std::f32::consts::PI;
                w.players[0].pitch = -0.12;
                w
            };
            let dt = 1.0 / 60.0;
            let develop = |w: &World, fx: &mut crate::effects::Effects, frames: usize| {
                for _ in 0..frames { crate::drawlist::build_frame_with(w, aspect, dt, fx); }
            };

            // Viewmodels: idle, firing, mid reload or cycle, mid switch.
            for (weapon, label) in [(0u8, "disc"), (1, "chaingun"), (2, "grenade")] {
                let reload = crate::sim::weapon_reload(weapon);
                let mut w = fresh(MapId::OldHoller);
                w.players[0].weapon = weapon;
                let mut fx = crate::effects::Effects::new();
                develop(&w, &mut fx, 30);
                save(&crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut fx), &format!("vm-{label}-idle"));
                // Firing: the chaingun needs a spun-up, heated barrel first.
                if weapon == 1 {
                    for f in 0..90 {
                        w.players[0].cooldown = reload - (f % 4) as f32 * 0.018;
                        w.players[0].shots += (f % 4 == 0) as u32;
                        develop(&w, &mut fx, 1);
                    }
                    w.players[0].cooldown = reload - 0.012;
                } else {
                    w.players[0].shots += 1;
                    w.players[0].cooldown = reload - 0.02;
                    develop(&w, &mut fx, 1);
                }
                save(&crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut fx), &format!("vm-{label}-firing"));
                if weapon == 1 {
                    // Released: the barrels coast down while the jacket cools.
                    w.players[0].cooldown = 0.0;
                    develop(&w, &mut fx, 15);
                    save(&crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut fx), "vm-chaingun-spindown");
                } else {
                    w.players[0].cooldown = reload * 0.45;
                    develop(&w, &mut fx, 1);
                    save(&crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut fx), &format!("vm-{label}-reload"));
                }
                w.players[0].cooldown = 0.0;
                develop(&w, &mut fx, 30);
                w.players[0].weapon = (weapon + 1) % 3;
                develop(&w, &mut fx, 10);
                save(&crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut fx), &format!("vm-{label}-switch"));
            }

            // Rifles: the laser (light) and the railgun (heavy), idle and just fired
            // with their shot in the air.
            for heavy in [false, true] {
                use crate::sim::rifles;
                let mut w = fresh(MapId::OldHoller);
                // Each armor's own rifle.
                let rifle = if heavy { crate::sim::arsenal::RAILGUN } else { rifles::RIFLE_SLOT };
                w.players[0].loadout.push(rifle);
                w.players[0].weapon = rifle;
                if heavy { w.players[0].armor = crate::sim::ArmorClass::Heavy; w.players[0].ammo[rifle as usize] = 20; }
                w.players[0].energy = peakrunner_core::defs::movement().energy_max;
                let label = if heavy { "railgun" } else { "laser" };
                let mut fx = crate::effects::Effects::new();
                develop(&w, &mut fx, 30);
                save(&crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut fx), &format!("vm-{label}-idle"));
                let (eye, dir, _) = w.camera();
                let muzzle = eye+dir*1.0+Vec3::new(0.25, -0.25, 0.0);
                w.players[0].shots += 1;
                w.players[0].cooldown = crate::sim::weapon_reload(w.players[0].weapon)-0.03;
                w.discs.push(if heavy {
                    Disc { pos: eye+dir*35.0, vel: dir*peakrunner_core::defs::weapons().railgun.speed, team: Team::Ember, owner: 0,
                        life: peakrunner_core::defs::weapons().railgun.life-0.06, kind: rifles::RAIL_KIND, spin: 0.0, target: 0, power: 1.0 }
                } else {
                    Disc { pos: muzzle, vel: dir*120.0, team: Team::Ember, owner: 0, life: peakrunner_core::defs::weapons().laser.fade*0.8,
                        kind: rifles::LASER_KIND, spin: 0.0, target: 0, power: 1.0 }
                });
                develop(&w, &mut fx, 1);
                save(&crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut fx), &format!("vm-{label}-firing"));
            }

            // Third-person weapons, one of each, posed for the camera.
            {
                let mut w = fresh(MapId::OldHoller);
                let origin = w.players[0].pos;
                for i in 0..3 {
                    let mut p = w.players[0].clone();
                    p.pos = origin + Vec3::new((i as f32 - 1.0) * 1.6, 0.0, 0.0);
                    p.yaw = [0.55, 1.2, 2.6][i];
                    p.team = if i == 1 { Team::Glacier } else { Team::Ember };
                    p.weapon = i as u8;
                    p.net_id = 50 + i as u32;
                    w.players.push(p);
                }
                let mut frame = crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut crate::effects::Effects::new());
                let target = origin + Vec3::Y * 0.75;
                frame.eye = target + Vec3::new(0.6, 0.4, -4.2);
                frame.view = Mat4::look_at_rh(frame.eye, target, Vec3::Y);
                frame.proj = crate::drawlist::clip_correct(Mat4::perspective_rh(45f32.to_radians(), aspect, 0.1, 2500.0));
                frame.inv_vp = (frame.proj * frame.view).inverse();
                frame.viewmodel.clear();
                frame.vm_models.clear();
                save(&frame, "thirdperson");
            }

            // Rounds in flight: disc crossing the view, grenade with its fuse
            // lit, turret plasma, all staged a few metres ahead.
            {
                let mut w = fresh(MapId::OldHoller);
                w.players[0].pos.y += 10.0;
                let (eye, dir, _) = w.camera();
                let side = dir.cross(Vec3::Y).normalize();
                w.discs.push(Disc { pos: eye + dir * 5.0 - Vec3::Y * 0.9 + side * 0.5, vel: side * 95.0, team: Team::Ember,
                    owner: 0, life: 4.88, kind: 0, spin: 1.1, target: 0, power: 1.0 });
                w.discs.push(Disc { pos: eye + dir * 3.2 - Vec3::Y * 0.4 - side * 1.1, vel: dir * 30.0 + Vec3::Y * 3.0,
                    team: Team::Ember, owner: 0, life: 0.2, kind: 2, spin: 0.7, target: 0, power: 1.0 });
                w.discs.push(Disc { pos: eye + dir * 9.0 + Vec3::Y * 0.6 - side * 2.5, vel: -dir * 80.0 + side * 10.0,
                    team: Team::Glacier, owner: usize::MAX, life: 2.93, kind: 3, spin: 0.0, target: 0, power: 1.0 });
                w.players[0].weapon = 2;
                save(&crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut crate::effects::Effects::new()), "rounds-in-flight");
            }

            // Chaingun stream into the ground: real fired rounds, tracers and
            // impact sparks and dust, detected the way a live client sees them.
            {
                let mut w = fresh(MapId::OldHoller);
                w.players[0].pos.y += 10.0;
                w.players[0].weapon = 1;
                w.input.weapon = 1;
                w.players[0].pitch = -0.55;
                let mut fx = crate::effects::Effects::new();
                w.input.fire = true;
                for _ in 0..50 {
                    w.tick(dt);
                    w.players[0].pos.y = w.players[0].pos.y.max(0.0);
                    develop(&w, &mut fx, 1);
                }
                save(&crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut fx), "chaingun-impacts");
            }

            // Layered explosions on grass, snow and sand: early (flash and
            // flames over fresh smoke) and late (smoke, debris, scorch only).
            for (map, label) in [(MapId::OldHoller, "holler"), (MapId::Frostline, "frostline"), (MapId::Dustreach, "dustreach")] {
                for (kind, what) in [(2u8, "grenade"), (4, "generator")] {
                    let mut w = fresh(map);
                    let size = crate::terrain::info(map).size;
                    let stand = Vec3::new(size * 0.5 + 40.0, 0.0, size * 0.5 - 120.0);
                    let ground = crate::terrain::height_on(map, stand.x, stand.z);
                    w.players[0].pos = Vec3::new(stand.x, ground + 6.0, stand.z);
                    w.players[0].yaw = 0.0;
                    w.players[0].pitch = -0.2;
                    let (eye, dir, _) = w.camera();
                    let flat = Vec3::new(dir.x, 0.0, dir.z).normalize();
                    let spot = eye + flat * if kind == 4 { 34.0 } else { 20.0 };
                    let floor = crate::terrain::height_on(map, spot.x, spot.z);
                    let pos = Vec3::new(spot.x, floor + if kind == 4 { 2.0 } else { 0.3 }, spot.z);
                    let radius = if kind == 4 { crate::sim::GENERATOR_BLAST_RADIUS } else { 9.0 };
                    let mut fx = crate::effects::Effects::new();
                    for (age, when) in [(0.3f32, "early"), (1.0, "late")] {
                        while fx_age(&w) < age {
                            let next = fx_age(&w) + dt;
                            w.explosions.retain(|e| e.age < 0.55);
                            if next < 0.55 {
                                if w.explosions.is_empty() { w.explosions.push(Explosion { pos, age: 0.0, max_r: radius, kind }); }
                                w.explosions[0].age = next;
                            } else { w.explosions.clear(); }
                            w.time += dt;
                            develop(&w, &mut fx, 1);
                            if next >= 0.55 && w.explosions.is_empty() { w.smoke.clear(); }
                            set_fx_age(&mut w, next);
                        }
                        let mut frame = crate::drawlist::build_frame_with(&w, aspect, 0.0, &mut fx);
                        frame.viewmodel.clear();
                        frame.vm_models.clear();
                        save(&frame, &format!("blast-{what}-{label}-{when}"));
                    }
                }
            }
        });
        // Simulated time since the staged blast, kept outside the explosion
        // record because that record is dropped at 0.55 s.
        fn fx_age(w: &crate::sim::World) -> f32 { w.flyby }
        fn set_fx_age(w: &mut crate::sim::World, age: f32) { w.flyby = age; }
    }

    /// Eight players in one fight: chaingun streams into the ground, discs and
    /// grenades landing, bots doing their own thing. Reports peak draw counts,
    /// CPU frame-build time and GPU render time. Run with --release --ignored.
    #[test]
    #[ignore = "timing probe; requires a GPU adapter"]
    fn heavy_fight_budget() {
        use super::*;
        use crate::sim::{Disc, MatchState, Team, World};
        let mut w = World::new();
        // PROBE_MAP=<map key> times another map; Old Holler by default.
        w.set_map(std::env::var("PROBE_MAP").ok().and_then(|k| MapId::parse(&k)).unwrap_or(MapId::OldHoller));
        w.start_match(true);
        w.state = MatchState::Playing;
        let me = w.player_id;
        let origin = w.players[me].pos;
        while w.players.len() < 8 { let p = w.players[0].clone(); w.players.push(p); }
        for i in 0..8 {
            let p = &mut w.players[i];
            p.alive = true;
            p.net_id = i as u32 + 1;
            p.team = if i % 2 == 0 { Team::Ember } else { Team::Glacier };
            if i != me { p.pos = origin + Vec3::new((i as f32 - 3.5) * 6.0, 0.0, -24.0 - (i % 2) as f32 * 7.0); }
            p.weapon = (i % 3) as u8;
        }
        let dt = 1.0 / 60.0;
        let mut fx = crate::effects::Effects::new();
        let (mut peak_lit, mut peak_emit, mut peak_smoke, mut total) = (0, 0, 0, std::time::Duration::ZERO);
        let mut heaviest: Option<crate::drawlist::DrawFrame> = None;
        let frames = 600;
        for f in 0..frames {
            let (eye, dir, _) = w.camera();
            for i in 0..8 {
                if i == me { continue; }
                let p = w.players[i].clone();
                let from = p.pos + Vec3::Y * 1.2;
                let spot = eye + Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero() * (18.0 + (i * 3) as f32)
                    + Vec3::X * ((f % 40) as f32 - 20.0) * 0.4;
                let target = Vec3::new(spot.x, crate::terrain::height_on(w.map, spot.x, spot.z), spot.z);
                let aim = (target - from).normalize_or_zero();
                let (kind, every, speed, life) = match i % 3 { 1 => (1u8, 5, 420.0, 1.2), 0 => (0, 63, 95.0, 5.0), _ => (2, 51, 48.0, 2.0) };
                if (f + i * 7) % every == 0 {
                    w.discs.push(Disc { pos: from, vel: aim * speed, team: p.team, owner: i, life, kind, spin: 0.0, target: 0, power: 1.0 });
                    w.players[i].shots += 1;
                }
            }
            w.tick(dt);
            let start = std::time::Instant::now();
            let frame = crate::drawlist::build_frame_with(&w, 1.6, dt, &mut fx);
            total += start.elapsed();
            let weight = frame.lit.len() + frame.emit.len() + frame.smoke.len();
            peak_lit = peak_lit.max(frame.lit.len());
            peak_emit = peak_emit.max(frame.emit.len());
            peak_smoke = peak_smoke.max(frame.smoke.len());
            if heaviest.as_ref().is_none_or(|h| h.lit.len() + h.emit.len() + h.smoke.len() < weight) { heaviest = Some(frame); }
        }
        println!("peak draws: lit {peak_lit} emit {peak_emit} smoke {peak_smoke}; build avg {:.3} ms",
            total.as_secs_f64() * 1000.0 / frames as f64);
        let frame = heaviest.unwrap();
        pollster::block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.expect("GPU adapter");
            let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.expect("GPU device");
            let mut scene = SceneGpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            scene.set_max_samples(supported_samples(&adapter));
            // Render plus the bloom-composite blit, as the game draws a frame.
            for (width, height) in [(1280u32, 800u32), (1920, 1080), (2560, 1440), (3456, 2234), (5120, 2880)] {
                let out = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("probe out"),
                    size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                    mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
                });
                let out_view = out.create_view(&Default::default());
                for msaa in [false, true] {
                    MSAA_WANTED.store(msaa, std::sync::atomic::Ordering::Relaxed);
                    for bloom in [0u8, 1, 2] {
                        BLOOM_LEVEL.store(bloom, std::sync::atomic::Ordering::Relaxed);
                        let mut times = Vec::new();
                        for k in 0..80 {
                            let start = std::time::Instant::now();
                            let mut encoder = device.create_command_encoder(&Default::default());
                            scene.render(&device, &queue, &mut encoder, width, height, &frame);
                            {
                                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                    label: Some("probe composite"),
                                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                        view: &out_view, depth_slice: None, resolve_target: None,
                                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                                    })],
                                    depth_stencil_attachment: None, timestamp_writes: None,
                                    occlusion_query_set: None, multiview_mask: None,
                                });
                                scene.composite(&mut pass);
                            }
                            queue.submit([encoder.finish()]);
                            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                            if k >= 20 { times.push(start.elapsed().as_secs_f64() * 1000.0); }
                        }
                        times.sort_by(f64::total_cmp);
                        println!("heaviest frame ({} lit, {} emit, {} smoke) {width}x{height}, {}x MSAA, bloom {bloom}: median {:.2} ms, p95 {:.2} ms",
                            frame.lit.len(), frame.emit.len(), frame.smoke.len(), scene.samples,
                            times[times.len() / 2], times[times.len() * 95 / 100]);
                    }
                }
            }
            MSAA_WANTED.store(true, std::sync::atomic::Ordering::Relaxed);
            BLOOM_LEVEL.store(1, std::sync::atomic::Ordering::Relaxed);
        });
    }

    #[test]
    fn armor_mesh_has_outward_unit_normals_and_valid_indices() {
        let (vertices,indices)=super::armor_mesh();
        assert!(indices.iter().all(|&i|(i as usize)<vertices.len()/6));
        for v in vertices.chunks_exact(6) {
            let p=glam::Vec3::from_slice(&v[..3]);
            let n=glam::Vec3::from_slice(&v[3..]);
            assert!((n.length()-1.).abs()<0.001 && p.dot(n)>0.);
        }
    }

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn render_grass_captures() {
        use super::*;
        pollster::block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default())
                .await.expect("GPU adapter");
            let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default())
                .await.expect("GPU device");
            let mut scene = SceneGpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            std::fs::create_dir_all("screenshots").unwrap();
            let shoot = |scene: &mut SceneGpu, world: &crate::sim::World, name: &str, w: u32, h: u32| {
                let frame = crate::drawlist::build_frame(world, w as f32 / h as f32, 0.016);
                let mut encoder = device.create_command_encoder(&Default::default());
                scene.render(&device, &queue, &mut encoder, w, h, &frame);
                let stride = (w * 4).div_ceil(256) * 256;
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("capture"),
                    size: (stride * h) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                encoder.copy_texture_to_buffer(
                    scene.color.as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(stride),
                            rows_per_image: Some(h),
                        },
                    },
                    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                );
                queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| { tx.send(r).unwrap(); });
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                rx.recv().unwrap().unwrap();
                let mapped = buffer.slice(..).get_mapped_range().unwrap();
                let pixels: Vec<u8> = mapped.chunks(stride as usize)
                    .flat_map(|row| row[..w as usize * 4].iter().copied()).collect::<Vec<u8>>()
                    // Alpha carries the bloom mask; captures are opaque images.
                    .chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                let path = if name.starts_with("menu-clip") {
                    format!("screenshots/{name}.png")
                } else { format!("screenshots/grass-look-{name}.png") };
                let mut encoder = png::Encoder::new(std::fs::File::create(&path).unwrap(), w, h);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
                println!("Rendered {path}");
            };
            let mut world = crate::sim::World::new();
            if std::env::var_os("QA_LID_PROBE").is_some() {
                world.set_map(MapId::Dustreach); world.start_match(true); world.players.truncate(1);
                for (name,pos,pitch) in [("menu-clip-lid-high",[1076.,420.,760.],-1.25_f32),("menu-clip-lid-low",[1076.,200.,700.],-0.9)] {
                    world.players[0].pos=Vec3::from_array(pos); world.players[0].yaw=std::f32::consts::PI; world.players[0].pitch=pitch;
                    shoot(&mut scene,&world,name,1280,800);
                }
                return;
            }
            if let Ok(tag)=std::env::var("QA_MENU_CLIP") {
                // Menu flyby at six points round the orbit, each as a pair of
                // frames 0.05 s apart: depth fighting flickers between them.
                for map in [MapId::OldHoller, MapId::TowerComplex,
                    MapId::Cairnhold, MapId::Frostline, MapId::Dustreach] {
                    world.set_map(map);
                    world.state=crate::sim::MatchState::Flyby;
                    for i in 0..6 {
                        for (j,dt) in [(0,0.0_f32),(1,0.05)] {
                            world.flyby=i as f32*std::f32::consts::TAU/0.18/6.+dt;
                            shoot(&mut scene,&world,&format!("menu-clip-{tag}-{}-{i}{}",map.key(),["a","b"][j]),1280,800);
                        }
                    }
                }
                return;
            }
            if std::env::var_os("QA_ROTATION").is_some() {
                for map in [MapId::OldHoller, MapId::TowerComplex,
                    MapId::Cairnhold, MapId::Frostline, MapId::Dustreach,
                    MapId::OldHoller] {
                    world.set_map(map); world.start_match(true); world.players.truncate(1);
                    let pack = peakrunner_core::map_pack::on(map).expect("rotation pack installed");
                    let pos = Vec3::from_array(pack.manifest.spawns[0]) + Vec3::Y * 2.;
                    let target = Vec3::from_array(pack.manifest.flags[1]);
                    let d = (target-pos).normalize();
                    world.players[0].pos = pos;
                    world.players[0].yaw = (-d.x).atan2(-d.z);
                    world.players[0].pitch = d.y.asin();
                    shoot(&mut scene, &world, &format!("rotation-{}",map.key()), 1280, 800);
                }
                return;
            }
            world.set_map(MapId::OldHoller);
            world.flyby = 0.6;
            shoot(&mut scene, &world, "menu", 1280, 800);
            world.flyby = 2.4;
            shoot(&mut scene, &world, "menu-b", 1280, 800);
            world.start_match(true);
            world.players.truncate(1);
            world.players[0].yaw = std::f32::consts::PI;
            world.players[0].pitch = -0.18;
            shoot(&mut scene, &world, "ahead", 1280, 800);
            world.players[0].pitch = -0.62;
            shoot(&mut scene, &world, "down", 1280, 800);
        });
    }
}

