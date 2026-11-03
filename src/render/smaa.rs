//! SMAA 1x (`shaders/smaa.wesl`, the reference's Medium preset): FAF's
//! Anti-Aliasing option with key 1. The bloom's add writes the gamma-space
//! frame into [`Smaa::input`]; three full-screen passes find its luma edges,
//! work out how much each pixel along an edge blends with its neighbour
//! (from the edge's length and the crossing edges at its ends, through the
//! precomputed area and search tables in `smaa/`), and write the blended
//! frame onto the sRGB surface. Presentation only (FA offered MSAA).

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::globals::FRAME_FORMAT;

/// The three passes, in order (GPU labels).
const PASSES: [&str; 3] = ["SMAA: edges", "SMAA: blend weights", "SMAA: resolve"];

const SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/smaa.wgsl"));
/// The reference's lookup tables (`Textures/AreaTex.h`, `SearchTex.h`):
/// 160 x 560 RG8 and 64 x 16 R8.
const AREA: &[u8] = include_bytes!("smaa/area.bin");
const SEARCH: &[u8] = include_bytes!("smaa/search.bin");

const EDGES_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg8Unorm;
const WEIGHTS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    /// 1 / width, 1 / height, width, height.
    metrics: [f32; 4],
}

/// The per-size targets and the passes' bind groups.
struct Targets {
    input: wgpu::TextureView,
    edges: wgpu::TextureView,
    weights: wgpu::TextureView,
    groups: [wgpu::BindGroup; 3],
}

pub struct Smaa {
    /// The finished gamma-space frame (the bloom's add writes it).
    pub input: wgpu::TextureView,
    edges: wgpu::TextureView,
    weights: wgpu::TextureView,
    params: wgpu::Buffer,
    area: wgpu::TextureView,
    search: wgpu::TextureView,
    linear: wgpu::Sampler,
    point: wgpu::Sampler,
    layouts: [wgpu::BindGroupLayout; 3],
    pipelines: [wgpu::RenderPipeline; 3],
    /// Edges, weights, blend.
    groups: [wgpu::BindGroup; 3],
    size: (u32, u32),
}

impl Smaa {
    /// `format`: the surface the blend writes to.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, w: u32, h: u32) -> Self {
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty, count: None };
        let uniform = wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None };
        let texture = wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false };
        let filtering = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);
        let layout = |label, entries: &[wgpu::BindGroupLayoutEntry]| device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(label), entries });
        // The shader's bindings: 0 params, 1 frame, 2 edges, 3 weights,
        // 4 area, 5 search, 6 linear, 7 point; each pass binds what it reads.
        let layouts = [
            layout("SMAA: edges", &[entry(0, uniform), entry(1, texture), entry(7, filtering)]),
            layout("SMAA: weights", &[entry(0, uniform), entry(2, texture), entry(4, texture), entry(5, texture), entry(6, filtering)]),
            layout("SMAA: blend", &[entry(0, uniform), entry(1, texture), entry(3, texture), entry(6, filtering)]),
        ];
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("SMAA: shader"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let pipeline = |i: usize, vs: &str, fs: &str, target: wgpu::TextureFormat| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(PASSES[i]), bind_group_layouts: &[Some(&layouts[i])], immediate_size: 0 });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(PASSES[i]),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some(vs), buffers: &[], compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    targets: &[Some(wgpu::ColorTargetState { format: target, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipelines = [pipeline(0, "vs_edges", "fs_edges", EDGES_FORMAT), pipeline(1, "vs_weights", "fs_weights", WEIGHTS_FORMAT), pipeline(2, "vs_blend", "fs_blend", format)];
        let table = |label, (w, h), format, data: &[u8]| {
            device
                .create_texture_with_data(
                    queue,
                    &wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    data,
                )
                .create_view(&wgpu::TextureViewDescriptor { label: Some(label), ..Default::default() })
        };
        let area = table("SMAA: area", (160, 560), wgpu::TextureFormat::Rg8Unorm, AREA);
        let search = table("SMAA: search", (64, 16), wgpu::TextureFormat::R8Unorm, SEARCH);
        let sampler = |label, filter| {
            device.create_sampler(&wgpu::SamplerDescriptor { label: Some(label), mag_filter: filter, min_filter: filter, ..Default::default() })
        };
        let (linear, point) = (sampler("SMAA: linear sampler", wgpu::FilterMode::Linear), sampler("SMAA: point sampler", wgpu::FilterMode::Nearest));
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SMAA: params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let t = Self::targets(device, &layouts, &params, &area, &search, &linear, &point, w, h);
        Self { input: t.input, edges: t.edges, weights: t.weights, params, area, search, linear, point, layouts, pipelines, groups: t.groups, size: (w, h) }
    }

    #[allow(clippy::too_many_arguments)]
    fn targets(
        device: &wgpu::Device,
        layouts: &[wgpu::BindGroupLayout; 3],
        params: &wgpu::Buffer,
        area: &wgpu::TextureView,
        search: &wgpu::TextureView,
        linear: &wgpu::Sampler,
        point: &wgpu::Sampler,
        w: u32,
        h: u32,
    ) -> Targets {
        let target = |label, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor { label: Some(label), ..Default::default() })
        };
        let (input, edges, weights) = (target("SMAA: input", FRAME_FORMAT), target("SMAA: edges", EDGES_FORMAT), target("SMAA: weights", WEIGHTS_FORMAT));
        let view = |binding, v| wgpu::BindGroupEntry { binding, resource: wgpu::BindingResource::TextureView(v) };
        let smp = |binding, s| wgpu::BindGroupEntry { binding, resource: wgpu::BindingResource::Sampler(s) };
        let p = || wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() };
        let group = |i: usize, entries: &[wgpu::BindGroupEntry]| device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some(PASSES[i]), layout: &layouts[i], entries });
        let groups = [
            group(0, &[p(), view(1, &input), smp(7, point)]),
            group(1, &[p(), view(2, &edges), view(4, area), view(5, search), smp(6, linear)]),
            group(2, &[p(), view(1, &input), view(3, &weights), smp(6, linear)]),
        ];
        Targets { input, edges, weights, groups }
    }

    /// Uploads this frame's parameters.
    pub fn prepare(&self, queue: &wgpu::Queue) {
        let (w, h) = (self.size.0.max(1) as f32, self.size.1.max(1) as f32);
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&Params { metrics: [1.0 / w, 1.0 / h, w, h] }));
    }

    pub fn resize(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        let t = Self::targets(device, &self.layouts, &self.params, &self.area, &self.search, &self.linear, &self.point, w, h);
        (self.input, self.edges, self.weights, self.groups, self.size) = (t.input, t.edges, t.weights, t.groups, (w, h));
    }

    /// Antialiases [`Smaa::input`] onto `target`.
    pub fn apply(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let _span = rts_core_engine::prof_span!("smaa");
        for (i, view) in [&self.edges, &self.weights, target].into_iter().enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(PASSES[i]),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: super::gpu_prof::pass_writes(PASSES[i]),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipelines[i]);
            pass.set_bind_group(0, &self.groups[i], &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
