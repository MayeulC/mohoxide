//! Moho's bloom (`frame.fx`, driven by the exe's `CRenFrame` code): the
//! scene renders into [`Bloom::scene`] with each pixel's glow beside it in
//! [`Bloom::glow`] (FA's frame alpha); the glowing part is copied into a
//! half-size buffer (`TCopyGlowingStuff`: colour times
//! `saturate((glow - 0.02) * ren_BloomGlowCopyScale + map bloom)`), blurred
//! `ren_BloomBlurCount` times horizontally then vertically (7 taps, each
//! pass scaled by `ren_BloomBlurKernelScale`) and added back over the frame
//! (`TFrameAdd`). Like D3D9 FA, the copy, blur and add work on gamma-space
//! colour. The constants are the exe's defaults (see
//! `docs/research/verifying-against-the-binary.md`); the map's `Bloom` lighting value is
//! the copy's `GlowCopyAdd`.
//!
//! Moho's buffers are half the frame in pixels, so its glow spreads a
//! fixed number of pixels. On a scaled display (Retina, 2x) that would be
//! half as far against the interface-sized icons and lines, so ours are
//! half the frame in interface pixels (`w / (2 x scale)`): every glow
//! looks as it does on an unscaled screen. The copy averages four samples
//! a buffer texel (at 1x exactly the texels the one bilinear sample read),
//! so thin glows still reach the smaller buffers.

use bytemuck::{Pod, Zeroable};

use super::globals::{FRAME_FORMAT, GLOW_FORMAT};

/// `ren_BloomBlurCount`'s default.
pub const BLUR_COUNT: usize = 2;
/// `ren_BloomBlurKernelScale`'s default.
pub const BLUR_SCALE: f32 = 1.5;
/// `ren_BloomGlowCopyScale`'s default.
pub const GLOW_COPY_SCALE: f32 = 2.0;
/// The half-size glow buffers' format (8-bit, gamma-space colour).
const BUFFER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    /// x = glow copy scale, y = glow copy add (the map's bloom).
    copy: [f32; 4],
    /// x = blur scale, y = 1 / buffer width, z = 1 / buffer height.
    blur: [f32; 4],
}

const SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/bloom.wgsl"));

struct Targets {
    scene: wgpu::TextureView,
    scene_texture: wgpu::Texture,
    background: wgpu::TextureView,
    background_texture: wgpu::Texture,
    glow: wgpu::TextureView,
    buffers: [wgpu::TextureView; 2],
    buffer_size: (u32, u32),
}

pub struct Bloom {
    /// The scene's frame (gamma-space colour, sampled by the copy and add).
    pub scene: wgpu::TextureView,
    pub scene_texture: wgpu::Texture,
    /// A copy of the frame for refracting particles (FA's
    /// `BackgroundTexture`), taken before they draw.
    pub background_texture: wgpu::Texture,
    pub background: wgpu::TextureView,
    /// Each pixel's glow.
    pub glow: wgpu::TextureView,
    /// The half-size glow buffers the blur ping-pongs between.
    buffers: [wgpu::TextureView; 2],
    buffer_size: (u32, u32),
    /// The frame's size and its physical pixels per interface pixel.
    frame_size: (u32, u32),
    scale: f32,
    params: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    copy: wgpu::RenderPipeline,
    blur_h: wgpu::RenderPipeline,
    blur_v: wgpu::RenderPipeline,
    add: wgpu::RenderPipeline,
    /// The add into a gamma-space frame (for SMAA).
    add_gamma: wgpu::RenderPipeline,
    /// copy (scene, glow), blur h (buffer 0), blur v (buffer 1), add (scene, buffer 0).
    groups: [wgpu::BindGroup; 4],
    /// `ren_bloom` (FAF's Bloom Render option): off, the frame is added
    /// with no glow (no copy or blur).
    pub enabled: bool,
    /// `ren_BloomBlurKernelScale` (FAF's Bloom Intensity option / 10): how
    /// far each blur pass reaches.
    pub blur_scale: f32,
    map_bloom: f32,
}

fn texture(device: &wgpu::Device, label: &str, (w, h): (u32, u32), format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        // (The frame is also copied for refracting particles.)
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn view(t: &wgpu::Texture, label: &str) -> wgpu::TextureView {
    super::view(t, label)
}

impl Bloom {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, w: u32, h: u32) -> Self {
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom: layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                tex(1),
                tex(2),
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("bloom: shader"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("bloom: pipeline layout"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = |label: &str, fs: &str, target: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
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
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bloom: parameters"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bloom: sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let (copy, blur_h, blur_v, add) = (
            pipeline("bloom: copy glow", "fs_copy", BUFFER_FORMAT),
            pipeline("bloom: blur horizontal", "fs_blur_h", BUFFER_FORMAT),
            pipeline("bloom: blur vertical", "fs_blur_v", BUFFER_FORMAT),
            pipeline("bloom: add to frame", "fs_add", format),
        );
        let add_gamma = pipeline("bloom: add to frame (gamma, for SMAA)", "fs_add_gamma", FRAME_FORMAT);
        let t = Self::textures(device, w, h, 1.0);
        let groups = Self::bind_groups(device, &layout, &params, &sampler, &t.scene, &t.glow, &t.buffers);
        Self { scene: t.scene, scene_texture: t.scene_texture, background_texture: t.background_texture, background: t.background, glow: t.glow, buffers: t.buffers, buffer_size: t.buffer_size, frame_size: (w, h), scale: 1.0, params, layout, sampler, copy, blur_h, blur_v, add, add_gamma, groups, map_bloom: 0.0, enabled: true, blur_scale: BLUR_SCALE }
    }

    /// Frame, glow and the half-size buffers (moho: `width >> 1`,
    /// `height >> 1`; ours in interface pixels, `scale` physical a piece).
    fn textures(device: &wgpu::Device, w: u32, h: u32, scale: f32) -> Targets {
        let half = |n: u32| ((n as f32 / (2.0 * scale.max(1.0))) as u32).max(1);
        let half = (half(w), half(h));
        let scene_texture = texture(device, "world: colour", (w, h), FRAME_FORMAT);
        let background_texture = texture(device, "world: refraction background", (w, h), FRAME_FORMAT);
        let target = |label, size, format| view(&texture(device, label, size, format), label);
        Targets {
            scene: view(&scene_texture, "world: colour"),
            scene_texture,
            background: view(&background_texture, "world: refraction background"),
            background_texture,
            glow: target("world: glow", (w, h), GLOW_FORMAT),
            buffers: [target("bloom: buffer 0 (half size)", half, BUFFER_FORMAT), target("bloom: buffer 1 (half size)", half, BUFFER_FORMAT)],
            buffer_size: half,
        }
    }

    /// Copies the frame so far into [`Bloom::background`].
    pub fn copy_background(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_texture_to_texture(self.scene_texture.as_image_copy(), self.background_texture.as_image_copy(), self.scene_texture.size());
    }

    fn bind_groups(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, params: &wgpu::Buffer, sampler: &wgpu::Sampler, scene: &wgpu::TextureView, glow: &wgpu::TextureView, buffers: &[wgpu::TextureView; 2]) -> [wgpu::BindGroup; 4] {
        let group = |label: &str, source: &wgpu::TextureView, second: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(source) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(second) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(sampler) },
                ],
            })
        };
        [group("bloom: copy glow", scene, glow), group("bloom: blur horizontal", &buffers[0], glow), group("bloom: blur vertical", &buffers[1], glow), group("bloom: add to frame", scene, &buffers[0])]
    }

    pub fn resize(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        self.frame_size = (w, h);
        let t = Self::textures(device, w, h, self.scale);
        self.groups = Self::bind_groups(device, &self.layout, &self.params, &self.sampler, &t.scene, &t.glow, &t.buffers);
        (self.scene, self.scene_texture, self.background, self.background_texture, self.glow, self.buffers, self.buffer_size) = (t.scene, t.scene_texture, t.background, t.background_texture, t.glow, t.buffers, t.buffer_size);
    }

    /// The display's physical pixels per interface pixel: the buffers are
    /// remade when it changes (a window moved to another screen). Returns
    /// whether they were: views of the old ones (the refraction background
    /// the particles read) must be taken again.
    pub fn set_scale(&mut self, device: &wgpu::Device, scale: f32) -> bool {
        if scale == self.scale {
            return false;
        }
        self.scale = scale;
        let (w, h) = self.frame_size;
        self.resize(device, w, h);
        true
    }

    /// The map's `Bloom` lighting value (0 without a map).
    pub fn set_map_bloom(&mut self, bloom: f32) {
        self.map_bloom = bloom;
    }

    /// Uploads this frame's parameters.
    pub fn prepare(&self, queue: &wgpu::Queue) {
        let (w, h) = self.buffer_size;
        let params = Params { copy: [GLOW_COPY_SCALE, self.map_bloom, 0.0, 0.0], blur: [self.blur_scale, 1.0 / w as f32, 1.0 / h as f32, 0.0] };
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&params));
    }

    /// Copies, blurs and adds the glow, writing the finished frame to `target`:
    /// the sRGB surface, or (`gamma`) a gamma-space frame for SMAA.
    pub fn apply(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, gamma: bool) {
        let _span = rts_core_engine::prof_span!("bloom");
        let pass = |encoder: &mut wgpu::CommandEncoder, label: &str, view: &wgpu::TextureView, pipeline: &wgpu::RenderPipeline, group: &wgpu::BindGroup| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: super::gpu_prof::pass_writes(label),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        };
        if self.enabled {
            encoder.push_debug_group("bloom");
            pass(encoder, "bloom: copy glow", &self.buffers[0], &self.copy, &self.groups[0]);
            for _ in 0..BLUR_COUNT {
                pass(encoder, "bloom: blur horizontal", &self.buffers[1], &self.blur_h, &self.groups[1]);
                pass(encoder, "bloom: blur vertical", &self.buffers[0], &self.blur_v, &self.groups[2]);
            }
            encoder.pop_debug_group();
        } else {
            // No glow to add: the buffer the add reads, cleared.
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom: off (clear)"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.buffers[0],
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: super::gpu_prof::pass_writes("bloom: off (clear)"),
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        pass(encoder, if gamma { "bloom: add to frame (for SMAA)" } else { "bloom: add to frame" }, target, if gamma { &self.add_gamma } else { &self.add }, &self.groups[3]);
    }
}
