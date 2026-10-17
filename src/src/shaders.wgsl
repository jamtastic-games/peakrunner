struct WorldUniforms {
    mvp: mat4x4<f32>,
    model: mat4x4<f32>,
    normal: mat3x3<f32>,
    cam: vec3<f32>,
    emit: f32,
    sun: vec3<f32>,
    mode: f32,
    fog: vec3<f32>,
    pad0: f32,
    color: vec3<f32>,
    pad1: f32,
    ground_cover: vec4<f32>,
    ground_soil: vec4<f32>,
    ground_rock: vec4<f32>,
    ground_rules: vec4<f32>,
    // World to the sun's shadow map (scene.rs `ShadowGpu`).
    light_vp: mat4x4<f32>,
}

struct SkyUniforms {
    inv_vp: mat4x4<f32>,
    cam: vec3<f32>,
    pad0: f32,
    sun: vec3<f32>,
    pad1: f32,
}

struct EmitUniforms {
    mvp: mat4x4<f32>,
    color: vec4<f32>,
    // Smoke pass only: fog colour and this draw's fog amount (0..1).
    fog: vec4<f32>,
    // x: 1 for a flat ground decal (scorch), 0 otherwise.
    params: vec4<f32>,
}

@group(0) @binding(0) var<uniform> world: WorldUniforms;
@group(0) @binding(1) var grass_albedo: texture_2d<f32>;
@group(0) @binding(2) var grass_samp: sampler;
@group(0) @binding(3) var grass_normal: texture_2d<f32>;
@group(1) @binding(0) var<uniform> sky: SkyUniforms;
@group(2) @binding(0) var<uniform> emit_u: EmitUniforms;
@group(3) @binding(0) var scene_tex: texture_2d<f32>;
@group(3) @binding(1) var scene_samp: sampler;
@group(3) @binding(2) var bloom_tex: texture_2d<f32>;
@group(3) @binding(3) var<uniform> bloom_u: vec4<f32>;

struct LitIn {
    @location(0) pos: vec3<f32>,
    @location(1) n: vec3<f32>,
}

struct LitOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) n: vec3<f32>,
}

@vertex
fn vs_world(in: LitIn) -> LitOut {
    let wp = world.model * vec4<f32>(in.pos, 1.0);
    var out: LitOut;
    out.world_pos = wp.xyz;
    out.n = world.normal * in.n;
    out.clip = world.mvp * vec4<f32>(in.pos, 1.0);
    return out;
}

// A caster's depth from the sun, for the shadow map.
@vertex
fn vs_world_shadow(in: LitIn) -> @builtin(position) vec4<f32> {
    return world.light_vp * world.model * vec4<f32>(in.pos, 1.0);
}

fn terrain_hash(p: vec2<f32>) -> f32 {
    let p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    let q = p3 + dot(p3, p3.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn terrain_noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(terrain_hash(cell), terrain_hash(cell + vec2<f32>(1.0, 0.0)), u.x),
        mix(terrain_hash(cell + vec2<f32>(0.0, 1.0)), terrain_hash(cell + vec2<f32>(1.0)), u.x), u.y);
}

// Same highlight shoulder as map.wgsl's `tone`: identity below 0.8, rolling
// off toward 1 instead of clipping, so entities and the map grade alike.
fn tone_shoulder(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    let t = 0.8;
    let rolled = vec3<f32>(t) + (1.0 - t) * (vec3<f32>(1.0) - exp(-(x - vec3<f32>(t)) / (1.0 - t)));
    return select(x, rolled, x > vec3<f32>(t));
}

fn safe_normalize(v: vec3<f32>) -> vec3<f32> {
    let l2 = dot(v, v);
    if (l2 < 1e-8) {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    return v / sqrt(l2);
}

fn terrain_tangent(n: vec3<f32>) -> vec3<f32> {
    var t = vec3<f32>(1.0, 0.0, 0.0) - n * n.x;
    if (dot(t, t) < 1e-4) {
        t = vec3<f32>(0.0, 0.0, 1.0) - n * n.z;
    }
    return safe_normalize(t);
}

@fragment
fn fs_world(in: LitOut) -> @location(0) vec4<f32> {
    var n = normalize(in.n);
    let mesh_n = n;
    let slope = 1.0 - mesh_n.y;
    var albedo = world.color;
    if (world.mode > 0.5 && world.mode < 1.5) {
        let rock = vec3<f32>(0.27, 0.25, 0.23);
        let snow = vec3<f32>(0.66, 0.73, 0.79);
        let ice = vec3<f32>(0.42, 0.55, 0.64);
        let snow_amt = (1.0 - smoothstep(0.12, 0.42, slope)) * smoothstep(8.0, 20.0, in.world_pos.y);
        let ice_amt = 1.0 - smoothstep(3.0, 14.0, in.world_pos.y);
        albedo = mix(mix(rock, ice, ice_amt * 0.55), snow, snow_amt);
        let broad = terrain_noise(in.world_pos.xz * 0.045);
        let detail = terrain_noise(in.world_pos.xz * 0.7);
        let detail_fade = 1.0 - smoothstep(40.0, 220.0, length(world.cam - in.world_pos));
        albedo *= 0.88 + 0.12 * broad + (detail - 0.5) * 0.12 * detail_fade;
    }
    if (world.mode > 1.5 && world.mode < 2.5) {
        let xz = in.world_pos.xz;
        let tile = world.ground_cover.w;
        let patch_scale = world.ground_soil.w;
        // Fixed world-space scales: no camera-distance UV morphing.
        let tex = textureSample(grass_albedo, grass_samp, xz / tile).r;
        let mid = textureSample(grass_albedo, grass_samp,
            vec2<f32>(xz.y, -xz.x) / (tile * 3.1)).r;
        let broad = terrain_noise(xz / patch_scale);
        let broken = terrain_noise(xz / (patch_scale * 0.23) + vec2<f32>(13.0, 29.0));
        let dry = smoothstep(world.ground_rules.w - 0.13, world.ground_rules.w + 0.13,
            broad * 0.7 + broken * 0.3 + slope * 0.20);
        let rock_weight = smoothstep(world.ground_rules.x, world.ground_rules.y, slope);
        let cover = world.ground_cover.rgb * mix(0.85, 1.15, broken);
        albedo = mix(mix(cover, world.ground_soil.rgb, dry),
            world.ground_rock.rgb, rock_weight);
        let grain = (tex - 0.5) * 1.5 + (mid - 0.5) * 0.6;
        albedo *= 1.0 + grain * world.ground_rock.w;

        let tn = textureSample(grass_normal, grass_samp, xz / tile).xyz * 2.0 - 1.0;
        let tangent = terrain_tangent(mesh_n);
        let bitangent = safe_normalize(cross(tangent, mesh_n));
        let mapped = safe_normalize(tangent * tn.x + bitangent * tn.z + mesh_n * tn.y);
        n = safe_normalize(mix(mesh_n, mapped, world.ground_rules.z));
    }
    let ndl = max(dot(n, world.sun), 0.0);
    var col = albedo * (0.22 + 0.78 * ndl);
    if (world.mode > 2.5 && world.mode < 3.5) {
        // Skin: light wraps past the terminator and the shadow side warms,
        // as light scattered under skin does, with a soft broad sheen.
        let wrap = max((dot(n, world.sun) + 0.4) / 1.4, 0.0);
        col = albedo * (0.26 + 0.74 * wrap) + albedo * vec3<f32>(0.16, 0.04, 0.02) * (1.0 - ndl);
        let half_dir = normalize(world.sun + normalize(world.cam - in.world_pos));
        col += vec3<f32>(0.05, 0.045, 0.04) * pow(max(dot(n, half_dir), 0.0), 10.0);
    }
    if (world.mode > 1.5 && world.mode < 2.5) {
        col = mix(col * vec3<f32>(0.92, 0.96, 0.99), col * vec3<f32>(1.05, 1.02, 0.94), ndl);
    }
    col = col + albedo * vec3<f32>(0.12, 0.16, 0.22) * max(n.y, 0.0);
    col = col + world.color * world.emit;
    if (world.mode < 0.5 && world.emit < 0.5) {
        let half_dir = normalize(world.sun + normalize(world.cam - in.world_pos));
        let specular = pow(max(dot(n, half_dir), 0.0), 36.0);
        col += vec3<f32>(0.2, 0.22, 0.23) * specular;
    }
    col = tone_shoulder(col);
    let dist = length(world.cam - in.world_pos);
    let density = select(0.0072, world.pad0, world.pad0 > 0.0);
    let fog = clamp(1.0 - exp(-dist * density), 0.0, 0.92);
    col = mix(col, world.fog, fog);
    // Bloom mask (glow = 1 - alpha): self-lit parts such as visors, thruster
    // housings and turret emitters glow; faint emit (snow motes) does not.
    let glow = clamp((world.emit - 0.2) / 0.8, 0.0, 1.0) * (1.0 - fog);
    return vec4<f32>(col, 1.0 - glow);
}

struct FullOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

@vertex
fn vs_fullscreen(@builtin(vertex_index) i: u32) -> FullOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = positions[i];
    var out: FullOut;
    out.clip = vec4<f32>(p, 0.999, 1.0);
    out.ndc = p;
    return out;
}

@fragment
fn fs_sky(in: FullOut) -> @location(0) vec4<f32> {
    let far = sky.inv_vp * vec4<f32>(in.ndc, 0.0, 1.0);
    let dir = normalize(far.xyz / far.w - sky.cam);
    let h = dir.y;
    let zenith = vec3<f32>(0.16, 0.26, 0.38);
    let horizon = vec3<f32>(0.62, 0.48, 0.38);
    let ground = vec3<f32>(0.18, 0.20, 0.24);
    var col = mix(horizon, zenith, smoothstep(-0.02, 0.55, h));
    col = mix(ground, col, smoothstep(-0.18, 0.04, h));
    let cloud_uv = dir.xz / max(dir.y + 0.25, 0.12);
    let cloud = terrain_noise(cloud_uv * 2.3) * 0.65 + terrain_noise(cloud_uv * 6.0) * 0.35;
    let cover = smoothstep(0.48, 0.76, cloud) * smoothstep(0.03, 0.22, h);
    col = mix(col, vec3<f32>(0.68, 0.67, 0.62), cover * 0.65);
    let sun_d = max(dot(dir, sky.sun), 0.0);
    let disc = pow(sun_d, 180.0);
    col = col + vec3<f32>(1.0, 0.86, 0.62) * disc * 1.6;
    col = col + vec3<f32>(1.0, 0.55, 0.28) * pow(sun_d, 6.0) * 0.18;
    return vec4<f32>(col, 1.0 - clamp(disc * 1.2, 0.0, 1.0));
}

struct EmitOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) facing: f32,
}

@vertex
fn vs_emit(in: LitIn) -> EmitOut {
    var out: EmitOut;
    out.clip = emit_u.mvp * vec4<f32>(in.pos, 1.0);
    if (emit_u.params.x > 0.5) {
        // A decal is flat, so a facing fade would hide it at grazing views.
        // Carry 1 at the centre falling to 0 at the rim for a soft edge.
        out.facing = 1.0 - length(in.pos.xz);
        return out;
    }
    let view_axis = normalize(vec3<f32>(emit_u.mvp[0].w, emit_u.mvp[1].w, emit_u.mvp[2].w));
    out.facing = dot(in.n, view_axis);
    return out;
}

@fragment
fn fs_emit(in: EmitOut) -> @location(0) vec4<f32> {
    // Negative alpha selects soft smoke; other additive effects stay unchanged.
    if (emit_u.color.a < 0.0) {
        let soft = pow(abs(in.facing), 3.0);
        return vec4<f32>(emit_u.color.rgb, -emit_u.color.a * soft);
    }
    return emit_u.color;
}

// Alpha-blended effects (smoke, dust, scorch): unlike the additive pass these
// can darken what is behind them, and they take the same distance fog as the
// lit world so a plume sinks into haze instead of glowing through it.
@fragment
fn fs_smoke(in: EmitOut) -> @location(0) vec4<f32> {
    var soft = pow(clamp(abs(in.facing), 0.0, 1.0), 1.5);
    if (emit_u.params.x > 0.5) {
        soft = smoothstep(0.0, 0.55, in.facing);
    }
    let rgb = mix(emit_u.color.rgb, emit_u.fog.rgb, emit_u.fog.a);
    return vec4<f32>(rgb, emit_u.color.a * soft);
}

fn blit_uv(ndc: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
}

// Screen-blend the blurred glow over the scene (x: intensity; 0 = bloom off),
// so highlights brighten without hard clipping. The HUD is drawn after this.
fn composite(uv: vec2<f32>) -> vec3<f32> {
    let c = textureSample(scene_tex, scene_samp, uv).rgb;
    let b = clamp(textureSample(bloom_tex, scene_samp, uv).rgb * bloom_u.x, vec3<f32>(0.0), vec3<f32>(1.0));
    return c + b - c * b;
}

@fragment
fn fs_blit(in: FullOut) -> @location(0) vec4<f32> {
    return vec4<f32>(composite(blit_uv(in.ndc)), 1.0);
}

@fragment
fn fs_blit_srgb(in: FullOut) -> @location(0) vec4<f32> {
    let c = composite(blit_uv(in.ndc));
    return vec4<f32>(pow(max(c, vec3<f32>(0.0)), vec3<f32>(2.2)), 1.0);
}
