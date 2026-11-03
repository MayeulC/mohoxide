// The interface's quads (`render/ui.rs`).
struct Screen { size: vec2<f32>, pad: vec2<f32> };
@group(0) @binding(0) var<uniform> screen: Screen;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
struct VOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec4<f32> };
@vertex fn vs(@location(0) p: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VOut {
    var o: VOut;
    o.pos = vec4<f32>(p.x / screen.size.x * 2.0 - 1.0, 1.0 - p.y / screen.size.y * 2.0, 0.0, 1.0);
    o.uv = uv;
    o.color = color;
    return o;
}
@fragment fn fs(i: VOut) -> @location(0) vec4<f32> {
    return textureSample(atlas, samp, i.uv) * i.color;
}

// Movies (`DrawItem::Movie`): MPEG-1's YCbCr 4:2:0 planes, BT.601 studio
// range (pl_mpeg's `plm_frame_to_rgb` coefficients), to linear RGB.
@group(0) @binding(3) var plane_y: texture_2d<f32>;
@group(0) @binding(4) var plane_cb: texture_2d<f32>;
@group(0) @binding(5) var plane_cr: texture_2d<f32>;
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}
@fragment fn fs_movie(i: VOut) -> @location(0) vec4<f32> {
    let y = (textureSample(plane_y, samp, i.uv).r - 16.0 / 255.0) * 1.164;
    let cb = textureSample(plane_cb, samp, i.uv).r - 128.0 / 255.0;
    let cr = textureSample(plane_cr, samp, i.uv).r - 128.0 / 255.0;
    let rgb = clamp(vec3<f32>(y + 1.596 * cr, y - 0.391 * cb - 0.813 * cr, y + 2.018 * cb), vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(srgb_to_linear(rgb), 1.0) * i.color;
}
