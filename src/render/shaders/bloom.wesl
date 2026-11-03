// Moho's bloom passes (`frame.fx`); see `render/bloom.rs`.
struct Params {
    copy: vec4<f32>,
    blur: vec4<f32>,
};
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var second: texture_2d<f32>;
@group(0) @binding(3) var smp: sampler;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One triangle covering the screen.
@vertex fn vs(@builtin(vertex_index) i: u32) -> Out {
    let xy = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: Out;
    o.clip = vec4<f32>(xy * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(xy.x, 1.0 - xy.y);
    return o;
}

// `MinimumGlow`: FA's water marker sits below it.
const MINIMUM_GLOW: f32 = 0.02;

// `CopyGlowingPS` at one point.
fn glowing(uv: vec2<f32>) -> vec4<f32> {
    let c = textureSample(source, smp, uv).rgb;
    let a = clamp((textureSample(second, smp, uv).r - MINIMUM_GLOW) * p.copy.x + p.copy.y, 0.0, 1.0);
    return vec4<f32>(c * a, a * a);
}

// Four points a quarter of a buffer texel out: at half size each is one
// frame texel (the 2x2 one bilinear sample read); on scaled displays'
// smaller buffers each a 2x2 average, covering the buffer texel.
@fragment fn fs_copy(i: Out) -> @location(0) vec4<f32> {
    let q = 0.25 * p.blur.yz;
    return 0.25 * (glowing(i.uv + vec2<f32>(-q.x, -q.y)) + glowing(i.uv + vec2<f32>(q.x, -q.y)) + glowing(i.uv + vec2<f32>(-q.x, q.y)) + glowing(i.uv + vec2<f32>(q.x, q.y)));
}

const WEIGHTS = array<f32, 7>(0.102734, 0.120985, 0.176033, 0.199471, 0.176033, 0.120985, 0.102734);

fn blur(uv: vec2<f32>, step: vec2<f32>) -> vec4<f32> {
    var c = vec4<f32>(0.0);
    for (var k = 0; k < 7; k++) {
        c += WEIGHTS[k] * textureSample(source, smp, uv + step * f32(k - 3));
    }
    return c * p.blur.x;
}

// `BlurHorizontalPS` / `BlurVerticalPS`.
@fragment fn fs_blur_h(i: Out) -> @location(0) vec4<f32> {
    return blur(i.uv, vec2<f32>(p.blur.y, 0.0));
}

@fragment fn fs_blur_v(i: Out) -> @location(0) vec4<f32> {
    return blur(i.uv, vec2<f32>(0.0, p.blur.z));
}

// `TFrameAdd` (One, One) in gamma space, kept there for SMAA.
@fragment fn fs_add_gamma(i: Out) -> @location(0) vec4<f32> {
    let scene = textureSample(source, smp, i.uv).rgb;
    let glow = textureSample(second, smp, i.uv).rgb;
    return vec4<f32>(clamp(scene + glow, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

// `TFrameAdd` (One, One) in gamma space, then onto the sRGB surface.
@fragment fn fs_add(i: Out) -> @location(0) vec4<f32> {
    let scene = textureSample(source, smp, i.uv).rgb;
    let glow = textureSample(second, smp, i.uv).rgb;
    return vec4<f32>(pow(clamp(scene + glow, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(2.2)), 1.0);
}
