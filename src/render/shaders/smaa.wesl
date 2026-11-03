// SMAA 1x (Jimenez et al., "SMAA: Enhanced Subpixel Morphological
// Antialiasing"; reference https://github.com/iryoku/smaa, MIT, see
// `render/smaa/LICENSE.txt`), ported at its Medium preset (no diagonal or
// corner detection). Three full-screen passes over the finished gamma-space
// frame (bloom applied): luma edges, blending weights from the edges'
// lengths and crossings (the area and search lookup tables), then each
// pixel blended with the neighbour across its edge, written onto the sRGB
// surface. See `render/smaa.rs`.
struct Params {
    // 1 / width, 1 / height, width, height.
    metrics: vec4<f32>,
};
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var frame: texture_2d<f32>;
@group(0) @binding(2) var edges_tex: texture_2d<f32>;
@group(0) @binding(3) var blend_tex: texture_2d<f32>;
@group(0) @binding(4) var area_tex: texture_2d<f32>;
@group(0) @binding(5) var search_tex: texture_2d<f32>;
@group(0) @binding(6) var linear_smp: sampler;
@group(0) @binding(7) var point_smp: sampler;

// Medium preset.
const THRESHOLD: f32 = 0.1;
const MAX_SEARCH_STEPS: f32 = 8.0;
const LOCAL_CONTRAST_ADAPTATION: f32 = 2.0;
// The lookup tables' layout.
const AREATEX_MAX_DISTANCE: f32 = 16.0;
const AREATEX_PIXEL_SIZE: vec2<f32> = vec2<f32>(1.0 / 160.0, 1.0 / 560.0);
const AREATEX_SUBTEX_SIZE: f32 = 1.0 / 7.0;
const SEARCHTEX_SIZE: vec2<f32> = vec2<f32>(66.0, 33.0);
const SEARCHTEX_PACKED_SIZE: vec2<f32> = vec2<f32>(64.0, 16.0);

// One triangle covering the screen: clip position and texture coordinate.
fn screen(i: u32) -> vec4<f32> {
    let xy = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(xy * 2.0 - 1.0, xy.x, 1.0 - xy.y);
}

// HLSL's round (halves away from zero; WGSL's rounds them to even).
fn round_away(x: vec2<f32>) -> vec2<f32> {
    return sign(x) * floor(abs(x) + 0.5);
}

// ---- Pass 1: luma edges (left in r, top in g) ----

struct EdgesOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) o0: vec4<f32>,
    @location(2) o1: vec4<f32>,
    @location(3) o2: vec4<f32>,
};

@vertex fn vs_edges(@builtin(vertex_index) i: u32) -> EdgesOut {
    let s = screen(i);
    var o: EdgesOut;
    o.clip = vec4<f32>(s.xy, 0.0, 1.0);
    o.uv = s.zw;
    o.o0 = fma(p.metrics.xyxy, vec4<f32>(-1.0, 0.0, 0.0, -1.0), o.uv.xyxy);
    o.o1 = fma(p.metrics.xyxy, vec4<f32>(1.0, 0.0, 0.0, 1.0), o.uv.xyxy);
    o.o2 = fma(p.metrics.xyxy, vec4<f32>(-2.0, 0.0, 0.0, -2.0), o.uv.xyxy);
    return o;
}

fn luma_at(uv: vec2<f32>) -> f32 {
    return dot(textureSampleLevel(frame, point_smp, uv, 0.0).rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@fragment fn fs_edges(i: EdgesOut) -> @location(0) vec4<f32> {
    let l = luma_at(i.uv);
    let l_left = luma_at(i.o0.xy);
    let l_top = luma_at(i.o0.zw);
    var delta = vec4<f32>(abs(l - vec2<f32>(l_left, l_top)), 0.0, 0.0);
    var edges = step(vec2<f32>(THRESHOLD), delta.xy);
    if (dot(edges, vec2<f32>(1.0)) == 0.0) {
        discard;
    }
    let l_right = luma_at(i.o1.xy);
    let l_bottom = luma_at(i.o1.zw);
    delta = vec4<f32>(delta.xy, abs(l - vec2<f32>(l_right, l_bottom)));
    var max_delta = max(delta.xy, delta.zw);
    let l_left2 = luma_at(i.o2.xy);
    let l_top2 = luma_at(i.o2.zw);
    delta = vec4<f32>(delta.xy, abs(vec2<f32>(l_left, l_top) - vec2<f32>(l_left2, l_top2)));
    max_delta = max(max_delta, delta.zw);
    let final_delta = max(max_delta.x, max_delta.y);
    // Local contrast adaptation: a much stronger edge nearby wins.
    edges *= step(vec2<f32>(final_delta), LOCAL_CONTRAST_ADAPTATION * delta.xy);
    return vec4<f32>(edges, 0.0, 0.0);
}

// ---- Pass 2: blending weights ----

struct WeightsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) pix: vec2<f32>,
    @location(2) o0: vec4<f32>,
    @location(3) o1: vec4<f32>,
    @location(4) o2: vec4<f32>,
};

@vertex fn vs_weights(@builtin(vertex_index) i: u32) -> WeightsOut {
    let s = screen(i);
    var o: WeightsOut;
    o.clip = vec4<f32>(s.xy, 0.0, 1.0);
    o.uv = s.zw;
    o.pix = o.uv * p.metrics.zw;
    // Offsets between texels, so one bilinear fetch reads two edges.
    o.o0 = fma(p.metrics.xyxy, vec4<f32>(-0.25, -0.125, 1.25, -0.125), o.uv.xyxy);
    o.o1 = fma(p.metrics.xyxy, vec4<f32>(-0.125, -0.25, -0.125, 1.25), o.uv.xyxy);
    // Where the searches end.
    o.o2 = fma(p.metrics.xxyy, vec4<f32>(-2.0, 2.0, -2.0, 2.0) * MAX_SEARCH_STEPS, vec4<f32>(o.o0.xz, o.o1.yw));
    return o;
}

fn edge_at(uv: vec2<f32>) -> vec2<f32> {
    return textureSampleLevel(edges_tex, linear_smp, uv, 0.0).rg;
}

// How much length the search's last step adds (0, 1 or 2), from the
// bilinearly fetched edges there.
fn search_length(e: vec2<f32>, offset: f32) -> f32 {
    let scale = (SEARCHTEX_SIZE * vec2<f32>(0.5, -1.0) + vec2<f32>(-1.0, 1.0)) / SEARCHTEX_PACKED_SIZE;
    let bias = (SEARCHTEX_SIZE * vec2<f32>(offset, 1.0) + vec2<f32>(0.5, -0.5)) / SEARCHTEX_PACKED_SIZE;
    return textureSampleLevel(search_tex, linear_smp, fma(scale, e, bias), 0.0).r;
}

fn search_x_left(start: vec2<f32>, end: f32) -> f32 {
    var uv = start;
    var e = vec2<f32>(0.0, 1.0);
    while (uv.x > end && e.g > 0.8281 && e.r == 0.0) {
        e = edge_at(uv);
        uv = fma(-vec2<f32>(2.0, 0.0), p.metrics.xy, uv);
    }
    let offset = fma(-(255.0 / 127.0), search_length(e, 0.0), 3.25);
    return fma(p.metrics.x, offset, uv.x);
}

fn search_x_right(start: vec2<f32>, end: f32) -> f32 {
    var uv = start;
    var e = vec2<f32>(0.0, 1.0);
    while (uv.x < end && e.g > 0.8281 && e.r == 0.0) {
        e = edge_at(uv);
        uv = fma(vec2<f32>(2.0, 0.0), p.metrics.xy, uv);
    }
    let offset = fma(-(255.0 / 127.0), search_length(e, 0.5), 3.25);
    return fma(-p.metrics.x, offset, uv.x);
}

fn search_y_up(start: vec2<f32>, end: f32) -> f32 {
    var uv = start;
    var e = vec2<f32>(1.0, 0.0);
    while (uv.y > end && e.r > 0.8281 && e.g == 0.0) {
        e = edge_at(uv);
        uv = fma(-vec2<f32>(0.0, 2.0), p.metrics.xy, uv);
    }
    let offset = fma(-(255.0 / 127.0), search_length(e.gr, 0.0), 3.25);
    return fma(p.metrics.y, offset, uv.y);
}

fn search_y_down(start: vec2<f32>, end: f32) -> f32 {
    var uv = start;
    var e = vec2<f32>(1.0, 0.0);
    while (uv.y < end && e.r > 0.8281 && e.g == 0.0) {
        e = edge_at(uv);
        uv = fma(vec2<f32>(0.0, 2.0), p.metrics.xy, uv);
    }
    let offset = fma(-(255.0 / 127.0), search_length(e.gr, 0.5), 3.25);
    return fma(-p.metrics.y, offset, uv.y);
}

// The areas each side of an edge `dist` (square-rooted) long with crossing
// edges e1, e2 at its ends.
fn area(dist: vec2<f32>, e1: f32, e2: f32) -> vec2<f32> {
    var uv = fma(vec2<f32>(AREATEX_MAX_DISTANCE), round_away(4.0 * vec2<f32>(e1, e2)), dist);
    uv = fma(AREATEX_PIXEL_SIZE, uv, 0.5 * AREATEX_PIXEL_SIZE);
    // SMAA 1x: subsample offset 0.
    uv.y = fma(AREATEX_SUBTEX_SIZE, 0.0, uv.y);
    return textureSampleLevel(area_tex, linear_smp, uv, 0.0).rg;
}

@fragment fn fs_weights(i: WeightsOut) -> @location(0) vec4<f32> {
    var weights = vec4<f32>(0.0);
    let e = textureSampleLevel(edges_tex, linear_smp, i.uv, 0.0).rg;
    // Edge at north.
    if (e.g > 0.0) {
        var coords = vec3<f32>(search_x_left(i.o0.xy, i.o2.x), i.o1.y, 0.0);
        var d = vec2<f32>(coords.x, 0.0);
        let e1 = textureSampleLevel(edges_tex, linear_smp, coords.xy, 0.0).r;
        coords.z = search_x_right(i.o0.zw, i.o2.y);
        d.y = coords.z;
        d = abs(round_away(fma(p.metrics.zz, d, -i.pix.xx)));
        let e2 = textureSampleLevel(edges_tex, linear_smp, coords.zy, 0.0, vec2<i32>(1, 0)).r;
        let a = area(sqrt(d), e1, e2);
        weights = vec4<f32>(a, weights.zw);
    }
    // Edge at west.
    if (e.r > 0.0) {
        var coords = vec3<f32>(i.o0.x, search_y_up(i.o1.xy, i.o2.z), 0.0);
        var d = vec2<f32>(coords.y, 0.0);
        let e1 = textureSampleLevel(edges_tex, linear_smp, coords.xy, 0.0).g;
        coords.z = search_y_down(i.o1.zw, i.o2.w);
        d.y = coords.z;
        d = abs(round_away(fma(p.metrics.ww, d, -i.pix.yy)));
        let e2 = textureSampleLevel(edges_tex, linear_smp, coords.xz, 0.0, vec2<i32>(0, 1)).g;
        let a = area(sqrt(d), e1, e2);
        weights = vec4<f32>(weights.xy, a);
    }
    return weights;
}

// ---- Pass 3: neighbourhood blending ----

struct BlendOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) o: vec4<f32>,
};

@vertex fn vs_blend(@builtin(vertex_index) i: u32) -> BlendOut {
    let s = screen(i);
    var o: BlendOut;
    o.clip = vec4<f32>(s.xy, 0.0, 1.0);
    o.uv = s.zw;
    o.o = fma(p.metrics.xyxy, vec4<f32>(1.0, 0.0, 0.0, 1.0), o.uv.xyxy);
    return o;
}

fn colour_at(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(frame, linear_smp, uv, 0.0).rgb;
}

// Gamma-space colour onto the sRGB surface (as the bloom's own add does).
fn finish(c: vec3<f32>) -> vec4<f32> {
    return vec4<f32>(pow(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(2.2)), 1.0);
}

@fragment fn fs_blend(i: BlendOut) -> @location(0) vec4<f32> {
    let here = textureSampleLevel(blend_tex, linear_smp, i.uv, 0.0);
    // Right, top, left, bottom weights.
    let a = vec4<f32>(textureSampleLevel(blend_tex, linear_smp, i.o.xy, 0.0).a, textureSampleLevel(blend_tex, linear_smp, i.o.zw, 0.0).g, here.z, here.x);
    if (dot(a, vec4<f32>(1.0)) < 1e-5) {
        return finish(colour_at(i.uv));
    }
    let h = max(a.x, a.z) > max(a.y, a.w);
    let offset = select(vec4<f32>(0.0, a.y, 0.0, a.w), vec4<f32>(a.x, 0.0, a.z, 0.0), h);
    var weight = select(a.yw, a.xz, h);
    weight /= dot(weight, vec2<f32>(1.0));
    let coord = fma(offset, vec4<f32>(p.metrics.xy, -p.metrics.xy), i.uv.xyxy);
    return finish(weight.x * colour_at(coord.xy) + weight.y * colour_at(coord.zw));
}
