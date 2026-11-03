//! Fog of war's vision circles: the clear ground drawn as circles round the
//! view army's side's vision sources, as moho draws it, while what shows of
//! other armies still follows the sim's intel grids (`snapshot::fog`).
//!
//! Moho (MohoEngine.dll 0x103e5580, `vision.fx` `CastVision`, `frame.fx`
//! `Vision`): each frame it takes the circles of the session's vision tree
//! (one per user entity with vision whose army is the focus army or its
//! ally, at the entity's position and radius lerped between the last two
//! ticks; UserEntity::UpdateEntityData 0x1047b2ac, 0x103e4350) that reach
//! the terrain in view, and draws each as a 45-sided cylinder (unit circle
//! scaled by the radius, y -256 to 256; built at 0x103e5292) into the
//! stencil, z-fail counted, 4096 instances a draw; then blends black at
//! alpha 0.33 over every pixel no volume holds. So the edge is per pixel and
//! hard: a point of the frame is clear when its world xz lies in a circle.
//!
//! Here the shaders that fog the frame (`globals.wesl` `fog_factor`) test
//! the point against the circles directly: the circles are binned each
//! frame on the GPU into a grid over the ground they reach (the range
//! overlays' binning, `ranges.rs`): a cell wholly inside a circle is clear
//! throughout, an empty one fogged, and an edge cell lists the circles to
//! test. Not matched: the volumes' 45-gon (ours round, at most 0.25% of the
//! radius further out between its corners) and their height (moho's leave
//! ground above y 256 fogged).
//!
//! Presentation only.

use super::ranges::{self, Binner, Grid, Group, Ring, CELL_WORDS, MAX_LIST};

/// Cells of the vision grid at most.
const MAX_CELLS: usize = 1 << 18;
/// The volumes' height (`vision.fx` cylinders span y -256 to 256): the
/// circles are culled against the views with it.
pub const BAND: [f32; 2] = [-256.0, 256.0];

pub struct VisionCircles {
    binner: Binner,
    params: wgpu::Buffer,
    grid: wgpu::Buffer,
    /// What group 0 binds for the shaders (`globals.wesl`).
    pub rings: wgpu::Buffer,
    pub cells: wgpu::Buffer,
    pub lists: wgpu::Buffer,
    bin_bind: wgpu::BindGroup,
    /// This frame's circles, `[x, z, radius]` (filled by the scene).
    pub circles: Vec<[f32; 3]>,
    /// Reused: the circles as rings.
    group: Group,
    /// This frame: rings binned and cell words to clear.
    count: u32,
    words: u32,
    grid_now: Grid,
}

impl VisionCircles {
    pub fn new(device: &wgpu::Device) -> Self {
        let binner = Binner::new(device, "vision: binning");
        let params = device.create_buffer(&wgpu::BufferDescriptor { label: Some("vision: binning params"), size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let grid = ranges::storage(device, "vision: grid", std::mem::size_of::<Grid>() as u64);
        let (rings, cells, lists) = (ranges::storage(device, "vision: circles", 4096), ranges::storage(device, "vision: grid cells", 4096), ranges::storage(device, "vision: cell lists", 4096));
        let bin_bind = binner.bind(device, "vision: binning", &params, &rings, &grid, &cells, &lists);
        Self { binner, params, grid, rings, cells, lists, bin_bind, circles: Vec::new(), group: Group::default(), count: 0, words: 0, grid_now: Grid { origin_cell: [0.0; 4], dims: [0; 4], rings: [0; 4] } }
    }

    /// Uploads this frame's circles (`self.circles`) and their grid. True
    /// when a buffer was remade (group 0 must be bound again).
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        self.count = 0;
        if self.circles.is_empty() {
            return false;
        }
        self.group.rings.clear();
        self.group.rings.extend(self.circles.iter().map(|&[x, z, r]| Ring { center_band: [x, z, BAND[0], BAND[1]], radii: [r, 0.0, 0.0, 0.0] }));
        let (grids, cells, bound) = ranges::grids(&[&self.group], MAX_CELLS);
        let words = 1 + cells * CELL_WORDS;
        let rings: &[Ring] = &self.group.rings;
        let mut remade = ranges::fit(device, &mut self.rings, "vision: circles", std::mem::size_of_val(rings) as u64);
        remade |= ranges::fit(device, &mut self.cells, "vision: grid cells", (words * 4) as u64);
        remade |= ranges::fit(device, &mut self.lists, "vision: cell lists", (bound.min(MAX_LIST) * 4) as u64);
        if remade {
            self.bin_bind = self.binner.bind(device, "vision: binning", &self.params, &self.rings, &self.grid, &self.cells, &self.lists);
        }
        queue.write_buffer(&self.rings, 0, bytemuck::cast_slice(rings));
        queue.write_buffer(&self.grid, 0, bytemuck::cast_slice(&grids));
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&[rings.len() as u32, 1, cells as u32, (self.lists.size() / 4).min(MAX_LIST as u64) as u32]));
        (self.count, self.words, self.grid_now) = (rings.len() as u32, words as u32, grids[0]);
        remade
    }

    /// Bins the frame's circles (before any pass that fogs).
    pub fn bin(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.count > 0 {
            self.binner.run(encoder, "vision: bin circles", &self.bin_bind, &self.cells, self.count, self.words);
        }
    }

    /// The shaders' grid (`Globals::vision`, `vision_dims`): origin (world
    /// xz), cell size, circles (0: none, all fogged); cells across, down.
    pub fn globals(&self) -> ([f32; 4], [u32; 4]) {
        let g = &self.grid_now;
        ([g.origin_cell[0], g.origin_cell[1], g.origin_cell[2], self.count as f32], [g.dims[0], g.dims[1], 0, 0])
    }
}

/// Could a circle `[x, z, radius]`'s volume (y over [`BAND`]) show in the
/// view with these frustum planes (`(n, d)`, `n . p + d >= 0` inside)?
pub fn circle_in_view(planes: &[glam::Vec4; 6], [x, z, r]: [f32; 3]) -> bool {
    let centre = glam::Vec3::new(x, (BAND[0] + BAND[1]) * 0.5, z);
    let half = glam::Vec3::new(r, (BAND[1] - BAND[0]) * 0.5, r);
    planes.iter().all(|p| {
        let n = p.truncate();
        n.dot(centre) + p.w + n.abs().dot(half) >= 0.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Circles blend by id between snapshots; a new source shows where it
    /// is, a gone one is dropped, `keep` filters.
    #[test]
    fn circles_blend_between_snapshots() {
        use crate::engine::snapshot::{blend_vision, VisionSource};
        let s = |id, x: f32, r: f32| VisionSource { id, position: [x, 0.0], radius: r };
        let previous = [s(1, 0.0, 20.0), s(2, 100.0, 20.0), s(4, 50.0, 10.0)];
        let latest = [s(1, 10.0, 30.0), s(3, 200.0, 5.0), s(4, 60.0, 10.0)];
        let mut out = Vec::new();
        blend_vision(&latest, &previous, 0.5, |c| c[0] < 100.0, &mut out);
        assert_eq!(out, vec![[5.0, 0.0, 25.0], [55.0, 0.0, 10.0]]);
    }

    /// A circle off to one side of a view is culled, one in front kept, one
    /// reaching into it from outside kept.
    #[test]
    fn circles_cull_against_the_view() {
        let view = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 1.0, 1000.0) * glam::camera::rh::view::look_at_mat4(glam::Vec3::new(0.0, 100.0, 0.0), glam::Vec3::new(0.0, 0.0, -100.0), glam::Vec3::Y);
        let r = |i: usize| view.row(i);
        let planes = [r(3) + r(0), r(3) - r(0), r(3) + r(1), r(3) - r(1), r(2), r(3) - r(2)].map(|p| p / p.truncate().length());
        assert!(circle_in_view(&planes, [0.0, -100.0, 20.0]));
        assert!(!circle_in_view(&planes, [2000.0, -100.0, 20.0]));
        assert!(!circle_in_view(&planes, [0.0, 400.0, 20.0]), "behind the camera");
        assert!(circle_in_view(&planes, [0.0, 400.0, 1000.0]), "a huge circle round the camera");
    }
}
