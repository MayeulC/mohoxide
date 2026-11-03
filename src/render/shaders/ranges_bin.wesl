// Bins the range overlays' rings into their groups' grids
// (`ranges_grid`), each frame on the GPU: mark covered cells, count each
// cell's rings, give each cell its slice of the list buffer, fill the
// slices. A ring's work is one workgroup over the cells of its square; the
// order rings land in a list is whatever the atomics give, which the drawing
// pass doesn't depend on (it ORs over the list).
import package::ranges_grid::{Ring, Grid, CELL_WORDS, OVERFLOW, cell_word, slack, cell_distances};

// x = rings, y = groups, z = cells in all grids, w = list capacity.
struct Params {
    counts: vec4<u32>,
};
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> rings: array<Ring>;
@group(0) @binding(2) var<storage, read> grids: array<Grid>;
// Word 0 = the list allocator, then `CELL_WORDS` per cell.
@group(0) @binding(3) var<storage, read_write> cells: array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> lists: array<u32>;

const WORKGROUP: u32 = 64u;

// The ring a workgroup bins (rings past 65535 go up in y).
fn ring_index(wg: vec3<u32>) -> u32 {
    return wg.x + wg.y * 65535u;
}

fn group_of(i: u32) -> Grid {
    var g = grids[0];
    for (var j = 0u; j < params.counts.y; j++) {
        g = grids[j];
        if (i < g.rings.x + g.rings.y) {
            break;
        }
    }
    return g;
}

// The cells of ring `r`'s square: first cell, size.
fn ring_cells(g: Grid, r: Ring) -> vec4<u32> {
    let reach = r.radii.x + slack(g, r.radii.x);
    let lo = clamp(floor((r.center_band.xy - reach - g.origin_cell.xy) / g.origin_cell.z), vec2<f32>(0.0), vec2<f32>(g.dims.xy - 1u));
    let hi = clamp(floor((r.center_band.xy + reach - g.origin_cell.xy) / g.origin_cell.z), vec2<f32>(0.0), vec2<f32>(g.dims.xy - 1u));
    let a = vec2<u32>(lo);
    return vec4<u32>(a, vec2<u32>(hi) - a + 1u);
}

// Can ring `r` draw in a cell `d` (nearest, farthest) from it: it reaches the
// cell, and the cell isn't covered (`covered`) or its minimum range line
// reaches it.
fn needed(g: Grid, r: Ring, d: vec2<f32>, covered: bool) -> bool {
    let s = slack(g, r.radii.x);
    if (d.x > r.radii.x + s) {
        return false;
    }
    if (!covered) {
        return true;
    }
    return r.radii.z > 0.0 && d.x <= r.radii.z + s && d.y >= r.radii.z - r.radii.w - s;
}

@compute @workgroup_size(64) fn cs_cover(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let i = ring_index(wg);
    if (i >= params.counts.x) {
        return;
    }
    let r = rings[i];
    let core = r.radii.x - r.radii.y;
    if (core <= 0.0) {
        return;
    }
    let g = group_of(i);
    let box = ring_cells(g, r);
    for (var t = li; t < box.z * box.w; t += WORKGROUP) {
        let at = box.xy + vec2<u32>(t % box.z, t / box.z);
        let d = cell_distances(g, at, r.center_band.xy);
        if (d.y <= core - slack(g, r.radii.x)) {
            atomicMax(&cells[cell_word(g, at.y * g.dims.x + at.x, 0u)], 1u);
        }
    }
}

@compute @workgroup_size(64) fn cs_count(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let i = ring_index(wg);
    if (i >= params.counts.x) {
        return;
    }
    let r = rings[i];
    let g = group_of(i);
    let box = ring_cells(g, r);
    for (var t = li; t < box.z * box.w; t += WORKGROUP) {
        let at = box.xy + vec2<u32>(t % box.z, t / box.z);
        let cell = at.y * g.dims.x + at.x;
        let covered = atomicLoad(&cells[cell_word(g, cell, 0u)]) != 0u;
        if (needed(g, r, cell_distances(g, at, r.center_band.xy), covered)) {
            atomicAdd(&cells[cell_word(g, cell, 1u)], 1u);
        }
    }
}

// One invocation per cell of every grid: takes the cell's slice of the list
// buffer (or marks it overflowed).
@compute @workgroup_size(64) fn cs_alloc(@builtin(global_invocation_id) id: vec3<u32>) {
    let c = id.x;
    if (c >= params.counts.z) {
        return;
    }
    let base = 1u + c * CELL_WORDS;
    let n = atomicLoad(&cells[base + 1u]);
    if (n == 0u) {
        return;
    }
    let start = atomicAdd(&cells[0], n);
    atomicStore(&cells[base + 2u], select(OVERFLOW, start, start + n <= params.counts.w));
}

@compute @workgroup_size(64) fn cs_fill(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let i = ring_index(wg);
    if (i >= params.counts.x) {
        return;
    }
    let r = rings[i];
    let g = group_of(i);
    let box = ring_cells(g, r);
    for (var t = li; t < box.z * box.w; t += WORKGROUP) {
        let at = box.xy + vec2<u32>(t % box.z, t / box.z);
        let cell = at.y * g.dims.x + at.x;
        let covered = atomicLoad(&cells[cell_word(g, cell, 0u)]) != 0u;
        if (!needed(g, r, cell_distances(g, at, r.center_band.xy), covered)) {
            continue;
        }
        let start = atomicLoad(&cells[cell_word(g, cell, 2u)]);
        if (start == OVERFLOW) {
            continue;
        }
        lists[start + atomicAdd(&cells[cell_word(g, cell, 3u)], 1u)] = i;
    }
}
