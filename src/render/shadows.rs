//! Sun shadows: one cascaded shadow map every model casts into and every
//! receiver (terrain, decals, models, water) samples through the shared
//! `sun_shadow` helper (`globals::PRELUDE`).
//!
//! Cascades split the camera's view depth (out to where it sees the ground)
//! and each is an orthographic sun view fitted around its slice, squared and
//! snapped to whole texels so panning doesn't make shadow edges crawl. The
//! terrain doesn't cast into it: its own shadow is a horizon map baked from
//! the heightfield at map load (`terrain::horizon_map`), since FA's sun
//! never moves.

use glam::{Mat4, Vec3, Vec4Swizzles};

use super::camera::Camera;
use super::globals::CASCADES;

pub const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Texels per cascade side until `ren_ShadowSize` (FAF's Shadow
/// Resolution option, which a profile sets at start) says otherwise.
pub const SIZE: u32 = 2048;

pub struct Shadows {
    /// Texels per cascade side.
    pub size: u32,
    /// All cascades (sampled by receivers).
    pub array_view: wgpu::TextureView,
    /// One per cascade (render targets).
    layer_views: Vec<wgpu::TextureView>,
    pub compare_sampler: wgpu::Sampler,
    /// Group 0 of the caster pipelines: a cascade's light matrix.
    light_layout: wgpu::BindGroupLayout,
    pub opaque: wgpu::RenderPipeline,
    pub cutout: wgpu::RenderPipeline,
}

const SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/shadows.wgsl"));

impl Shadows {
    pub fn new(device: &wgpu::Device, model_textures: &wgpu::BindGroupLayout, palette: &wgpu::BindGroupLayout, vertex_layouts: &[wgpu::VertexBufferLayout]) -> Self {
        let (array_view, layer_views) = maps(device, SIZE);
        let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadows: light layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("shadows: shader"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("shadows: pipeline layout"), bind_group_layouts: &[Some(&light_layout), Some(model_textures), Some(palette)], immediate_size: 0 });
        let pipeline = |label: &str, fs: Option<&str>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_shadow"), buffers: vertex_layouts, compilation_options: Default::default() },
                fragment: fs.map(|f| wgpu::FragmentState { module: &shader, entry_point: Some(f), targets: &[], compilation_options: Default::default() }),
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: SHADOW_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 },
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            size: SIZE,
            array_view,
            layer_views,
            compare_sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("shadows: compare sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                compare: Some(wgpu::CompareFunction::LessEqual),
                ..Default::default()
            }),
            light_layout,
            opaque: pipeline("shadows: opaque casters", None),
            cutout: pipeline("shadows: cutout casters", Some("fs_cutout")),
        }
    }

    /// `ren_ShadowSize`: remakes the maps at `size` texels a side (clamped
    /// to 256 and the device's limit). Whether it changed: the caller
    /// rebinds group 0, which samples them.
    pub fn set_size(&mut self, device: &wgpu::Device, size: u32) -> bool {
        let size = size.clamp(256, device.limits().max_texture_dimension_2d);
        if size == self.size {
            return false;
        }
        (self.array_view, self.layer_views) = maps(device, size);
        self.size = size;
        true
    }

    /// A world view's cascade light matrices (each view fits its own
    /// cascades; the maps are shared, the views drawing in turn).
    pub fn lights(&self, device: &wgpu::Device) -> Lights {
        Lights(
            (0..CASCADES)
                .map(|c| {
                    let b = device.create_buffer(&wgpu::BufferDescriptor { label: Some(&format!("shadows: cascade {c} light")), size: std::mem::size_of::<LightUniform>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
                    let g = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some(&format!("shadows: cascade {c} light")), layout: &self.light_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: b.as_entire_binding() }] });
                    (b, g)
                })
                .collect(),
        )
    }

    /// Starts cascade `i`'s depth pass (cleared) with its light (one of
    /// `lights`) bound.
    pub fn begin<'e>(&'e self, encoder: &'e mut wgpu::CommandEncoder, i: usize, lights: &'e Lights) -> wgpu::RenderPass<'e> {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(CASCADE_PASSES[i]),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.layer_views[i],
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: super::gpu_prof::pass_writes(CASCADE_PASSES[i]),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &lights.0[i].1, &[]);
        pass
    }
}

/// Per cascade: its light matrix uniform and bind group ([`Shadows::lights`]).
pub struct Lights(Vec<(wgpu::Buffer, wgpu::BindGroup)>);

/// A cascade's uniform (`shadows.wesl` `Light`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LightUniform {
    view_proj: [f32; 16],
    /// x = moho's shader `time` (game ticks the frame shows): casters that
    /// sway (`UndulatingDepthVS`) sway in their shadows too.
    params: [f32; 4],
}

impl Lights {
    pub fn write(&self, queue: &wgpu::Queue, cascades: &Cascades, game_ticks: f32) {
        for (i, (b, _)) in self.0.iter().enumerate() {
            let uniform = LightUniform { view_proj: cascades.view_proj[i].to_cols_array(), params: [game_ticks, 0.0, 0.0, 0.0] };
            queue.write_buffer(b, 0, bytemuck::bytes_of(&uniform));
        }
    }
}

/// Each cascade's pass, nearest first.
const CASCADE_PASSES: [&str; CASCADES] = ["shadows: cascade 0 (near)", "shadows: cascade 1", "shadows: cascade 2 (far)"];

/// This frame's cascades.
#[derive(Clone, Copy, Debug)]
pub struct Cascades {
    /// Cascades in use (the first `count`; Shadow Fidelity Low uses fewer).
    pub count: usize,
    pub view_proj: [Mat4; CASCADES],
    /// View depth where each cascade ends.
    pub ends: [f32; CASCADES],
    /// Sun view per cascade and the square it covers there (centre x, y,
    /// half size): casters outside a cascade's column skip it.
    pub view: [Mat4; CASCADES],
    pub area: [[f32; 3]; CASCADES],
}

impl Cascades {
    /// Whether a caster at `p` (reaching `radius` around it) can shade
    /// anything cascade `c` covers.
    pub fn casts_into(&self, c: usize, p: Vec3, radius: f32) -> bool {
        let l = self.view[c].transform_point3(p);
        let [cx, cy, half] = self.area[c];
        (l.x - cx).abs() <= half + radius && (l.y - cy).abs() <= half + radius
    }
}

/// The cascades' depth maps, `size` texels a side: the array receivers
/// sample, and a view per layer to render into.
fn maps(device: &wgpu::Device, size: u32) -> (wgpu::TextureView, Vec<wgpu::TextureView>) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadows: cascade maps"),
        size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: CASCADES as u32 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SHADOW_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let array_view = tex.create_view(&wgpu::TextureViewDescriptor { label: Some("shadows: cascade maps"), dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
    let layer_views = (0..CASCADES as u32)
        .map(|l| tex.create_view(&wgpu::TextureViewDescriptor { label: Some(CASCADE_PASSES[l as usize]), dimension: Some(wgpu::TextureViewDimension::D2), base_array_layer: l, array_layer_count: Some(1), ..Default::default() }))
        .collect();
    (array_view, layer_views)
}

/// Fits `CASCADES` sun views around the part of the camera's view that can
/// hold ground: from where it first meets the highest terrain to where it
/// last meets the lowest (`ground` = (lowest, highest) terrain height).
/// `sun` points toward the sun; `size`: texels per cascade side; `reach`:
/// the view depth past which nothing is shadowed (`ren_ShadowLOD`); `count`:
/// cascades to split it into (1..=`CASCADES`), the rest unused.
pub fn fit(camera: &Camera, sun: Vec3, ground: (f32, f32), size: u32, reach: f32, count: usize) -> Cascades {
    let count = count.clamp(1, CASCADES);
    let inv = camera.view_proj().inverse();
    let eye = camera.eye();
    let forward = (camera.target - eye).normalize();
    // Corner rays of the screen.
    let rays: Vec<Vec3> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .map(|&(x, y)| {
            let near = inv.project_point3(Vec3::new(x, y, 0.0));
            let far = inv.project_point3(Vec3::new(x, y, 1.0));
            (far - near).normalize()
        })
        .collect();
    // View depth where a corner ray meets height `y` (capped: a ray over the
    // horizon never does).
    let cap = camera.eye_distance() * 4.0;
    let depth_at = |r: &Vec3, y: f32| if r.y < -1e-3 { (((y - eye.y) / r.y).max(0.0)).min(cap / r.dot(forward)) * r.dot(forward) } else { cap };
    let far = rays.iter().map(|r| depth_at(r, ground.0)).fold(0.0f32, f32::max).max(camera.eye_distance() * 1.2).min(reach.max(1.0));
    let near = rays.iter().map(|r| depth_at(r, ground.1 + 10.0)).fold(f32::MAX, f32::min).clamp(0.5, far * 0.5);
    // Practical split: mostly logarithmic, partly uniform.
    let split = |i: usize| {
        let f = i as f32 / count as f32;
        0.75 * near * (far / near).powf(f) + 0.25 * (near + (far - near) * f)
    };
    let sun = sun.normalize_or(Vec3::Y);
    let up = if sun.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let mut out = Cascades { count, view_proj: [Mat4::IDENTITY; CASCADES], ends: [far; CASCADES], view: [Mat4::IDENTITY; CASCADES], area: [[0.0; 3]; CASCADES] };
    for c in 0..count {
        let (d0, d1) = (split(c), split(c + 1));
        let corners: Vec<Vec3> = rays.iter().flat_map(|r| [eye + *r * (d0 / r.dot(forward)), eye + *r * (d1 / r.dot(forward))]).collect();
        let center = corners.iter().copied().sum::<Vec3>() / corners.len() as f32;
        // A bounding sphere keeps the cascade's size fixed as the view turns.
        let radius = corners.iter().map(|p| p.distance(center)).fold(0.0f32, f32::max).max(1.0);
        let view = glam::camera::rh::view::look_at_mat4(center + sun * (radius + 400.0), center, up);
        // Snap the centre to whole texels.
        let texel = 2.0 * radius / size as f32;
        let c_ls = (view * center.extend(1.0)).xyz();
        let snapped = Vec3::new((c_ls.x / texel).floor() * texel, (c_ls.y / texel).floor() * texel, c_ls.z);
        let (cx, cy) = (snapped.x, snapped.y);
        let proj = glam::camera::rh::proj::directx::orthographic(cx - radius, cx + radius, cy - radius, cy + radius, 1.0, 2.0 * radius + 800.0);
        out.view_proj[c] = proj * view;
        out.ends[c] = d1;
        out.view[c] = view;
        out.area[c] = [cx, cy, radius];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cascades grow with depth, and the camera's target (on the ground)
    /// projects inside the cascade covering its view depth.
    #[test]
    fn cascades_cover_the_view() {
        let mut cam = Camera::new(512.0, 512.0);
        cam.zoom = 60.0;
        let c = fit(&cam, Vec3::new(0.6, 0.6, 0.5), (0.0, 0.0), SIZE, f32::INFINITY, CASCADES);
        assert!(c.ends[0] < c.ends[1] && c.ends[1] < c.ends[2], "{:?}", c.ends);
        let depth = (cam.target - cam.eye()).length();
        let i = c.ends.iter().position(|&e| depth <= e).expect("target within the cascades");
        let p = c.view_proj[i] * cam.target.extend(1.0);
        assert!(p.x.abs() <= 1.0 && p.y.abs() <= 1.0 && (0.0..=1.0).contains(&p.z), "cascade {i}: {p}");
    }
}
