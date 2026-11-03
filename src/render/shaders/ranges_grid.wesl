// The range overlays' ring grid (`render/ranges.rs`), shared by the pass
// that bins rings into it (`ranges_bin`) and the one that draws from it
// (`ranges`). Each group of rings has a grid over the ground its rings
// reach: a cell is covered when it lies wholly inside one ring's radius less
// its line (no outer line can show there), and lists the rings that could
// still draw in it: every ring reaching it, or on a covered cell those whose
// minimum range line reaches it. The tests are conservative (`slack`), so a
// cell never misses a ring a point inside it would find.

// One ring: xy = centre (world xz), zw = the height band; outer radius,
// outer line, minimum radius (0: none), its line.
struct Ring {
    center_band: vec4<f32>,
    radii: vec4<f32>,
};

// A group's grid: origin (world xz) and cell size; size in cells, its first
// cell in the cell buffer, 1 when any ring has a minimum range; its rings.
struct Grid {
    origin_cell: vec4<f32>,
    dims: vec4<u32>,
    rings: vec4<u32>,
};

// Per cell in the cell buffer (after the list allocator's word): covered,
// list length, list start, fill cursor.
const CELL_WORDS: u32 = 4u;
// A list start for a cell whose list didn't fit: its pixels test every ring
// of the group.
const OVERFLOW: u32 = 0xffffffffu;

fn cell_word(g: Grid, cell: u32, word: u32) -> u32 {
    return 1u + (g.dims.z + cell) * CELL_WORDS + word;
}

// The rounding margin for cell tests against a ring of radius `r`.
fn slack(g: Grid, r: f32) -> f32 {
    return g.origin_cell.z * 1e-3 + r * 1e-5 + 1e-3;
}

// Nearest and farthest distance from `c` to the cell at `at`.
fn cell_distances(g: Grid, at: vec2<u32>, c: vec2<f32>) -> vec2<f32> {
    let lo = g.origin_cell.xy + vec2<f32>(at) * g.origin_cell.z;
    let hi = lo + g.origin_cell.z;
    let near = max(max(lo - c, c - hi), vec2<f32>(0.0));
    let far = max(abs(c - lo), abs(c - hi));
    return vec2<f32>(length(near), length(far));
}
