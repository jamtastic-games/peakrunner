// Bloom: a glow mask rides in the resolved scene colour's alpha channel
// (glow = 1 - alpha; opaque unlit surfaces write 1). The prefilter keeps only
// that masked light, a dual-Kawase chain blurs it at half, quarter, ... size,
// and the blit screen-blends the result over the scene before the HUD.
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct FullOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_bloom(@builtin(vertex_index) i: u32) -> FullOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = positions[i];
    var out: FullOut;
    out.clip = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    return out;
}

fn masked(uv: vec2<f32>) -> vec3<f32> {
    let c = textureSampleLevel(src, samp, uv, 0.0);
    return c.rgb * clamp(1.0 - c.a, 0.0, 1.0);
}

fn texel() -> vec2<f32> {
    return 1.0 / vec2<f32>(textureDimensions(src));
}

// Full-resolution scene to half resolution, keeping only masked glow.
@fragment
fn fs_prefilter(in: FullOut) -> @location(0) vec4<f32> {
    let t = texel();
    var s = masked(in.uv) * 4.0;
    s += masked(in.uv + vec2<f32>(-t.x, -t.y));
    s += masked(in.uv + vec2<f32>(t.x, -t.y));
    s += masked(in.uv + vec2<f32>(-t.x, t.y));
    s += masked(in.uv + vec2<f32>(t.x, t.y));
    return vec4<f32>(s / 8.0, 1.0);
}

fn tap(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(src, samp, uv, 0.0).rgb;
}

// Dual-Kawase downsample: centre plus four half-texel diagonals.
@fragment
fn fs_down(in: FullOut) -> @location(0) vec4<f32> {
    let h = texel() * 0.5;
    var s = tap(in.uv) * 4.0;
    s += tap(in.uv - h);
    s += tap(in.uv + h);
    s += tap(in.uv + vec2<f32>(h.x, -h.y));
    s += tap(in.uv - vec2<f32>(h.x, -h.y));
    return vec4<f32>(s / 8.0, 1.0);
}

// Dual-Kawase upsample, blended additively onto the next larger level.
@fragment
fn fs_up(in: FullOut) -> @location(0) vec4<f32> {
    let h = texel() * 0.5;
    var s = tap(in.uv + vec2<f32>(-h.x * 2.0, 0.0));
    s += tap(in.uv + vec2<f32>(-h.x, h.y)) * 2.0;
    s += tap(in.uv + vec2<f32>(0.0, h.y * 2.0));
    s += tap(in.uv + vec2<f32>(h.x, h.y)) * 2.0;
    s += tap(in.uv + vec2<f32>(h.x * 2.0, 0.0));
    s += tap(in.uv + vec2<f32>(h.x, -h.y)) * 2.0;
    s += tap(in.uv + vec2<f32>(0.0, -h.y * 2.0));
    s += tap(in.uv + vec2<f32>(-h.x, -h.y)) * 2.0;
    return vec4<f32>(s / 12.0, 1.0);
}
