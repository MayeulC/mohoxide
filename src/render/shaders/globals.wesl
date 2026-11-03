// Bind group 0 as every scene shader declares it, plus the shadow helpers
// (`render/globals.rs` holds the matching Rust `Globals`). Imported by the
// scene shaders.
import package::ranges_grid::{Ring, CELL_WORDS, OVERFLOW};

struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    params: vec4<f32>,
    overlay: vec4<f32>,
    camera: vec4<f32>,
    // w: the camera's LOD scale (moho's LOD plane = forward x scale).
    forward: vec4<f32>,
    light_vp: array<mat4x4<f32>, 3>,
    shadow: vec4<f32>,
    horizon: vec4<f32>,
    sun_color: vec4<f32>,
    ambience: vec4<f32>,
    shadow_fill: vec4<f32>,
    fog: vec4<f32>,
    terrain: vec4<f32>,
    // x: view depth where sun shadows end.
    shadow_reach: vec4<f32>,
    // x: FAF's Fidelity (0 low, 1 medium, 2 high).
    graphics: vec4<f32>,
    // Fog of war's vision circles' grid: origin (world xz), cell size,
    // circles (0: none); cells across and down.
    vision: vec4<f32>,
    vision_dims: vec4<u32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
// The terrain's heights, every sample ((width + 1) x (height + 1)).
@group(0) @binding(10) var<storage, read> heights: array<f32>;
// mesh.fx's `insectTexture` (Cybran units at Medium Fidelity).
@group(0) @binding(11) var insect_lut: texture_2d<f32>;

// The terrain mesh's vertex at sample (x, z): the same floats as its vertex
// buffer (`terrain::mesh_vertex`), so a grid built from it rasterises to the
// terrain's own depth.
fn terrain_vertex(x: u32, z: u32) -> vec3<f32> {
    let w = u32(g.terrain.y);
    return vec3<f32>(f32(x), heights[z * (w + 1u) + x], f32(z));
}

// A cell of the terrain mesh at this frame's step, as its index buffer
// triangulates it (`terrain::lod_indices`): corner `k` (0..6) of cell
// (ci, cj) in units of the step, as sample coordinates.
fn terrain_cell_corner(ci: u32, cj: u32, k: u32) -> vec2<u32> {
    var o = vec2<u32>(0u, 0u);
    switch k {
        case 1u, 4u: { o = vec2<u32>(0u, 1u); }
        case 2u, 3u: { o = vec2<u32>(1u, 0u); }
        case 5u: { o = vec2<u32>(1u, 1u); }
        default: {}
    }
    let step = u32(g.terrain.x);
    return (vec2<u32>(ci, cj) + o) * step;
}

// The frame holds gamma-space colour (see `FRAME_FORMAT`): shaders that
// light in linear space convert with this.
fn frame_rgb(linear: vec3<f32>) -> vec3<f32> {
    return pow(max(linear, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
}
@group(0) @binding(1) var overlay_tex: texture_2d<f32>;
@group(0) @binding(2) var overlay_smp: sampler;

// The grid overlay at world `xz`: nearest cell (nav / threat cells stay
// crisp).
fn overlay_value(xz: vec2<f32>) -> f32 {
    return textureSampleLevel(overlay_tex, overlay_smp, xz / g.overlay.zw, 0.0).r;
}
@group(0) @binding(3) var shadow_map: texture_depth_2d_array;
@group(0) @binding(4) var shadow_cmp: sampler_comparison;
@group(0) @binding(5) var horizon_tex: texture_2d<f32>;
// Units' environment cube (reflections / ambient), FAF's BRDF lookup, and a
// wrapping linear sampler for them.
@group(0) @binding(6) var environment_cube: texture_cube<f32>;
@group(0) @binding(7) var brdf_lut: texture_2d<f32>;
@group(0) @binding(8) var env_smp: sampler;
// The view army's fog of war (`render/scene.rs`): a level per intel cell
// (`vision::fog_level` x 85: 0 unexplored, 85 explored, 170 radar, 255
// visible).
@group(0) @binding(9) var fog_tex: texture_2d<f32>;

// Fog of war's vision circles (`render/vision.rs`), binned into the grid
// `g.vision` describes (`ranges_grid`'s layout, one grid): the circles, the
// cells' words, the cells' lists.
@group(0) @binding(12) var<storage, read> vision_rings: array<Ring>;
@group(0) @binding(13) var<storage, read> vision_cells: array<u32>;
@group(0) @binding(14) var<storage, read> vision_lists: array<u32>;

fn in_circle(r: Ring, xz: vec2<f32>) -> bool {
    let d = xz - r.center_band.xy;
    return dot(d, d) <= r.radii.x * r.radii.x;
}

// Is world `xz` inside one of the view's vision circles (moho's vision
// volumes, `vision.fx` `CastVision`)? A covered cell is inside one
// throughout, an empty one in none; an edge cell tests the circles it lists.
fn in_vision(xz: vec2<f32>) -> bool {
    let count = u32(g.vision.w);
    if (count == 0u) { return false; }
    let at = floor((xz - g.vision.xy) / g.vision.z);
    if (any(at < vec2<f32>(0.0)) || any(at >= vec2<f32>(g.vision_dims.xy))) { return false; }
    let base = 1u + (u32(at.y) * g.vision_dims.x + u32(at.x)) * CELL_WORDS;
    if (vision_cells[base] != 0u) { return true; }
    let n = vision_cells[base + 1u];
    let start = vision_cells[base + 2u];
    var inside = false;
    if (n == 0u) {
        return false;
    } else if (start == OVERFLOW) {
        for (var k = 0u; k < count && !inside; k++) {
            inside = in_circle(vision_rings[k], xz);
        }
    } else {
        for (var k = start; k < start + n && !inside; k++) {
            inside = in_circle(vision_rings[vision_lists[k]], xz);
        }
    }
    return inside;
}

// What fog of war leaves of the colour at world `xz` (gamma space, as the
// frame): 1 inside a vision circle; outside them moho's vision pass blends
// black at alpha 0.33 over the frame (`frame.fx` `VisionPS(0.33)`,
// MohoEngine.dll 0x103e5580), so 0.67, radar or not, with a hard edge per
// pixel; unexplored ground ('unexplored' games) black, bilinear between the
// intel grid's cell centres.
fn fog_factor(xz: vec2<f32>) -> f32 {
    if (g.fog.x < 0.5 || in_vision(xz)) { return 1.0; }
    let size = vec2<i32>(textureDimensions(fog_tex));
    let t = xz / g.fog.yz * vec2<f32>(size) - 0.5;
    let base = floor(t);
    let f = t - base;
    let top = size - 1;
    let a = clamp(vec2<i32>(base), vec2<i32>(0), top);
    let b = clamp(vec2<i32>(base) + 1, vec2<i32>(0), top);
    let v00 = fog_brightness(textureLoad(fog_tex, vec2<i32>(a.x, a.y), 0).r);
    let v10 = fog_brightness(textureLoad(fog_tex, vec2<i32>(b.x, a.y), 0).r);
    let v01 = fog_brightness(textureLoad(fog_tex, vec2<i32>(a.x, b.y), 0).r);
    let v11 = fog_brightness(textureLoad(fog_tex, vec2<i32>(b.x, b.y), 0).r);
    return mix(mix(v00, v10, f.x), mix(v01, v11, f.x), f.y);
}

// A fog level's brightness outside the vision circles: 0 unexplored, else
// 0.67 (explored, radar: no tint in moho's world view; a cell the grid has
// eyes on but no circle reaches, as moho's frame shows it).
fn fog_brightness(v: f32) -> f32 {
    if (v > 0.2) { return 0.67; }
    return 0.0;
}

// Sun shadow from the cascaded shadow map at `world` (1 lit, 0 shadowed):
// the cascade by view depth, a normal offset against acne, 3x3 PCF.
fn sun_shadow(world: vec3<f32>, normal: vec3<f32>) -> f32 {
    let count = i32(g.shadow.z);
    if (count == 0) { return 1.0; }
    let depth = dot(world - g.camera.xyz, g.forward.xyz);
    var c = 0;
    if (depth > g.shadow.x) { c = 1; }
    if (depth > g.shadow.y) { c = 2; }
    if (depth > g.shadow_reach.x) { return 1.0; }
    if (c >= count) { return 1.0; }
    let p = world + normal * (0.04 * f32(c + 1));
    let l = g.light_vp[c] * vec4<f32>(p, 1.0);
    let uv = vec2<f32>(l.x * 0.5 + 0.5, 0.5 - l.y * 0.5);
    if (uv.x <= 0.0 || uv.x >= 1.0 || uv.y <= 0.0 || uv.y >= 1.0 || l.z >= 1.0) { return 1.0; }
    let d = l.z - 0.0008;
    let t = g.shadow.w;
    var sum = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            sum += textureSampleCompareLevel(shadow_map, shadow_cmp, uv + vec2<f32>(f32(x), f32(y)) * t, c, d);
        }
    }
    return sum / 9.0;
}

// The terrain's own shadow on the ground at `xz` (heightfield horizon map,
// computed at map load: the sun never moves). Bilinear between texel
// centres: a texel is a world unit or two, and read nearest (the overlay
// sampler) a small shadow showed as hard-edged blocks close up.
fn terrain_shadow(xz: vec2<f32>) -> f32 {
    if (g.horizon.z < 0.5) { return 1.0; }
    let size = vec2<i32>(textureDimensions(horizon_tex));
    let t = xz * g.horizon.xy * vec2<f32>(size) - 0.5;
    let base = floor(t);
    let f = t - base;
    let top = size - 1;
    let a = clamp(vec2<i32>(base), vec2<i32>(0), top);
    let b = clamp(vec2<i32>(base) + 1, vec2<i32>(0), top);
    let v00 = textureLoad(horizon_tex, vec2<i32>(a.x, a.y), 0).r;
    let v10 = textureLoad(horizon_tex, vec2<i32>(b.x, a.y), 0).r;
    let v01 = textureLoad(horizon_tex, vec2<i32>(a.x, b.y), 0).r;
    let v11 = textureLoad(horizon_tex, vec2<i32>(b.x, b.y), 0).r;
    return mix(mix(v00, v10, f.x), mix(v01, v11, f.x), f.y);
}
