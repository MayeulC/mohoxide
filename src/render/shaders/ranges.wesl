// The strategic range overlays (`render/ranges.rs`), moho's `RangeMask` /
// `RangeFill` / `RangeBurn` (`frame.fx`) without its per-ring volumes: one
// full-screen pass per group finds the ground under each pixel (from the
// terrain's depth) and draws the edge where the point is inside a ring's
// radius but not inside any ring's radius less its line (so overlapping
// rings show one outline), or on a minimum range's line. The rings it tests
// are the ones its grid cell lists (`ranges_grid`, binned by `ranges_bin`).
// The ground inside isn't tinted: FA's `RangeFill` leaves it as it is in the
// game.
import package::ranges_grid::{Ring, Grid, OVERFLOW, cell_word};

struct Frame {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    // xyz = eye.
    eye: vec4<f32>,
    // x = water height (-1e9 without water), yz = the view's size in pixels.
    params: vec4<f32>,
    // xy = the view's top left in the frame, pixels.
    origin: vec4<f32>,
};
// A group of rings in one colour: rgb, a = glow; x = its grid.
struct Group {
    color: vec4<f32>,
    index: vec4<u32>,
};
@group(0) @binding(0) var<uniform> f: Frame;
@group(0) @binding(1) var terrain_depth: texture_depth_2d;
@group(0) @binding(2) var<storage, read> rings: array<Ring>;
@group(0) @binding(3) var<storage, read> grids: array<Grid>;
@group(0) @binding(4) var<storage, read> cells: array<u32>;
@group(0) @binding(5) var<storage, read> lists: array<u32>;
@group(1) @binding(0) var<uniform> grp: Group;

// The ground under pixel `px` (on the water's surface where the ground is
// under water); w = its depth, 0 where nothing was drawn.
fn ground(px: vec2<i32>) -> vec4<f32> {
    let d = textureLoad(terrain_depth, px, 0);
    if (d >= 1.0) {
        return vec4<f32>(0.0);
    }
    let at = vec2<f32>(px) + 0.5 - f.origin.xy;
    let ndc = vec2<f32>(at.x / f.params.y * 2.0 - 1.0, 1.0 - at.y / f.params.z * 2.0);
    let h = f.inv_view_proj * vec4<f32>(ndc, d, 1.0);
    var p = h.xyz / h.w;
    var depth = d;
    if (p.y < f.params.x) {
        let e = f.eye.xyz;
        p = e + (p - e) * ((f.params.x - e.y) / (p.y - e.y));
        let clip = f.view_proj * vec4<f32>(p, 1.0);
        depth = clip.z / clip.w;
    }
    // (The stored depth itself on dry ground: re-projecting the point
    // rounds, and the depth test would drop pixels in rows.)
    return vec4<f32>(p, max(depth, 1e-7));
}

struct Screen {
    @builtin(position) clip: vec4<f32>,
};

// One triangle covering the screen.
@vertex fn vs_screen(@builtin(vertex_index) i: u32) -> Screen {
    let xy = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: Screen;
    o.clip = vec4<f32>(xy * 2.0 - 1.0, 0.0, 1.0);
    return o;
}

struct Burn {
    @location(0) color: vec4<f32>,
    @location(1) glow: vec4<f32>,
    // At the ground's depth, so units standing in the way hide it.
    @builtin(frag_depth) depth: f32,
};

// Ring `r` at ground distance `d`, ORed into `m`: x = inside its radius,
// y = inside its radius less its line, z = on its minimum range's line.
fn test_ring(m: vec3<bool>, r: vec4<f32>, d: f32) -> vec3<bool> {
    return m | vec3<bool>(d <= r.x, d <= r.x - r.y, r.z > 0.0 && d <= r.z && d >= r.z - r.w);
}

// `RangeBurn`: the line, opaque in the group's colour, with its glow.
@fragment fn fs_composite(i: Screen) -> Burn {
    let px = vec2<i32>(i.clip.xy);
    let p = ground(px);
    if (p.w == 0.0) {
        discard;
    }
    let g = grids[grp.index.x];
    let at = floor((p.xz - g.origin_cell.xy) / g.origin_cell.z);
    if (any(at < vec2<f32>(0.0)) || any(at >= vec2<f32>(g.dims.xy))) {
        discard;
    }
    let cell = u32(at.y) * g.dims.x + u32(at.x);
    let n = cells[cell_word(g, cell, 1u)];
    if (n == 0u) {
        discard;
    }
    // A covered cell is inside some ring less its line throughout.
    var m = vec3<bool>(false, cells[cell_word(g, cell, 0u)] != 0u, false);
    let start = cells[cell_word(g, cell, 2u)];
    // Stop at a minimum range line, or inside a ring less its line when no
    // ring has one.
    if (start == OVERFLOW) {
        for (var k = g.rings.x; k < g.rings.x + g.rings.y && !m.z && !(m.y && g.dims.w == 0u); k++) {
            m = test_ring(m, rings[k].radii, distance(p.xz, rings[k].center_band.xy));
        }
    } else {
        for (var k = start; k < start + n && !m.z && !(m.y && g.dims.w == 0u); k++) {
            let r = rings[lists[k]];
            m = test_ring(m, r.radii, distance(p.xz, r.center_band.xy));
        }
    }
    if (!((m.x && !m.y) || m.z)) {
        discard;
    }
    var o: Burn;
    o.depth = p.w;
    o.color = vec4<f32>(grp.color.rgb, 1.0);
    o.glow = vec4<f32>(grp.color.a, 0.0, 0.0, 1.0);
    return o;
}
