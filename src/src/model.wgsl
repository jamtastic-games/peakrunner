// Authored models (src/model.rs): textured, with a glow texture whose alpha
// marks where the team's color is painted, and when a model carries them a
// metallic-roughness map (green, blue), an occlusion map (red) and a
// tangent-space normal map.
//
// Lit by the map's own look (map.wgsl): the same hemisphere ambient, sun
// color and exposure, so a model's diffuse light is exactly the ground's
// under it. On top of that it is physically based: the albedo is linearized,
// metals lose their diffuse, the sun gives a GGX highlight and the sky a
// reflection (the procedural sky's gradient, blurred by roughness and
// weighted by an analytic environment BRDF), and the result is encoded back
// into the gamma space the rest of the frame is drawn in.
struct ModelUniforms {
    mvp: mat4x4<f32>,
    model: mat4x4<f32>,
    normal: mat3x3<f32>,
    cam: vec3<f32>,
    // Glow strength for this draw (0: off).
    glow: f32,
    sun: vec3<f32>,
    density: f32,
    fog: vec3<f32>,
    // How brightly the painted plates light themselves (`ModelDraw::shine`:
    // the rabbit's gold, so it shows in any shade).
    shine: f32,
    team: vec3<f32>,
    // Team armor (`ModelDraw::plates`): how much of the plates the
    // artwork leaves bare take the team's color too (0: only the mask).
    plates: f32,
    // The material's base color and emissive factors; emissive.w is 1 for
    // a material that's team paint all over, or a draw painted all over (a
    // round's color).
    base: vec4<f32>,
    emissive: vec4<f32>,
    // Metalness and roughness factors, 1 when the material has a normal
    // map, and the map's exposure.
    surface: vec4<f32>,
    // The map's look (map.wgsl `U`): sun color, sky and ground ambient
    // (w: 1 for a procedural sky), and the sky's zenith and horizon.
    sun_color: vec4<f32>,
    amb_sky: vec4<f32>,
    amb_ground: vec4<f32>,
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    // Sun shadows (`ShadowGpu`): world to the shadow map's clip space, and
    // x: texel size, y: depth bias, z: strength, w: 1 when there is a map.
    light_vp: mat4x4<f32>,
    shadow: vec4<f32>,
    // x: 1 for a skinned part, bent by its bone palette (`bones`); its
    // `model` is then the body's frame.
    skin: vec4<f32>,
}

// Team armor's bare plates: a darker tone of the team's color than its
// paint, so the paint still reads as the accent, with half their metal
// traded for paint.
const PLATE_TONE: f32 = 0.7;
const PLATE_PAINT: f32 = 0.5;

@group(0) @binding(0) var<uniform> m: ModelUniforms;
@group(1) @binding(0) var albedo_tex: texture_2d<f32>;
@group(1) @binding(1) var glow_tex: texture_2d<f32>;
@group(1) @binding(2) var samp: sampler;
@group(1) @binding(3) var surface_tex: texture_2d<f32>;
@group(1) @binding(4) var occlusion_tex: texture_2d<f32>;
@group(1) @binding(5) var normal_tex: texture_2d<f32>;
@group(2) @binding(0) var shadow_map: texture_depth_2d;
@group(2) @binding(1) var shadow_samp: sampler_comparison;
// The draw's bone palette: each bone's posed frame times its inverse rest
// frame, in the body's frame (scene.rs `PALETTE`).
@group(3) @binding(0) var<uniform> bones: array<mat4x4<f32>, 16>;

struct In {
    @location(0) pos: vec3<f32>,
    @location(1) n: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) joints: vec4<f32>,
    @location(5) weights: vec4<f32>,
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) n: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
}

// A skinned vertex's blend of its bones (identity for a rigid part).
fn skinning(v: In) -> mat4x4<f32> {
    if (m.skin.x < 0.5) {
        return mat4x4<f32>(vec4<f32>(1.0, 0.0, 0.0, 0.0), vec4<f32>(0.0, 1.0, 0.0, 0.0), vec4<f32>(0.0, 0.0, 1.0, 0.0), vec4<f32>(0.0, 0.0, 0.0, 1.0));
    }
    let j = vec4<u32>(v.joints + vec4<f32>(0.5));
    return bones[min(j.x, 15u)] * v.weights.x + bones[min(j.y, 15u)] * v.weights.y
        + bones[min(j.z, 15u)] * v.weights.z + bones[min(j.w, 15u)] * v.weights.w;
}

@vertex
fn vs_model(v: In) -> Out {
    var out: Out;
    let bend = skinning(v);
    let p = bend * vec4<f32>(v.pos, 1.0);
    let local_n = (bend * vec4<f32>(v.n, 0.0)).xyz;
    out.world_pos = (m.model * p).xyz;
    out.n = m.normal * local_n;
    out.tangent = vec4<f32>((m.model * bend * vec4<f32>(v.tangent.xyz, 0.0)).xyz, v.tangent.w);
    out.uv = v.uv;
    out.clip = m.mvp * p;
    return out;
}

// Depth from the sun for the shadow map (`ShadowGpu`): a caster's world
// position into the light's clip space.
@vertex
fn vs_model_shadow(v: In) -> @builtin(position) vec4<f32> {
    return m.light_vp * m.model * skinning(v) * vec4<f32>(v.pos, 1.0);
}

// shaders.wgsl's highlight shoulder, so models and the world grade alike.
fn tone_shoulder(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    let t = 0.8;
    let rolled = vec3<f32>(t) + (1.0 - t) * (vec3<f32>(1.0) - exp(-(x - vec3<f32>(t)) / (1.0 - t)));
    return select(x, rolled, x > vec3<f32>(t));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> { return pow(max(c, vec3<f32>(0.0)), vec3<f32>(2.2)); }
fn to_gamma(c: vec3<f32>) -> vec3<f32> { return pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2)); }

// The sky seen along `d` (gamma, like the look's colors): the procedural
// sky's zenith-to-horizon gradient, or for a painted sky the ambient sky
// fading to the fog at the horizon; below the horizon the ground's bounce.
fn sky_along(d: vec3<f32>) -> vec3<f32> {
    var up = mix(m.fog, m.amb_sky.rgb * 1.25, pow(max(d.y, 0.0), 0.6));
    if (m.amb_sky.w > 0.5) {
        up = mix(m.horizon.rgb, m.zenith.rgb, pow(max(d.y, 0.0), 0.55));
    }
    let down = mix(up, m.amb_ground.rgb * 0.8, smoothstep(0.0, 0.35, -d.y));
    return select(up, down, d.y < 0.0);
}

// GGX with Smith visibility and Schlick fresnel for one light, times N.L.
fn ggx(n: vec3<f32>, vd: vec3<f32>, l: vec3<f32>, rough: f32, f0: vec3<f32>) -> vec3<f32> {
    let h = normalize(vd + l);
    let ndv = max(dot(n, vd), 1e-4);
    let ndl = max(dot(n, l), 0.0);
    let ndh = max(dot(n, h), 0.0);
    let vdh = max(dot(vd, h), 0.0);
    let a = rough * rough;
    let a2 = a * a;
    let denom = ndh * ndh * (a2 - 1.0) + 1.0;
    let d = a2 / (3.14159 * denom * denom);
    let k = a * 0.5;
    let vis = 1.0 / ((ndv * (1.0 - k) + k) * (ndl * (1.0 - k) + k));
    let f = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - vdh, 5.0);
    return d * vis * 0.25 * f * ndl;
}

// The split-sum environment BRDF, fitted analytically (no lookup table).
fn env_brdf(f0: vec3<f32>, rough: f32, ndv: f32) -> vec3<f32> {
    let r = rough * vec4<f32>(-1.0, -0.0275, -0.572, 0.022) + vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let a004 = min(r.x * r.x, exp2(-9.28 * ndv)) * r.x + r.y;
    let ab = vec2<f32>(-1.04, 1.04) * a004 + r.zw;
    return f0 * ab.x + vec3<f32>(ab.y);
}

// How much sun reaches `p` past the casters in the shadow map: 1 in the
// open, falling to 0 in a caster's shadow (a 3x3 filter of comparisons).
fn sunlit(p: vec3<f32>, n: vec3<f32>) -> f32 {
    if (m.shadow.w < 0.5) { return 1.0; }
    // Push the lookup off the surface along its normal, by a texel or two,
    // so a surface never shadows itself.
    let q = m.light_vp * vec4<f32>(p + n * m.shadow.x * 1.5, 1.0);
    let c = q.xyz / q.w;
    let uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || c.z > 1.0) { return 1.0; }
    let texel = 1.0 / vec2<f32>(textureDimensions(shadow_map));
    var lit = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            lit += textureSampleCompareLevel(shadow_map, shadow_samp, uv + vec2<f32>(f32(x), f32(y)) * texel, c.z - m.shadow.y);
        }
    }
    // Fade toward the map's edge, so its square never shows.
    let edge = smoothstep(0.0, 0.08, min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y)));
    return mix(1.0, lit / 9.0, m.shadow.z * edge);
}

@fragment
fn fs_model(v: Out) -> @location(0) vec4<f32> {
    let t = textureSample(albedo_tex, samp, v.uv);
    let e = textureSample(glow_tex, samp, v.uv);
    let g = e.rgb * m.emissive.rgb;
    var albedo = t.rgb * m.base.rgb;
    // Team texels hold their shading as gray around 0.5: the team's color
    // times that shading.
    let team_mask = max(e.a, m.emissive.w);
    let shade = dot(albedo, vec3<f32>(0.333, 0.334, 0.333)) * 2.0;
    // Team armor: the bare plates (the light and mid grays) take the team's
    // color as well, with the artwork's shading and wear; the dark
    // undersuit, the glow strips and the masked paint stay as drawn.
    let glowing = clamp(max(g.r, max(g.g, g.b)) * 4.0, 0.0, 1.0);
    let bare = m.plates * smoothstep(0.24, 0.42, shade * 0.5) * (1.0 - team_mask) * (1.0 - glowing);
    albedo = mix(albedo, m.team * shade, team_mask);
    albedo = mix(albedo, m.team * shade * PLATE_TONE, bare);
    // What the paint covers, for its self-light.
    let painted = albedo * min(team_mask + bare, 1.0);
    let orm = textureSample(surface_tex, samp, v.uv);
    let metal = clamp(m.surface.x * orm.b, 0.0, 1.0) * (1.0 - bare * PLATE_PAINT);
    let rough = clamp(m.surface.y * orm.g, 0.06, 1.0);
    let ao = textureSample(occlusion_tex, samp, v.uv).r;

    var n = normalize(v.n);
    if (!all(n == n)) { n = vec3<f32>(0.0, 1.0, 0.0); }
    // The normal map, in the tangent frame the file's tangents give.
    if (m.surface.z > 0.5 && dot(v.tangent.xyz, v.tangent.xyz) > 1e-8) {
        let tn = textureSample(normal_tex, samp, v.uv).xyz * 2.0 - vec3<f32>(1.0);
        let tg = normalize(v.tangent.xyz - n * dot(n, v.tangent.xyz));
        let bt = cross(n, tg) * select(1.0, -1.0, v.tangent.w < 0.0);
        let mapped = normalize(tg * tn.x + bt * tn.y + n * tn.z);
        if (all(mapped == mapped)) { n = mapped; }
    }
    let vd = normalize(m.cam - v.world_pos);
    let sun = normalize(m.sun);
    let ndl = max(dot(n, sun), 0.0);
    let lit = sunlit(v.world_pos, normalize(v.n));

    // Diffuse: the map's light sum (gamma), applied to the linear albedo
    // as one product, which is the ground's shading exactly.
    let hemi = mix(m.amb_ground.rgb, m.amb_sky.rgb, clamp(n.y * 0.5 + 0.5, 0.0, 1.0));
    let sun_g = m.sun_color.rgb * 0.45;
    let light = hemi * ao + sun_g * ndl * lit;
    let base_l = to_linear(albedo);
    var col = base_l * (1.0 - metal) * to_linear(light);
    // The sun's own linear strength (what it adds over the ambient), for
    // its highlight.
    let sun_l = max(to_linear(hemi + sun_g) - to_linear(hemi), vec3<f32>(0.0));
    let f0 = mix(vec3<f32>(0.04), base_l, metal);
    col = col + ggx(n, vd, sun, rough, f0) * sun_l * 3.14159 * lit;
    // The sky's reflection: sharp for smooth surfaces, the sky's average
    // for rough ones; reflections into the surface come from the ground,
    // and occlusion hides them in crevices.
    let ndv = max(dot(n, vd), 1e-4);
    let r = reflect(-vd, n);
    let sharp = sky_along(r);
    let broad = mix(m.amb_ground.rgb, m.amb_sky.rgb, clamp(r.y * 0.5 + 0.5, 0.0, 1.0));
    let env = to_linear(mix(sharp, broad, rough * rough));
    let spec_ao = clamp(pow(ndv + ao, exp2(-16.0 * rough - 1.0)) - 1.0 + ao, 0.0, 1.0);
    col = col + env * env_brdf(f0, rough, ndv) * spec_ao;

    var rgb = to_gamma(col);
    let glow_light = g * m.glow + painted * m.shine;
    rgb = tone_shoulder((rgb + glow_light) * m.surface.w);
    let dist = length(m.cam - v.world_pos);
    let density = select(0.0072, m.density, m.density > 0.0);
    let fog = clamp(1.0 - exp(-dist * density), 0.0, 0.92);
    rgb = mix(rgb, m.fog, fog);
    let glow = clamp((max(glow_light.r, max(glow_light.g, glow_light.b)) - 0.2) / 0.8, 0.0, 1.0) * (1.0 - fog);
    return vec4<f32>(rgb, 1.0 - glow);
}
