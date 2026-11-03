//! FA's sky (moho's `SkyDome`, `effects/sky.fx`), drawn when the camera
//! tilts up to it (FAF's Render Sky option, `ren_Skydome`). The map's sky
//! box (`scmap::SkyBox`) gives the dome, its colours, planets and cirrus;
//! maps without one (v56) get moho's `SkyDome` constructor defaults.
//!
//! The dome (MohoEngine.dll `CreateDomeVertexBuffer` 0x103e12f0): a
//! spherical cap whose base circle has radius `scale` at elevation angle
//! `sub_height` on its sphere (radius `scale / cos(sub_height)`), rows of
//! `subdivisions.0 + 1` vertices from that angle up toward the zenith
//! (`subdivisions.1` rows) and an apex; centred on `position`, raised by
//! `horizon_height`. The atmosphere (`RenderAtmosphere` 0x103e1bc0) blends
//! the horizon colour into the zenith colour from `horizon_height` up to
//! `horizon_height + zenith_height`, shaped by `horizonLookup.dds`.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::engine::assets::dds;
use crate::engine::assets::scmap::SkyBox;

const SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sky.wgsl"));

/// The atmosphere's shape (moho loads it from here).
pub const HORIZON_LOOKUP: &str = "/textures/environment/horizonlookup.dds";
/// The cirrus texture when the map names none.
pub const DEFAULT_CIRRUS: &str = "/textures/environment/cirrus000.dds";

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DomeVertex {
    position: [f32; 3],
    /// Around the dome (radians), for the lookup's horizontal row.
    theta: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Planet {
    /// Position, rotation.
    position: [f32; 4],
    size: [f32; 2],
    region: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    horizon: [f32; 4],
    horizon_color: [f32; 4],
    zenith_color: [f32; 4],
    cirrus_color: [f32; 4],
    cirrus: [[f32; 4]; 8],
}

/// The sky's textures, decoded off the render thread.
pub struct SkyTextures {
    pub lookup: Option<dds::Texture>,
    pub cirrus: Option<dds::Texture>,
    pub albedo: Option<dds::Texture>,
    pub glow: Option<dds::Texture>,
}

impl SkyTextures {
    /// Reads `sky`'s textures through `file` (a VFS read and decode).
    pub fn read(sky: &SkyBox, file: impl Fn(&str) -> Option<dds::Texture>) -> Self {
        let cirrus = if sky.cirrus_texture.is_empty() { DEFAULT_CIRRUS } else { sky.cirrus_texture.as_str() };
        Self { lookup: file(HORIZON_LOOKUP), cirrus: file(&cirrus.to_lowercase()), albedo: file(&sky.albedo.to_lowercase()), glow: file(&sky.glow.to_lowercase()) }
    }
}

/// Moho's sky for a map without one (`SkyDome` constructor, 0x103df120),
/// centred on the map (where moho puts it isn't traced).
pub fn default_sky(map_size: (f32, f32)) -> SkyBox {
    SkyBox {
        position: [map_size.0 / 2.0, 0.0, map_size.1 / 2.0],
        horizon_height: 0.0,
        scale: 512.0,
        sub_height: 1.256_637,
        subdivisions: (16, 6),
        zenith_height: 44.0,
        horizon_color: [0.5608, 0.6706, 0.8857],
        zenith_color: [0.1804, 0.4039, 0.7245],
        decal_glow: 0.1,
        cirrus_multiplier: 1.8,
        cirrus_color: [1.0, 1.0, 1.0],
        cirrus_texture: DEFAULT_CIRRUS.into(),
        ..Default::default()
    }
}

/// The dome's vertices (rows from the base up, then the apex) and its
/// triangles, counter-clockwise seen from outside (the pipelines cull
/// those, so it's seen only from inside).
fn dome(sky: &SkyBox) -> (Vec<DomeVertex>, Vec<u32>) {
    let (around, rows) = (sky.subdivisions.0.max(3) as u32, sky.subdivisions.1.max(1) as u32);
    let base = sky.sub_height;
    let radius = sky.scale / base.cos();
    let rise = radius * base.sin();
    let step = (std::f32::consts::FRAC_PI_2 - base) / rows as f32;
    let [px, py, pz] = sky.position;
    let mut vertices = Vec::with_capacity(((around + 1) * rows + 1) as usize);
    for i in 0..rows {
        let phi = base + i as f32 * step;
        let (ring, y) = (radius * phi.cos(), radius * phi.sin() - rise);
        for j in 0..=around {
            let theta = j as f32 / around as f32 * std::f32::consts::TAU;
            vertices.push(DomeVertex { position: [px + theta.cos() * ring, py + y + sky.horizon_height, pz + theta.sin() * ring], theta });
        }
    }
    let apex = vertices.len() as u32;
    vertices.push(DomeVertex { position: [px, py + radius - rise + sky.horizon_height, pz], theta: 0.0 });
    let at = |i: u32, j: u32| i * (around + 1) + j;
    let mut indices = Vec::with_capacity((rows * around * 6) as usize);
    for i in 0..rows {
        for j in 0..around {
            let (a, b) = (at(i, j), at(i, j + 1));
            if i + 1 < rows {
                let (c, d) = (at(i + 1, j), at(i + 1, j + 1));
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            } else {
                indices.extend_from_slice(&[a, apex, b]);
            }
        }
    }
    (vertices, indices)
}

/// The sky's pipelines and samplers.
pub struct SkyRenderer {
    layout: wgpu::BindGroupLayout,
    atmosphere: wgpu::RenderPipeline,
    cirrus: wgpu::RenderPipeline,
    planet: wgpu::RenderPipeline,
    planet_glow: wgpu::RenderPipeline,
    samplers: [wgpu::Sampler; 3],
    /// Stand-in for a missing texture (transparent black).
    missing: wgpu::TextureView,
}

/// One map's sky, ready to draw.
pub struct Sky {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    planets: wgpu::Buffer,
    planet_count: u32,
    group: wgpu::BindGroup,
}

impl SkyRenderer {
    pub fn new(device: &wgpu::Device, globals: &wgpu::BindGroupLayout, format: wgpu::TextureFormat, depth: wgpu::TextureFormat) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let sampler = |binding| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky: layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                texture(1),
                texture(2),
                texture(3),
                texture(4),
                sampler(5),
                sampler(6),
                sampler(7),
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("sky: shader"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("sky: pipeline layout"), bind_group_layouts: &[Some(globals), Some(&layout)], immediate_size: 0 });
        let dome_layout = wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<DomeVertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32] };
        let planet_layout = wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Planet>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x2, 2 => Float32x4] };
        use super::globals::{scene_targets, GlowWrite, GLOW_FORMAT};
        let blend = Some(wgpu::BlendState::ALPHA_BLENDING);
        // The planets' glow pass writes only the glow (`Write_A`).
        let glow_only = [Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::empty() }), Some(wgpu::ColorTargetState { format: GLOW_FORMAT, blend: None, write_mask: wgpu::ColorWrites::RED })];
        let pipeline = |label: &str, vs: &str, fs: &str, buffers: &[wgpu::VertexBufferLayout], targets: &[Option<wgpu::ColorTargetState>], cull: Option<wgpu::Face>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some(vs), buffers, compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some(fs), targets, compilation_options: Default::default() }),
                primitive: wgpu::PrimitiveState { cull_mode: cull, ..Default::default() },
                // `ZEnable = false`.
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: depth,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        // Seen from inside only (moho's `CullMode = CW`): `dome` winds its
        // triangles counter-clockwise from outside.
        let dome_cull = Some(wgpu::Face::Front);
        let atmosphere = pipeline("sky: atmosphere", "vs_dome", "fs_atmosphere", std::slice::from_ref(&dome_layout), &scene_targets(format, None, GlowWrite::Keep), dome_cull);
        let cirrus = pipeline("sky: cirrus", "vs_dome", "fs_cirrus", std::slice::from_ref(&dome_layout), &scene_targets(format, blend, GlowWrite::Keep), dome_cull);
        let planet = pipeline("sky: planets", "vs_planet", "fs_planet", std::slice::from_ref(&planet_layout), &scene_targets(format, blend, GlowWrite::Keep), None);
        let planet_glow = pipeline("sky: planet glow", "vs_planet", "fs_planet_glow", std::slice::from_ref(&planet_layout), &glow_only, None);
        let make_sampler = |label, filter, address| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                address_mode_u: address,
                address_mode_v: address,
                mag_filter: filter,
                min_filter: filter,
                mipmap_filter: if filter == wgpu::FilterMode::Linear { wgpu::MipmapFilterMode::Linear } else { wgpu::MipmapFilterMode::Nearest },
                ..Default::default()
            })
        };
        let samplers = [
            make_sampler("sky: lookup", wgpu::FilterMode::Nearest, wgpu::AddressMode::ClampToEdge),
            make_sampler("sky: clamp", wgpu::FilterMode::Linear, wgpu::AddressMode::ClampToEdge),
            make_sampler("sky: wrap", wgpu::FilterMode::Linear, wgpu::AddressMode::Repeat),
        ];
        // (wgpu zero-fills new textures.)
        let missing = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("sky: missing texture"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor { label: Some("sky: missing texture"), ..Default::default() });
        Self { layout, atmosphere, cirrus, planet, planet_glow, samplers, missing }
    }

    /// Uploads `sky` with its textures.
    pub fn sky(&self, device: &wgpu::Device, queue: &wgpu::Queue, sky: &SkyBox, textures: &SkyTextures) -> Sky {
        let (vertices, indices) = dome(sky);
        let planets: Vec<Planet> = sky.planets.iter().map(|p| Planet { position: [p.position[0], p.position[1], p.position[2], p.rotation], size: p.scale, region: p.uv }).collect();
        let up = |t: &Option<dds::Texture>, srgb: bool, label: &str| t.as_ref().and_then(|t| super::models::upload_texture(device, queue, t, srgb, label));
        // (Colours in gamma space, as the frame: no sRGB decode.)
        let views = [up(&textures.lookup, false, "sky: lookup"), up(&textures.cirrus, false, "sky: cirrus"), up(&textures.albedo, false, "sky: albedo"), up(&textures.glow, false, "sky: glow")];
        let mut layers = [[0.0f32; 4]; 8];
        for (i, l) in sky.cirrus_layers.iter().take(4).enumerate() {
            layers[2 * i] = [l.frequency[0], l.frequency[1], l.speed, 0.0];
            layers[2 * i + 1] = [l.direction[0], l.direction[1], 0.0, 0.0];
        }
        let rgb = |c: [f32; 3]| [c[0], c[1], c[2], 1.0];
        let params = Params {
            horizon: [sky.horizon_height, sky.horizon_height + sky.zenith_height, sky.decal_glow, sky.cirrus_multiplier],
            horizon_color: rgb(sky.horizon_color),
            zenith_color: rgb(sky.zenith_color),
            cirrus_color: rgb(sky.cirrus_color),
            cirrus: layers,
        };
        let buffer = |label, contents: &[u8], usage| device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents, usage });
        let params = buffer("sky: params", bytemuck::bytes_of(&params), wgpu::BufferUsages::UNIFORM);
        let view = |i: usize| wgpu::BindingResource::TextureView(views[i].as_ref().unwrap_or(&self.missing));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky: textures"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: view(0) },
                wgpu::BindGroupEntry { binding: 2, resource: view(1) },
                wgpu::BindGroupEntry { binding: 3, resource: view(2) },
                wgpu::BindGroupEntry { binding: 4, resource: view(3) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(&self.samplers[0]) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::Sampler(&self.samplers[1]) },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::Sampler(&self.samplers[2]) },
            ],
        });
        let planet_bytes: &[u8] = if planets.is_empty() { &[0; std::mem::size_of::<Planet>()] } else { bytemuck::cast_slice(&planets) };
        tracing::info!(vertices = vertices.len(), planets = planets.len(), lookup = views[0].is_some(), cirrus = views[1].is_some(), "sky: dome");
        Sky {
            vertices: buffer("sky: dome", bytemuck::cast_slice(&vertices), wgpu::BufferUsages::VERTEX),
            indices: buffer("sky: dome", bytemuck::cast_slice(&indices), wgpu::BufferUsages::INDEX),
            index_count: indices.len() as u32,
            planets: buffer("sky: planets", planet_bytes, wgpu::BufferUsages::VERTEX),
            planet_count: planets.len() as u32,
            group,
        }
    }

    /// Draws `sky` (group 0, the scene's globals, already bound): the
    /// atmosphere, the planets (albedo, then glow), then the cirrus.
    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, sky: &'a Sky) {
        pass.set_bind_group(1, &sky.group, &[]);
        pass.set_vertex_buffer(0, sky.vertices.slice(..));
        pass.set_index_buffer(sky.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.set_pipeline(&self.atmosphere);
        pass.draw_indexed(0..sky.index_count, 0, 0..1);
        if sky.planet_count > 0 {
            pass.set_vertex_buffer(0, sky.planets.slice(..));
            pass.set_pipeline(&self.planet);
            pass.draw(0..6, 0..sky.planet_count);
            pass.set_pipeline(&self.planet_glow);
            pass.draw(0..6, 0..sky.planet_count);
            pass.set_vertex_buffer(0, sky.vertices.slice(..));
        }
        pass.set_pipeline(&self.cirrus);
        pass.draw_indexed(0..sky.index_count, 0, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Moho's dome: rows from the base angle up (the base circle at the
    /// sky's scale around its position, at the horizon height), an apex
    /// over the centre, every triangle indexing a vertex.
    #[test]
    fn dome_matches_moho_geometry() {
        let sky = default_sky((1024.0, 1024.0));
        let (v, i) = dome(&sky);
        assert_eq!(v.len(), 17 * 6 + 1);
        let base = v[0].position;
        assert!((base[0] - (512.0 + 512.0)).abs() < 1e-3 && base[1].abs() < 1e-3 && (base[2] - 512.0).abs() < 1e-3, "{base:?}");
        let apex = v.last().unwrap().position;
        let radius = 512.0 / sky.sub_height.cos();
        assert!((apex[1] - radius * (1.0 - sky.sub_height.sin())).abs() < 1e-2, "{apex:?}");
        assert_eq!(i.len(), 16 * 5 * 6 + 16 * 3);
        assert!(i.iter().all(|&k| (k as usize) < v.len()));
    }
}
