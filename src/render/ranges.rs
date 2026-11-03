//! The strategic range overlays (FAF's military / intel toggles, moho's
//! `RangeMask` / `RangeFill` / `RangeBurn` in `frame.fx`). Moho draws each
//! ring as a cylinder into the stencil buffer and colours what the volumes
//! leave; FAF's binary patch clamps the cylinders to the map's height band
//! (lowest ground - 5 to the highest) to cut fill, and draws selected units'
//! rings once (in their selected colour, not also in the normal pass).
//!
//! Here no ring draws a volume, which shaded every pixel under every ring's
//! volume once per ring (thousands of times a pixel under a big selection).
//! After the terrain the depth is copied (the ground's depth, before
//! units). Each group of rings (a filter's normal, selected or rollover
//! colour) gets a grid over the ground its rings reach, binned each frame by
//! compute passes (`shaders/ranges_bin.wesl`): a cell lying wholly inside one
//! ring's radius less its line is covered (no outer line shows there), and
//! each cell lists the rings that could still draw in it. Then a full-screen
//! pass per group finds the ground under each pixel and tests its cell's
//! rings: the edge is where the point is inside a ring but not inside any
//! ring less its line (overlapping rings show one outline), or on a minimum
//! range's line, opaque in the group's colour with its glow. The test per
//! pixel is the one the volumes made, so the picture is the same; the cost
//! is the screen's pixels plus the cells along the outlines. The ground
//! inside isn't tinted (the game doesn't). That pass is depth-tested at the
//! ground's depth, so units standing on a ring hide it. Over water the rings
//! lie on the water's surface.
//!
//! Presentation only.

use bytemuck::{Pod, Zeroable};
use glam::Mat4;

use super::globals::{FRAME_FORMAT, GLOW_FORMAT};

const SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/ranges.wgsl"));
const BIN_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/ranges_bin.wgsl"));
/// Group uniforms are this far apart (the dynamic offset alignment).
const GROUP_STRIDE: u64 = 256;
const MAX_GROUPS: usize = 64;
/// The cells of all a view's grids, at most.
const MAX_CELLS: usize = 1 << 20;
/// Words per cell (`ranges_grid.wesl` `CELL_WORDS`), after the list
/// allocator's word.
pub(super) const CELL_WORDS: usize = 4;
/// List entries at most; a cell whose list doesn't fit tests every ring of
/// its group.
pub(super) const MAX_LIST: usize = 1 << 22;
/// A cell is about this fraction of a group's mean radius across.
const CELLS_PER_RADIUS: f32 = 4.0;
/// Compute workgroups per dispatch dimension.
const MAX_DISPATCH: u32 = 65535;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniform {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    /// x = water height (-1e9 without), yz = the view's size.
    params: [f32; 4],
    /// xy = the view's top left in the frame.
    origin: [f32; 4],
}

/// One ring: centre (world xz) and the height band its volume spans; outer
/// radius and line width, minimum radius (0: none) and its line width.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub struct Ring {
    pub center_band: [f32; 4],
    pub radii: [f32; 4],
}

/// Rings drawn in one colour (rgb, a = glow).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    pub color: [f32; 4],
    pub rings: Vec<Ring>,
}

/// A group's colour and the index of its grid.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GroupUniform {
    color: [f32; 4],
    index: [u32; 4],
}

/// A group's grid (`ranges_grid.wesl` `Grid`): origin (world xz), cell size;
/// cells across and down, its first cell, 1 when a ring has a minimum range;
/// its first ring and ring count.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub(super) struct Grid {
    pub origin_cell: [f32; 4],
    pub dims: [u32; 4],
    pub rings: [u32; 4],
}

/// The grids for `groups` (rings numbered across them in order) within
/// `max_cells` cells in all, all their cells, and a bound on the list
/// entries the binning can make (each ring listed at most once per cell of
/// its square).
pub(super) fn grids(groups: &[&Group], max_cells: usize) -> (Vec<Grid>, usize, usize) {
    let per_group = (max_cells / groups.len().max(1)) as f32;
    let (mut out, mut cells, mut bound, mut first) = (Vec::with_capacity(groups.len()), 0usize, 0usize, 0u32);
    for g in groups {
        let (mut lo, mut hi, mut sum) = ([f32::MAX; 2], [f32::MIN; 2], 0.0);
        for r in &g.rings {
            let reach = r.radii[0] + 1.0;
            lo = [lo[0].min(r.center_band[0] - reach), lo[1].min(r.center_band[1] - reach)];
            hi = [hi[0].max(r.center_band[0] + reach), hi[1].max(r.center_band[1] + reach)];
            sum += r.radii[0];
        }
        let (w, h) = ((hi[0] - lo[0]).max(1.0), (hi[1] - lo[1]).max(1.0));
        let mean = sum / g.rings.len() as f32;
        // As fine as the rings want, within the group's share of cells.
        let cell = (mean / CELLS_PER_RADIUS).max((w * h / per_group).sqrt()).max(0.01);
        let (nx, nz) = ((w / cell).ceil() as u32 + 1, (h / cell).ceil() as u32 + 1);
        let has_min = g.rings.iter().any(|r| r.radii[2] > 0.0) as u32;
        out.push(Grid { origin_cell: [lo[0], lo[1], cell, 0.0], dims: [nx, nz, cells as u32, has_min], rings: [first, g.rings.len() as u32, 0, 0] });
        for r in &g.rings {
            let side = (2.0 * r.radii[0] / cell).ceil() as usize + 2;
            bound += side.min(nx as usize) * side.min(nz as usize);
        }
        cells += (nx * nz) as usize;
        first += g.rings.len() as u32;
    }
    (out, cells, bound)
}

/// Bins rings into their groups' grids on the GPU
/// (`shaders/ranges_bin.wesl`): the range overlays' rings and fog of war's
/// vision circles.
pub(super) struct Binner {
    layout: wgpu::BindGroupLayout,
    /// The binning passes, in order: cover, count, alloc, fill.
    pipelines: [wgpu::ComputePipeline; 4],
}

impl Binner {
    pub(super) fn new(device: &wgpu::Device, label: &str) -> Self {
        let compute = wgpu::ShaderStages::COMPUTE;
        let buffer = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: compute, ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let storage = |read_only| wgpu::BufferBindingType::Storage { read_only };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(label),
            entries: &[buffer(0, wgpu::BufferBindingType::Uniform), buffer(1, storage(true)), buffer(2, storage(true)), buffer(3, storage(false)), buffer(4, storage(false))],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(BIN_SHADER.into()) });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipelines = ["cs_cover", "cs_count", "cs_alloc", "cs_fill"].map(|entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        });
        Self { layout, pipelines }
    }

    /// Binds a set of buffers: `params` (rings, groups, cells in all grids,
    /// list capacity), the rings, the grids, the cell words and the lists.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn bind(&self, device: &wgpu::Device, label: &str, params: &wgpu::Buffer, rings: &wgpu::Buffer, grids: &wgpu::Buffer, cells: &wgpu::Buffer, lists: &wgpu::Buffer) -> wgpu::BindGroup {
        fn entry(binding: u32, b: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry { binding, resource: b.as_entire_binding() }
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some(label), layout: &self.layout, entries: &[entry(0, params), entry(1, rings), entry(2, grids), entry(3, cells), entry(4, lists)] })
    }

    /// Clears the first `words` cell words of `cells` and bins `rings`
    /// rings into the grids `bind` holds.
    pub(super) fn run(&self, encoder: &mut wgpu::CommandEncoder, label: &str, bind: &wgpu::BindGroup, cells: &wgpu::Buffer, rings: u32, words: u32) {
        encoder.clear_buffer(cells, 0, Some(words as u64 * 4));
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some(label), timestamp_writes: super::gpu_prof::compute_pass_writes(label) });
        pass.set_bind_group(0, bind, &[]);
        let per_ring = (rings.min(MAX_DISPATCH), rings.div_ceil(MAX_DISPATCH));
        let per_cell = (words / CELL_WORDS as u32).div_ceil(64);
        for (pipeline, (x, y)) in self.pipelines.iter().zip([per_ring, per_ring, (per_cell, 1), per_ring]) {
            pass.set_pipeline(pipeline);
            pass.dispatch_workgroups(x, y, 1);
        }
    }
}

/// The overlays' pipelines and the ground's depth, shared by the world views
/// (each draws its own [`RangesFrame`] in turn).
pub struct Ranges {
    layout: wgpu::BindGroupLayout,
    group_layout: wgpu::BindGroupLayout,
    composite_pipeline: wgpu::RenderPipeline,
    binner: Binner,
    terrain_depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    /// Bumped when the depth copy is remade (views rebind theirs).
    generation: u64,
    /// List entries at most ([`MAX_LIST`]; tests lower it to draw through
    /// the overflow path).
    pub(super) max_list: usize,
}

/// A world view's rings this frame ([`Ranges::prepare`]): its uniforms,
/// rings, grids and bind groups.
#[derive(Default)]
pub struct RangesFrame {
    gpu: Option<FrameGpu>,
    /// One per group.
    draws: usize,
    /// Rings, and cell words to clear.
    rings: u32,
    cells: u32,
}

struct FrameGpu {
    frame: wgpu::Buffer,
    groups: wgpu::Buffer,
    group_bind: wgpu::BindGroup,
    params: wgpu::Buffer,
    grids: wgpu::Buffer,
    rings: wgpu::Buffer,
    cells: wgpu::Buffer,
    lists: wgpu::Buffer,
    /// For the composite and the binning, made for `Ranges::generation` and
    /// the buffers as they are.
    bind: wgpu::BindGroup,
    bin_bind: wgpu::BindGroup,
    generation: u64,
}

impl RangesFrame {
    /// Anything to draw this frame?
    pub fn active(&self) -> bool {
        self.draws > 0
    }
}

fn target(device: &wgpu::Device, label: &str, (w, h): (u32, u32), format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

pub(super) fn storage(device: &wgpu::Device, label: &str, bytes: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: bytes.max(16), usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

/// `buffer` at least `bytes` long (grown to a power of two); true if remade.
pub(super) fn fit(device: &wgpu::Device, buffer: &mut wgpu::Buffer, label: &str, bytes: u64) -> bool {
    if buffer.size() >= bytes {
        return false;
    }
    *buffer = storage(device, label, bytes.next_power_of_two());
    true
}

impl Ranges {
    pub fn new(device: &wgpu::Device, depth_format: wgpu::TextureFormat, w: u32, h: u32) -> Self {
        let uniform = |binding, visibility, dynamic| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: dynamic, min_binding_size: None },
            count: None,
        };
        let buffer = |binding, visibility, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ranges: layout"),
            entries: &[
                uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT, false),
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: fragment, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                buffer(2, fragment, true),
                buffer(3, fragment, true),
                buffer(4, fragment, true),
                buffer(5, fragment, true),
            ],
        });
        let group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("ranges: filter group layout"), entries: &[uniform(0, fragment, true)] });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("ranges: shader"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ranges: pipeline layout"), bind_group_layouts: &[Some(&layout), Some(&group_layout)], immediate_size: 0 });
        let over = Some(wgpu::BlendState::ALPHA_BLENDING);
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ranges: composite"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_screen"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_composite"),
                targets: &[
                    Some(wgpu::ColorTargetState { format: FRAME_FORMAT, blend: over, write_mask: wgpu::ColorWrites::COLOR }),
                    Some(wgpu::ColorTargetState { format: GLOW_FORMAT, blend: over, write_mask: wgpu::ColorWrites::ALL }),
                ],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState { format: depth_format, depth_write_enabled: Some(false), depth_compare: Some(wgpu::CompareFunction::LessEqual), stencil: Default::default(), bias: Default::default() }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let binner = Binner::new(device, "ranges: binning");
        let (terrain_depth, depth_view) = Self::targets(device, depth_format, (w, h));
        Self { layout, group_layout, composite_pipeline, binner, terrain_depth, depth_view, generation: 0, max_list: MAX_LIST }
    }

    fn targets(device: &wgpu::Device, depth_format: wgpu::TextureFormat, size: (u32, u32)) -> (wgpu::Texture, wgpu::TextureView) {
        let terrain_depth = target(device, "ranges: ground depth", size, depth_format, wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING);
        let depth_view = super::view(&terrain_depth, "ranges: ground depth");
        (terrain_depth, depth_view)
    }

    pub fn resize(&mut self, device: &wgpu::Device, depth_format: wgpu::TextureFormat, w: u32, h: u32) {
        (self.terrain_depth, self.depth_view) = Self::targets(device, depth_format, (w, h));
        self.generation += 1;
    }

    /// `out`'s buffers, made as needed.
    fn gpu<'a>(&self, device: &wgpu::Device, out: &'a mut Option<FrameGpu>) -> &'a mut FrameGpu {
        out.get_or_insert_with(|| {
            let uniform = |label, size| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            let frame = uniform("ranges: frame", std::mem::size_of::<FrameUniform>() as u64);
            let groups = uniform("ranges: filter groups", GROUP_STRIDE * MAX_GROUPS as u64);
            let params = uniform("ranges: binning params", 16);
            let group_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ranges: filter groups"),
                layout: &self.group_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &groups, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<GroupUniform>() as u64) }) }],
            });
            let grids = storage(device, "ranges: grids", (MAX_GROUPS * std::mem::size_of::<Grid>()) as u64);
            let (rings, cells, lists) = (storage(device, "ranges: rings", 4096), storage(device, "ranges: grid cells", 4096), storage(device, "ranges: cell lists", 4096));
            let (bind, bin_bind) = self.binds(device, &frame, &params, &rings, &grids, &cells, &lists);
            FrameGpu { frame, groups, group_bind, params, grids, rings, cells, lists, bind, bin_bind, generation: self.generation }
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn binds(&self, device: &wgpu::Device, frame: &wgpu::Buffer, params: &wgpu::Buffer, rings: &wgpu::Buffer, grids: &wgpu::Buffer, cells: &wgpu::Buffer, lists: &wgpu::Buffer) -> (wgpu::BindGroup, wgpu::BindGroup) {
        fn entry(binding: u32, b: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry { binding, resource: b.as_entire_binding() }
        }
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ranges: composite"),
            layout: &self.layout,
            entries: &[entry(0, frame), wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.depth_view) }, entry(2, rings), entry(3, grids), entry(4, cells), entry(5, lists)],
        });
        let bin_bind = self.binner.bind(device, "ranges: binning", params, rings, grids, cells, lists);
        (bind, bin_bind)
    }

    /// Uploads a view's rings this frame into `out`, for the view in `rect`
    /// (frame pixels: left, top, right, bottom; groups past [`MAX_GROUPS`]
    /// are dropped), and the grids the binning fills.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(&self, device: &wgpu::Device, queue: &wgpu::Queue, view_proj: Mat4, eye: [f32; 3], water: Option<f32>, rect: [f32; 4], groups: &[Group], out: &mut RangesFrame) {
        out.draws = 0;
        let groups: Vec<&Group> = groups.iter().filter(|g| !g.rings.is_empty()).take(MAX_GROUPS).collect();
        if groups.is_empty() {
            return;
        }
        let gpu = self.gpu(device, &mut out.gpu);
        let rings: Vec<Ring> = groups.iter().flat_map(|g| g.rings.iter().copied()).collect();
        let (grids, cells, bound) = grids(&groups, MAX_CELLS);
        let words = 1 + cells * CELL_WORDS;
        let lists = bound.min(self.max_list);
        let mut remade = fit(device, &mut gpu.rings, "ranges: rings", std::mem::size_of_val(rings.as_slice()) as u64);
        remade |= fit(device, &mut gpu.cells, "ranges: grid cells", (words * 4) as u64);
        remade |= fit(device, &mut gpu.lists, "ranges: cell lists", (lists * 4) as u64);
        if remade || gpu.generation != self.generation {
            (gpu.bind, gpu.bin_bind) = self.binds(device, &gpu.frame, &gpu.params, &gpu.rings, &gpu.grids, &gpu.cells, &gpu.lists);
            gpu.generation = self.generation;
        }
        queue.write_buffer(&gpu.rings, 0, bytemuck::cast_slice(&rings));
        queue.write_buffer(&gpu.grids, 0, bytemuck::cast_slice(&grids));
        queue.write_buffer(&gpu.params, 0, bytemuck::cast_slice(&[rings.len() as u32, groups.len() as u32, cells as u32, (gpu.lists.size() / 4).min(self.max_list as u64) as u32]));
        for (i, g) in groups.iter().enumerate() {
            queue.write_buffer(&gpu.groups, i as u64 * GROUP_STRIDE, bytemuck::bytes_of(&GroupUniform { color: g.color, index: [i as u32, 0, 0, 0] }));
        }
        (out.draws, out.rings, out.cells) = (groups.len(), rings.len() as u32, words as u32);
        let frame = FrameUniform {
            view_proj: view_proj.to_cols_array_2d(),
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            eye: [eye[0], eye[1], eye[2], 0.0],
            params: [water.unwrap_or(-1e9), rect[2] - rect[0], rect[3] - rect[1], 0.0],
            origin: [rect[0], rect[1], 0.0, 0.0],
        };
        queue.write_buffer(&gpu.frame, 0, bytemuck::bytes_of(&frame));
    }

    /// Keeps the ground's depth (call after the terrain, before units).
    pub fn copy_ground_depth(&self, encoder: &mut wgpu::CommandEncoder, depth: &wgpu::Texture) {
        encoder.copy_texture_to_texture(depth.as_image_copy(), self.terrain_depth.as_image_copy(), self.terrain_depth.size());
    }

    /// Bins a view's rings (`rings`) into their grids and draws its overlays
    /// onto the frame (`frame`, `glow`) in `view`, tested against the
    /// scene's `depth`.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, frame: &wgpu::TextureView, glow: &wgpu::TextureView, depth: &wgpu::TextureView, view: &super::camera::View, rings: &RangesFrame) {
        let _span = rts_core_engine::prof_span!("range overlays");
        let Some(gpu) = &rings.gpu else { return };
        if rings.draws == 0 {
            return;
        }
        self.binner.run(encoder, "ranges: bin rings", &gpu.bin_bind, &gpu.cells, rings.rings, rings.cells);
        let keep = wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store };
        let target = |view| Some(wgpu::RenderPassColorAttachment { view, resolve_target: None, depth_slice: None, ops: keep });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ranges: composite"),
            color_attachments: &[target(frame), target(glow)],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }), stencil_ops: None }),
            timestamp_writes: super::gpu_prof::pass_writes("ranges: composite"),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        view.bind(&mut pass);
        pass.set_pipeline(&self.composite_pipeline);
        pass.set_bind_group(0, &gpu.bind, &[]);
        for i in 0..rings.draws {
            pass.set_bind_group(1, &gpu.group_bind, &[(i as u64 * GROUP_STRIDE) as u32]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(x: f32, z: f32, r: f32, min: f32) -> Ring {
        Ring { center_band: [x, z, 0.0, 50.0], radii: [r, 0.5, min, 0.2] }
    }

    /// Each group's grid covers its rings' discs, its cells follow the last
    /// group's and its rings the last group's; a huge spread stays within
    /// the cell budget.
    #[test]
    fn grids_cover_their_rings_within_the_budget() {
        let blob = Group { color: [1.0; 4], rings: (0..100).map(|i| ring(100.0 + (i % 10) as f32 * 3.0, 200.0 + (i / 10) as f32 * 3.0, 30.0, 0.0)).collect() };
        let spread = Group { color: [0.5; 4], rings: (0..64).map(|i| ring((i % 8) as f32 * 5000.0, (i / 8) as f32 * 5000.0, 5.0, 2.0)).collect() };
        let (grids, cells, bound) = grids(&[&blob, &spread], MAX_CELLS);
        assert_eq!(grids.len(), 2);
        assert!(cells <= MAX_CELLS, "{cells} cells");
        let [a, b] = [grids[0], grids[1]];
        assert_eq!((a.rings[0], a.rings[1], b.rings[0], b.rings[1]), (0, 100, 100, 64));
        assert_eq!((a.dims[2], b.dims[2]), (0, a.dims[0] * a.dims[1]));
        assert_eq!((a.dims[3], b.dims[3]), (0, 1), "only the second group has minimum ranges");
        // Blob: cells a quarter of the radius; every disc inside the grid.
        assert_eq!(a.origin_cell[2], 7.5);
        for (g, group) in [(a, &blob), (b, &spread)] {
            let size = [g.dims[0] as f32 * g.origin_cell[2], g.dims[1] as f32 * g.origin_cell[2]];
            for r in &group.rings {
                for (axis, c) in [(0, r.center_band[0]), (1, r.center_band[1])] {
                    assert!(c - r.radii[0] >= g.origin_cell[axis] && c + r.radii[0] <= g.origin_cell[axis] + size[axis]);
                }
            }
        }
        assert!(bound >= 100, "at least a cell a ring: {bound}");
    }
}
