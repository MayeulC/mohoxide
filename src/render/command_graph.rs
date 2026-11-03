//! The command graph as moho draws it: the selected units' queued orders as
//! order lines (unit to its first waypoint, then waypoint to waypoint) and a
//! waypoint icon per order. FAF's params also name arrowhead textures, but
//! the base game draws no arrowheads on these lines, so neither do we.
//! Moho's engine draws these; FAF's Lua only describes them
//! (`/lua/ui/game/commandgraphparams.lua` `CommandGraphParams`: per
//! `UNITCOMMAND_*` the waypoint and line textures, the line's UV
//! aspect and scroll rate, normal / selected / highlight colours and glow,
//! with `inherit_from` chains and a `default`). Orders whose entry names no
//! waypoint texture (stop, upgrade, factory builds, ...) aren't drawn, as
//! that file says.
//!
//! Patrol waypoints form a loop: a line runs from the last back to the
//! first, as the sim cycles them.
//!
//! A mobile build's site gets its structure's build skirt outlined, a few
//! pixels wide: green while a selected unit is on it, blue while queued.
//!
//! Everything lies on the ground like a splat (terrain-following quads),
//! drawn over the models as the base game does (a build line crosses the
//! factory it builds), coloured `texture * colour` and
//! glowing where the texture shows (`primbatcher.fx` `TCommand` /
//! `TCommandGlow`). Units sharing an order share its waypoint; units heading
//! for the same first waypoint from close together share one line to it,
//! from their centre (a straggler gets its own). A line's
//! width is `sqrt(units on it)` world units capped at 10 (FAF's
//! `CalculateWaypointLineWidth`; the base game's lines match it beside a
//! factory), never under a few pixels; waypoints stay between
//! `ui_MinWaypointSize` and `ui_MaxWaypointSize` pixels (7 and 100,
//! `commandwaypoint.lua`). The line texture repeats every `width /
//! orderline_uv_aspect_ratio` along the line and scrolls toward the waypoint
//! at `orderline_anim_rate` repeats a second.
//!
//! Inferred, not verified against the binary: waypoint icons' world size
//! (before the pixel clamp), the lines' pixel minimum, everything flat on the ground facing north, lines
//! to a formation order's shared waypoint rather than each unit's slot, and
//! the first lines' grouping ([`MERGE_DISTANCE`], [`MERGE_PX`]).

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, RwLock};

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use crate::engine::sim::command::CommandKind;
use crate::engine::snapshot::{EntityView, OrderMarker};
use crate::engine::vfs::Vfs;

/// FAF's description of the command graph.
const PARAMS: &str = "/lua/ui/game/commandgraphparams.lua";
/// `ui_MinWaypointSize` / `ui_MaxWaypointSize` (pixels).
const MIN_WAYPOINT_PX: f32 = 7.0;
const MAX_WAYPOINT_PX: f32 = 100.0;
/// A waypoint icon's size on the ground before the pixel clamp (the base
/// game's move icon is about 1.2 units across beside a factory).
const WAYPOINT_SIZE: f32 = 1.25;
/// `ui_WaypointLineScale`.
const LINE_SCALE: f32 = 1.0;
/// Lines no thinner than this many pixels when zoomed out (the texture's
/// band is about half the quad).
const MIN_LINE_PX: f32 = 6.0;
/// `ui_CurveSegments`: pieces a waypoint's turn is cut into, at most. A
/// turn is cut into one piece per [`CURVE_PX_PER_SEGMENT`] of its radius on
/// screen, up to this: past that the pieces are under a pixel apart, and a
/// spread attack's thousands of turns would be most of the frame.
const CURVE_SEGMENTS: usize = 32;
const CURVE_PX_PER_SEGMENT: f32 = 4.0;
/// A waypoint icon smaller than this (pixels) is one flat quad at its
/// ground height rather than an [`ICON_GRID`] following the ground: the
/// command graph draws over everything, so the ground's shape under so small
/// an icon moves it by under a pixel.
const ICON_FLAT_PX: f32 = 32.0;
/// How far past the view's edge (pixels) a line or waypoint still counts as
/// on it: the turns reach [`CURVE_SMOOTHNESS`] past their waypoints.
const OFF_SCREEN_MARGIN: f32 = 2.0 * CURVE_SMOOTHNESS;
/// `ui_CurveSmoothness` (35), read as pixels: a line turns this far either
/// side of a waypoint (straight elsewhere, as in the base game).
const CURVE_SMOOTHNESS: f32 = 35.0;
/// Build sites' squares (the structure's skirt): pixels wide, and blue for a
/// queued build, green for one a selected unit is on now (sampled from the
/// base game).
const SITE_PX: f32 = 2.0;
const SITE_QUEUED: [f32; 4] = [0.2, 0.52, 0.85, 1.0];
const SITE_ACTIVE: [f32; 4] = [0.25, 0.78, 0.25, 1.0];
/// Standing structures' skirts while placing another: green, a little
/// see-through so the ghost's own (blue) reads over them.
const SITE_NEARBY: [f32; 4] = [0.3, 0.9, 0.35, 0.75];
/// Draws in the plain colour (a white texel).
const SOLID: &str = "";
/// Above the ground, so the terrain between height samples doesn't cover it.
const LIFT: f32 = 0.25;
/// Ground-following pieces: a line is cut every this many world units (at
/// most [`MAX_PIECES`] pieces); a waypoint is a [`ICON_GRID`]-square grid.
const PIECE: f32 = 2.0;
const MAX_PIECES: usize = 64;
/// Line pieces a frame, shared out over its lines (big selections get
/// coarser lines, at least [`MIN_PIECES`] each).
const PIECE_BUDGET: usize = 50_000;
const MIN_PIECES: usize = 4;
const ICON_GRID: usize = 4;
/// Units on the same order whose first line would run side by side share one
/// line, from their centre (a hundred tanks drew a hundred lines):
/// units this many world units apart (or [`MERGE_PX`] pixels, zoomed out)
/// join a group, and a chain of them is one group (a formation's column).
const MERGE_DISTANCE: f32 = 6.0;
const MERGE_PX: f32 = 24.0;

/// A waypoint as drawn: its order kind, position, ETA (seconds) and size on
/// the ground.
#[derive(Clone, Copy, Debug)]
pub struct Waypoint {
    #[allow(dead_code)]
    pub kind: u8,
    pub position: Vec3,
    pub eta: f32,
    pub size: f32,
}

/// A selected unit's bracket: its half extents across and along it
/// ([`bracket_half`]), `SelectionThickness` (0: the default) and
/// `SelectionCenterOffsetX/Y/Z` (in its frame).
#[derive(Clone, Copy, Debug)]
pub struct Bracket {
    pub position: Vec3,
    pub rotation: glam::Quat,
    pub half: [f32; 2],
    pub thickness: f32,
    pub offset: [f32; 3],
}

/// The player's selection brackets (`selection_brackets_player`; moho's
/// `ren_SelectColor` white), as moho draws them (MohoEngine.dll 0x103c72f0):
/// a quarter of the texture at each corner of the unit's half extents, a
/// square of half size thickness x the larger extent centred on the
/// corner; all of it turned with the unit, `ren_SelectionHeightFudge` over
/// it, depth-tested so the unit stands over it. Constants from FAF's
/// `/lua/RenderSelectParams.lua`.
const BRACKET_TEXTURE: &str = "/textures/ui/common/game/selection/selection_brackets_player.dds";
const BRACKET_LIFT: f32 = 0.12;
/// `ren_UnitSelectionScale`: `SelectionSizeX/Z` x this are the half extents.
const UNIT_SELECTION_SCALE: f32 = 0.75;
/// `ren_SelectionSizeFudge`: without a selection size, the unit's box x this
/// (moho's is the mesh's bounding box; ours the collision box).
const SELECTION_SIZE_FUDGE: f32 = 1.85;
/// `ren_SelectBracketSize`: the thickness when a blueprint has none.
const BRACKET_SIZE: f32 = 0.2;
/// `ren_SelectBracketMinPixelSize`: a corner piece's half size is at least
/// this many pixels.
const MIN_BRACKET_PX: f32 = 3.0;

/// A bracket's half extents across and along the unit: `SelectionSizeX/Z`
/// x `ren_UnitSelectionScale` where set (above 0), else the unit's box x
/// `ren_SelectionSizeFudge`, per axis.
pub fn bracket_half(selection: [f32; 2], unit_half: [f32; 2]) -> [f32; 2] {
    std::array::from_fn(|i| if selection[i] > 0.0 { selection[i] * UNIT_SELECTION_SCALE } else { unit_half[i] * SELECTION_SIZE_FUDGE })
}

/// A bracket's four corner pieces (6 vertices each): the texture's quarter
/// toward each corner, on a square of half size `piece` centred on that
/// corner of `half` around `centre`, turned by `rotation`. The GPU builds
/// them from a [`BracketInstance`] (`command_graph.wesl` `vs_bracket`, the
/// same corners in the same order); this is its reference.
#[cfg(test)]
fn bracket_pieces(centre: Vec3, rotation: glam::Quat, half: [f32; 2], piece: f32) -> [Vertex; 24] {
    let mut out = [Vertex { pos: [0.0; 3], uv: [0.0; 2], color: [1.0; 4], glow: 0.0 }; 24];
    let at = |x: f32, z: f32| {
        let c = centre + rotation * Vec3::new(x, 0.0, z);
        [c.x, c.y, c.z]
    };
    for (k, (sx, sz)) in [(-1.0f32, 1.0f32), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)].into_iter().enumerate() {
        // The texture's left half to the left (-x), its top half to the front (+z).
        let (u0, v0) = (if sx < 0.0 { 0.0 } else { 0.5 }, if sz > 0.0 { 0.0 } else { 0.5 });
        let (cx, cz) = (sx * half[0], sz * half[1]);
        let corner = |u: f32, v: f32| Vertex { pos: at(cx + (u - 0.5) * 2.0 * piece, cz + (0.5 - v) * 2.0 * piece), uv: [u0 + 0.5 * u, v0 + 0.5 * v], color: [1.0; 4], glow: 0.0 };
        let (a, b, c, d) = (corner(0.0, 0.0), corner(1.0, 0.0), corner(0.0, 1.0), corner(1.0, 1.0));
        out[k * 6..k * 6 + 6].copy_from_slice(&[a, b, c, c, b, d]);
    }
    out
}
/// A selection bracket as the GPU draws it (`vs_bracket`: 24 vertices,
/// [`bracket_pieces`]' corners): a big selection is thousands of these, so
/// they go up as one small record each instead of their vertices.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct BracketInstance {
    centre: [f32; 3],
    piece: f32,
    /// x, y, z, w.
    rotation: [f32; 4],
    half: [f32; 2],
    _pad: [f32; 2],
}

/// Vertices per bracket (four corner pieces of two triangles).
const BRACKET_VERTICES: u32 = 24;

/// One order type's look, `inherit_from` and `default` resolved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub waypoint_texture: String,
    pub line_texture: String,
    pub uv_aspect: f32,
    pub anim_rate: f32,
    pub line_selected: [f32; 4],
    pub line_highlight: [f32; 4],
    pub glow_selected: f32,
    pub glow_highlight: f32,
    pub waypoint_selected: [f32; 4],
    pub waypoint_highlight: [f32; 4],
}

/// Groups units heading for the same first waypoint: `units` is (unit, its
/// first drawn command, position); units with one command within `link` of
/// each other, directly or through others, share a group. Returns each unit's
/// group (its lowest unit id) and each group's centre.
fn merge_starts(units: &[(u32, u32, Vec3)], link: &dyn Fn(Vec3) -> f32) -> (BTreeMap<u32, u32>, BTreeMap<u32, Vec3>) {
    let mut by_command: BTreeMap<u32, Vec<(u32, Vec3)>> = BTreeMap::new();
    for &(unit, cmd, p) in units {
        by_command.entry(cmd).or_default().push((unit, p));
    }
    let (mut leader, mut centre) = (BTreeMap::new(), BTreeMap::new());
    for (_, mut group) in by_command {
        group.sort_by_key(|g| g.0);
        let mean = group.iter().fold(Vec3::ZERO, |a, g| a + g.1) / group.len() as f32;
        let link = link(mean).max(1e-3);
        // Union-find over a grid of `link` cells: only neighbouring cells'
        // units can be close enough.
        let mut parent: Vec<usize> = (0..group.len()).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        let cell = |p: Vec3| ((p.x / link).floor() as i32, (p.z / link).floor() as i32);
        let mut grid: BTreeMap<(i32, i32), Vec<usize>> = BTreeMap::new();
        for (i, &(_, p)) in group.iter().enumerate() {
            let (cx, cz) = cell(p);
            for dz in -1..=1 {
                for dx in -1..=1 {
                    for &j in grid.get(&(cx + dx, cz + dz)).into_iter().flatten() {
                        let q = group[j].1;
                        if (p.x - q.x).hypot(p.z - q.z) <= link {
                            let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                            // The lower index (lower unit id) leads.
                            parent[a.max(b)] = a.min(b);
                        }
                    }
                }
            }
            grid.entry((cx, cz)).or_default().push(i);
        }
        let mut sums: BTreeMap<usize, (Vec3, u32)> = BTreeMap::new();
        for i in 0..group.len() {
            let r = root(&mut parent, i);
            leader.insert(group[i].0, group[r].0);
            let s = sums.entry(r).or_insert((Vec3::ZERO, 0));
            s.0 += group[i].1;
            s.1 += 1;
        }
        for (r, (sum, n)) in sums {
            centre.insert(group[r].0, sum / n as f32);
        }
    }
    (leader, centre)
}

/// `'AARRGGBB'` as RGBA (the frame is gamma space, as FA's was).
fn color(hex: &str) -> [f32; 4] {
    let v = u32::from_str_radix(hex.trim(), 16).unwrap_or(0xffff_ffff);
    let c = |shift: u32| ((v >> shift) & 0xff) as f32 / 255.0;
    [c(16), c(8), c(0), c(24)]
}

/// The styles in FAF's `CommandGraphParams`, by `UNITCOMMAND_*` name.
pub fn parse_params(source: &str) -> mlua::Result<BTreeMap<String, Style>> {
    use mlua::prelude::*;
    let lua = Lua::new();
    lua.load(source).set_name("=commandgraphparams").exec()?;
    let params: LuaTable = lua.globals().get("CommandGraphParams")?;
    let default: Option<LuaTable> = params.get("default")?;
    // A key: the entry's, else along its `inherit_from` chain, else default's.
    let lookup = |entry: &LuaTable, key: &str| -> LuaResult<LuaValue> {
        let mut at = entry.clone();
        for _ in 0..16 {
            let v: LuaValue = at.get(key)?;
            if !v.is_nil() {
                return Ok(v);
            }
            match at.get::<Option<String>>("inherit_from")?.and_then(|p| params.get::<Option<LuaTable>>(p).ok().flatten()) {
                Some(parent) => at = parent,
                None => break,
            }
        }
        match &default {
            Some(d) => d.get(key),
            None => Ok(LuaNil),
        }
    };
    let mut out = BTreeMap::new();
    for pair in params.pairs::<String, LuaTable>() {
        let (name, entry) = pair?;
        if !name.starts_with("UNITCOMMAND_") {
            continue;
        }
        let s = |k: &str| -> LuaResult<String> { Ok(lookup(&entry, k)?.as_string().map(|s| s.to_string_lossy()).unwrap_or_default()) };
        let n = |k: &str, d: f32| -> LuaResult<f32> {
            Ok(match lookup(&entry, k)? {
                LuaValue::Integer(i) => i as f32,
                LuaValue::Number(v) => v as f32,
                _ => d,
            })
        };
        out.insert(
            name,
            Style {
                waypoint_texture: s("waypoint_texture")?.to_lowercase(),
                line_texture: s("orderline_texture")?.to_lowercase(),
                uv_aspect: n("orderline_uv_aspect_ratio", 1.0)?,
                anim_rate: n("orderline_anim_rate", 0.0)?,
                line_selected: color(&s("orderline_selected_color")?),
                line_highlight: color(&s("orderline_highlight_color")?),
                glow_selected: n("orderline_selected_glow", 0.0)?,
                glow_highlight: n("orderline_highlight_glow", 0.0)?,
                waypoint_selected: color(&s("waypoint_selected_color")?),
                waypoint_highlight: color(&s("waypoint_highlight_color")?),
            },
        );
    }
    Ok(out)
}

/// `CommandKind` -> its `UNITCOMMAND_*` name (the kinds are moho's, named
/// alike; silo builds share FAF's `BuildSilo` entry).
fn command_name(kind: u8) -> Option<String> {
    let k = CommandKind::from_u8(kind)?;
    Some(match k {
        CommandKind::BuildSiloTactical | CommandKind::BuildSiloNuke => "UNITCOMMAND_BuildSilo".to_owned(),
        k => format!("UNITCOMMAND_{k:?}"),
    })
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
    glow: f32,
}

enum Slot<T> {
    Loading,
    Ready(T),
    Failed,
}

enum Done {
    Params(Option<BTreeMap<String, Style>>),
    /// Uploaded by the loader.
    Texture(String, Option<wgpu::TextureView>),
}

/// The camera and ground a frame's graph is built against.
pub struct View<'a> {
    pub eye: Vec3,
    /// World units per pixel at distance 1 (`2 tan(fov / 2) / screen height`).
    pub px_per_distance: f32,
    /// The view's size in pixels: lines and waypoints wholly off it aren't
    /// built.
    pub screen: [f32; 2],
    pub ground: &'a dyn Fn(f32, f32) -> f32,
    /// Screen position of a world point.
    pub project: &'a dyn Fn(Vec3) -> Option<[f32; 2]>,
    /// The cursor (pixels), to highlight the waypoint under it.
    pub cursor: Option<[f32; 2]>,
    /// Seconds, for scrolling lines.
    pub time: f32,
}

pub struct CommandGraph {
    vfs: Option<Arc<RwLock<Vfs>>>,
    /// For the loaders, which upload their textures themselves.
    gpu: (wgpu::Device, wgpu::Queue),
    styles: Slot<BTreeMap<String, Style>>,
    textures: BTreeMap<String, Slot<wgpu::BindGroup>>,
    done_tx: Sender<Done>,
    done_rx: Receiver<Done>,
    layout: wgpu::BindGroupLayout,
    /// Lines repeat along u; icons clamp.
    wrap: wgpu::Sampler,
    clamp: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
    /// Selection brackets: instanced, on the ground under their units
    /// (depth-tested).
    bracket_pipeline: wgpu::RenderPipeline,
    failures: Vec<String>,
}

/// A world view's order lines, waypoints, build sites and brackets this
/// frame ([`CommandGraph::update`]), drawn with [`CommandGraph::draw`].
#[derive(Default)]
pub struct GraphFrame {
    /// This frame's vertices by layer (build sites, lines, icons) and
    /// texture, each uploaded from here; kept between frames for their
    /// allocations.
    batches: BTreeMap<u8, BTreeMap<String, Vec<Vertex>>>,
    /// Per (layer, texture): vertex range, in draw order (build sites,
    /// lines, icons).
    draws: Vec<(u8, String, std::ops::Range<u32>)>,
    buffer: Option<wgpu::Buffer>,
    capacity: usize,
    /// The selection brackets, drawn after the rest.
    bracket_instances: Vec<BracketInstance>,
    bracket_buffer: Option<wgpu::Buffer>,
    bracket_capacity: usize,
    /// Waypoints drawn this frame (their ETA labels; render tests).
    pub waypoints: Vec<Waypoint>,
    /// Build sites drawn this frame: (skirt, current) (render tests).
    pub sites: Vec<([f32; 4], bool)>,
    /// Standing structures' skirts outlined this frame (render tests).
    pub nearby_sites: Vec<[f32; 4]>,
    /// Selection brackets drawn this frame (render tests).
    pub brackets: usize,
    /// Order lines drawn this frame: (from waypoint's command, or none from
    /// a unit; to waypoint's command) (render tests).
    pub lines: Vec<(Option<u32>, u32)>,
}


const SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/command_graph.wgsl"));

fn vertex_buffer<T>(device: &wgpu::Device, label: &str, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (capacity * std::mem::size_of::<T>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl CommandGraph {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, globals: &wgpu::BindGroupLayout, format: wgpu::TextureFormat, depth: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("command graph: texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("command graph: shader"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("command graph: pipeline layout"), bind_group_layouts: &[Some(globals), Some(&layout)], immediate_size: 0 });
        // `AlphaBlend_SrcAlpha_InvSrcAlpha_Write_RGB`, and the glow pass's
        // alpha write folded in as a max.
        let glow = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Max },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        let vertices = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4, 3 => Float32],
        }];
        let brackets = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<BracketInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4],
        }];
        let make = |label, entry, buffers: &[wgpu::VertexBufferLayout], compare| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some(entry), buffers, compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &super::globals::scene_targets(format, Some(wgpu::BlendState::ALPHA_BLENDING), super::globals::GlowWrite::Blend(glow)),
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth,
                depth_write_enabled: Some(false),
                depth_compare: Some(compare),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        // Over models (the base game's lines cross the structures under
        // them); the scene draws this after them. Selection brackets lie
        // on the ground under their units (depth-tested).
        let pipeline = make("command graph: lines + icons", "vs", &vertices, wgpu::CompareFunction::Always);
        let bracket_pipeline = make("command graph: selection brackets (on the ground)", "vs_bracket", &brackets, wgpu::CompareFunction::LessEqual);
        let sampler = |label, u| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                address_mode_u: u,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        };
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let clamp = sampler("command graph: icon sampler", wgpu::AddressMode::ClampToEdge);
        let white = super::models::solid(device, queue, "command graph: white", [255; 4], wgpu::TextureFormat::Rgba8Unorm);
        let solid = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("command graph: solid colour"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&white) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&clamp) }],
        });
        Self {
            vfs: None,
            gpu: (device.clone(), queue.clone()),
            styles: Slot::Loading,
            textures: BTreeMap::from([(SOLID.to_owned(), Slot::Ready(solid))]),
            done_tx,
            done_rx,
            wrap: sampler("command graph: line sampler", wgpu::AddressMode::Repeat),
            clamp,
            layout,
            pipeline,
            bracket_pipeline,
            failures: Vec::new(),
        }
    }

    /// Takes the VFS once and starts reading FAF's params.
    pub fn set_vfs(&mut self, vfs: Option<&Arc<RwLock<Vfs>>>) {
        if self.vfs.is_some() {
            return;
        }
        let Some(vfs) = vfs.cloned() else { return };
        self.vfs = Some(vfs.clone());
        let tx = self.done_tx.clone();
        rayon::spawn(move || {
            // Translated, as scripts are: LOUD's has `###` comments.
            let src = vfs.read().ok().and_then(|v| v.read_processed(PARAMS).ok());
            let styles = src.and_then(|b| match parse_params(&String::from_utf8_lossy(&b)) {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!("{PARAMS}: {e}");
                    None
                }
            });
            let _ = tx.send(Done::Params(styles));
        });
    }

    #[allow(dead_code)]
    pub fn failures(&self) -> &[String] {
        &self.failures
    }

    /// FAF's params or a texture still loading.
    #[allow(dead_code)]
    pub fn loading(&self) -> bool {
        (self.vfs.is_some() && matches!(self.styles, Slot::Loading)) || self.textures.values().any(|t| matches!(t, Slot::Loading))
    }

    fn request_texture(&mut self, path: &str) {
        if path.is_empty() || self.textures.contains_key(path) {
            return;
        }
        let Some(vfs) = self.vfs.clone() else { return };
        self.textures.insert(path.to_owned(), Slot::Loading);
        let (tx, path, gpu) = (self.done_tx.clone(), path.to_owned(), self.gpu.clone());
        rayon::spawn(move || {
            let view = super::texture_load::load(&gpu.0, &gpu.1, &vfs, &path, false, false, "command graph");
            let _ = tx.send(Done::Texture(path, view));
        });
    }

    fn collect(&mut self, device: &wgpu::Device, lines: &std::collections::BTreeSet<String>) {
        while let Ok(d) = self.done_rx.try_recv() {
            match d {
                Done::Params(styles) => {
                    if styles.is_none() {
                        self.failures.push(format!("command graph params {PARAMS}"));
                    }
                    self.styles = styles.map_or(Slot::Failed, Slot::Ready);
                }
                Done::Texture(path, view) => {
                    let group = view.map(|v| {
                        let sampler = if lines.contains(&path) { &self.wrap } else { &self.clamp };
                        device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some(&format!("command graph: {path}")),
                            layout: &self.layout,
                            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&v) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) }],
                        })
                    });
                    if group.is_none() {
                        self.failures.push(format!("command graph texture {path}"));
                    }
                    self.textures.insert(path, group.map_or(Slot::Failed, Slot::Ready));
                }
            }
        }
    }

    /// Builds this frame's graph from the selected units' orders
    /// (`DebugData::orders`) and the units' interpolated positions.
    /// `ghost_sites`: the build placement ghosts' skirts, outlined as queued
    /// builds'.
    /// `nearby_sites`: standing structures' skirts near the ghosts (green).
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, orders: &[(u32, Vec<OrderMarker>)], brackets: &[Bracket], ghost_sites: &[[f32; 4]], nearby_sites: &[[f32; 4]], entities: &[EntityView], view: &View, out: &mut GraphFrame) {
        let line_textures: std::collections::BTreeSet<String> = match &self.styles {
            Slot::Ready(s) => s.values().map(|s| s.line_texture.clone()).collect(),
            _ => Default::default(),
        };
        self.collect(device, &line_textures);
        for batch in out.batches.values_mut().flat_map(|b| b.values_mut()) {
            batch.clear();
        }
        out.draws.clear();
        out.bracket_instances.clear();
        out.waypoints.clear();
        out.sites.clear();
        out.lines.clear();
        out.brackets = 0;
        let Slot::Ready(styles) = &self.styles else { return };
        let styles = styles.clone();
        let style_of = |kind: u8| command_name(kind).and_then(|n| styles.get(&n)).filter(|s| !s.waypoint_texture.is_empty());

        // Waypoints (by command) and lines (from a unit or a waypoint to a
        // waypoint), with how many units use each.
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        enum From {
            Unit(u32),
            Node(u32),
        }
        // Waypoints: (kind, position, ETA: the latest of its units').
        let mut nodes: BTreeMap<u32, (u8, Vec3, f32)> = BTreeMap::new();
        // Lines: units on it, and the points before and after it on the
        // first unit's chain (the curve's tangents).
        struct Edge {
            count: u32,
            before: Option<Vec3>,
            after: Option<Vec3>,
        }
        let mut edges: BTreeMap<(From, u32), Edge> = BTreeMap::new();
        let mut starts: BTreeMap<u32, Vec3> = BTreeMap::new();
        // Build sites by command: (skirt, a selected unit's current order).
        let mut sites: BTreeMap<u32, ([f32; 4], bool)> = BTreeMap::new();
        // Units sharing a first waypoint and standing together draw one line
        // to it, from their centre.
        let firsts: Vec<(u32, u32, Vec3)> = orders
            .iter()
            .filter_map(|(unit, marks)| {
                let first = marks.iter().find(|m| style_of(m.kind).is_some())?;
                let i = entities.binary_search_by_key(unit, |e| e.id).ok()?;
                Some((*unit, first.command, Vec3::from_array(entities[i].position)))
            })
            .collect();
        let link = |p: Vec3| MERGE_DISTANCE.max(MERGE_PX * view.px_per_distance * (p - view.eye).length());
        let (group_of, group_at) = merge_starts(&firsts, &link);
        for (unit, marks) in orders {
            for (i, m) in marks.iter().enumerate() {
                if let Some(site) = m.site {
                    sites.entry(m.command).or_insert((site, false)).1 |= i == 0;
                }
            }
            let Some(&group) = group_of.get(unit) else { continue };
            let unit_at = group_at[&group];
            starts.insert(group, unit_at);
            let shown: Vec<&OrderMarker> = marks.iter().filter(|m| style_of(m.kind).is_some()).collect();
            let point = |k: usize| Vec3::from_array(shown[k].node);
            for (k, m) in shown.iter().enumerate() {
                let node = nodes.entry(m.command).or_insert((m.kind, point(k), m.eta));
                node.2 = node.2.max(m.eta);
                let from = if k == 0 { From::Unit(group) } else { From::Node(shown[k - 1].command) };
                let before = match k {
                    0 => None,
                    1 => Some(unit_at),
                    _ => Some(point(k - 2)),
                };
                let after = (k + 1 < shown.len()).then(|| point(k + 1));
                edges.entry((from, m.command)).or_insert(Edge { count: 0, before, after }).count += 1;
            }
            // Patrols loop (the sim sends a reached patrol point to the back
            // of the queue): a line from the last patrol waypoint back to
            // the first.
            let patrols: Vec<usize> = (0..shown.len()).filter(|&k| matches!(CommandKind::from_u8(shown[k].kind), Some(CommandKind::Patrol | CommandKind::FormPatrol))).collect();
            if let (Some(&first), Some(&last)) = (patrols.first(), patrols.last())
                && first != last {
                let before = (last > 0).then(|| point(last - 1));
                let after = (first + 1 < shown.len()).then(|| point(first + 1));
                edges.entry((From::Node(shown[last].command), shown[first].command)).or_insert(Edge { count: 0, before, after }).count += 1;
            }
        }
        if nodes.is_empty() && sites.is_empty() && ghost_sites.is_empty() && brackets.is_empty() {
            return;
        }
        // Each waypoint's size on the ground (pixel-clamped) and whether
        // the cursor is on it.
        let mut node_size: BTreeMap<u32, (f32, bool)> = BTreeMap::new();
        for (&cmd, &(_, p, _)) in &nodes {
            let per_px = view.px_per_distance * (p - view.eye).length().max(1e-3);
            let size = (WAYPOINT_SIZE / per_px).clamp(MIN_WAYPOINT_PX, MAX_WAYPOINT_PX) * per_px;
            let hovered = match (view.cursor, (view.project)(p)) {
                (Some(c), Some(s)) => (c[0] - s[0]).hypot(c[1] - s[1]) <= 0.5 * size / per_px,
                _ => false,
            };
            node_size.insert(cmd, (size, hovered));
        }
        // (A texture's batch is looked up by name: no key string made per line.)
        fn batch<'b>(batches: &'b mut BTreeMap<u8, BTreeMap<String, Vec<Vertex>>>, layer: u8, texture: &str) -> &'b mut Vec<Vertex> {
            let layer = batches.entry(layer).or_default();
            if !layer.contains_key(texture) {
                layer.insert(texture.to_owned(), Vec::new());
            }
            layer.get_mut(texture).expect("just inserted")
        }
        let ground = |x: f32, z: f32| (view.ground)(x, z) + LIFT;
        // Selection brackets: four corner pieces round each selected unit
        // (see `BRACKET_TEXTURE`), built on the GPU.
        out.brackets = 0;
        if !brackets.is_empty() {
            self.request_texture(BRACKET_TEXTURE);
        }
        if matches!(self.textures.get(BRACKET_TEXTURE), Some(Slot::Ready(_))) {
            out.bracket_instances.extend(brackets.iter().map(|b| {
                // Moved by its centre offset in its frame, lifted by the fudge.
                let centre = b.position + b.rotation * Vec3::from_array(b.offset) + Vec3::Y * BRACKET_LIFT;
                let thickness = if b.thickness.abs() >= 1e-5 { b.thickness } else { BRACKET_SIZE };
                let per_px = view.px_per_distance * (centre - view.eye).length().max(1e-3);
                let piece = (thickness * b.half[0].max(b.half[1])).max(MIN_BRACKET_PX * per_px);
                BracketInstance { centre: centre.to_array(), piece, rotation: b.rotation.to_array(), half: b.half, _pad: [0.0; 2] }
            }));
        }
        out.brackets = brackets.len();
        // Build sites: the skirt's outline, ground-following, a few pixels
        // wide, under the lines.
        let sites: Vec<([f32; 4], bool)> = sites.values().copied().chain(ghost_sites.iter().map(|&s| (s, false))).collect();
        out.sites.extend(sites.iter().copied());
        out.nearby_sites.clear();
        out.nearby_sites.extend_from_slice(nearby_sites);
        // Standing structures' skirts first, so builds' outlines draw over them.
        let outlines = nearby_sites.iter().map(|&s| (s, SITE_NEARBY)).chain(sites.iter().map(|&(s, active)| (s, if active { SITE_ACTIVE } else { SITE_QUEUED })));
        for ([x0, z0, x1, z1], color) in outlines {
            let verts = batch(&mut out.batches, 0, SOLID);
            let corners = [Vec3::new(x0, 0.0, z0), Vec3::new(x1, 0.0, z0), Vec3::new(x1, 0.0, z1), Vec3::new(x0, 0.0, z1)];
            for k in 0..4 {
                let (a, b) = (corners[k], corners[(k + 1) % 4]);
                let len = (b - a).length();
                let dir = (b - a) / len.max(1e-6);
                let side = Vec3::new(-dir.z, 0.0, dir.x);
                let half = |p: Vec3| 0.5 * SITE_PX * view.px_per_distance * (Vec3::new(p.x, ground(p.x, p.z), p.z) - view.eye).length().max(1e-3);
                let pieces = ((len / PIECE).ceil() as usize).clamp(1, MAX_PIECES);
                let point = |t: f32, s: f32| {
                    // Past the corners by a half width, so corners close.
                    let c = a + dir * t;
                    let w = half(c);
                    let c = c + dir * w * (if t <= 0.0 { -1.0 } else if t >= len { 1.0 } else { 0.0 }) + side * w * s;
                    Vertex { pos: [c.x, ground(c.x, c.z), c.z], uv: [0.5, 0.5], color, glow: 0.0 }
                };
                for i in 0..pieces {
                    let (t0, t1) = (len * i as f32 / pieces as f32, len * (i + 1) as f32 / pieces as f32);
                    let (l0, r0, l1, r1) = (point(t0, -1.0), point(t0, 1.0), point(t1, -1.0), point(t1, 1.0));
                    verts.extend_from_slice(&[l0, r0, l1, l1, r0, r1]);
                }
            }
        }
        let max_pieces = (PIECE_BUDGET / edges.len().max(1)).clamp(MIN_PIECES, MAX_PIECES);
        // Lines: straight, turning tightly at each waypoint on the chain
        // (a corner rounded `CURVE_SMOOTHNESS` pixels each way, at most half
        // of either line; each line draws its half of the turn), cut short
        // of the waypoints' icons.
        let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
        let turn = |at: Vec3, a: f32, b: f32| (CURVE_SMOOTHNESS * view.px_per_distance * (at - view.eye).length().max(1e-3)).min(0.5 * a).min(0.5 * b);
        // Half of the rounded corner at `at` (a quadratic from `at - din * r`
        // through `at` to `at + dout * r`): t in 0.5..1 (`leaving`) or 0..0.5.
        // Cut into a piece per few pixels of its radius on screen.
        let corner = |at: Vec3, din: Vec3, dout: Vec3, r: f32, leaving: bool| -> Vec<Vec3> {
            let (a, c) = (at - din * r, at + dout * r);
            let r_px = r / (view.px_per_distance * (at - view.eye).length().max(1e-3));
            let half = ((r_px / CURVE_PX_PER_SEGMENT).ceil() as usize).clamp(1, CURVE_SEGMENTS / 2);
            (0..=half)
                .map(|k| {
                    let t = k as f32 / (2 * half) as f32 + if leaving { 0.5 } else { 0.0 };
                    let m = 1.0 - t;
                    a * (m * m) + at * (2.0 * m * t) + c * (t * t)
                })
                .collect()
        };
        // Whether points' screen box (pixels, `margin` past each edge) meets
        // the view; a point behind the camera keeps them.
        let on_screen = |points: &[Vec3], margin: f32| {
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            for p in points {
                let Some([x, y]) = (view.project)(Vec3::new(p.x, ground(p.x, p.z), p.z)) else { return true };
                (lo, hi) = ([lo[0].min(x), lo[1].min(y)], [hi[0].max(x), hi[1].max(y)]);
            }
            hi[0] >= -margin && lo[0] <= view.screen[0] + margin && hi[1] >= -margin && lo[1] <= view.screen[1] + margin
        };
        let per_px = |p: Vec3| view.px_per_distance * (Vec3::new(p.x, ground(p.x, p.z), p.z) - view.eye).length().max(1e-3);
        for (&(from, cmd), edge) in &edges {
            let (kind, to, _) = nodes[&cmd];
            let Some(style) = style_of(kind) else { continue };
            let start = flat(match from {
                From::Unit(u) => starts[&u],
                From::Node(c) => nodes[&c].1,
            });
            let to = flat(to);
            // Off the view, turns and width included: not built.
            let half_width_px = 0.5 * (edge.count as f32).sqrt().min(10.0) * LINE_SCALE / per_px(start).min(per_px(to));
            if !on_screen(&[start, to], OFF_SCREEN_MARGIN + half_width_px) {
                continue;
            }
            let (size, hovered) = node_size[&cmd];
            let chord = to - start;
            let full = chord.length();
            let radius = 0.5 * size;
            // From the start (past its waypoint) to this waypoint's edge.
            let skip = match from {
                From::Node(c) => 0.5 * node_size[&c].0,
                From::Unit(_) => 0.0,
            };
            if full <= skip + radius {
                continue;
            }
            let dir = chord / full;
            // The turn into this line at its start, and out of it at its end.
            let mut curve: Vec<Vec3> = Vec::new();
            match edge.before.map(flat).filter(|b| (start - *b).length() > 1e-3) {
                Some(b) => {
                    let r = turn(start, (start - b).length(), full);
                    curve.extend(corner(start, (start - b).normalize(), dir, r, true));
                }
                None => curve.push(start),
            }
            let tail = match edge.after.map(flat).filter(|a| (*a - to).length() > 1e-3) {
                Some(a) => {
                    let r = turn(to, full, (a - to).length());
                    corner(to, dir, (a - to).normalize(), r, false)
                }
                None => vec![to],
            };
            // The straight between, cut for the ground.
            let (from_pt, to_pt) = (*curve.last().unwrap_or(&start), tail[0]);
            let pieces = (((to_pt - from_pt).length() / PIECE).ceil() as usize).clamp(1, max_pieces.max(1));
            curve.extend((1..pieces).map(|k| from_pt.lerp(to_pt, k as f32 / pieces as f32)));
            curve.extend(tail);
            let mut along = vec![0.0f32; curve.len()];
            for k in 1..curve.len() {
                along[k] = along[k - 1] + (curve[k] - curve[k - 1]).length();
            }
            let total = *along.last().unwrap_or(&0.0);
            // The icons cover the line's ends; the turns already end short of
            // the waypoints, so only cut what's left of each icon.
            let ends = (curve.first().map_or(0.0, |p| (*p - start).length()), curve.last().map_or(0.0, |p| (*p - to).length()));
            let (a, b) = ((skip - ends.0).max(0.0), total - (radius - ends.1).max(0.0));
            if b <= a {
                continue;
            }
            let at_length = |d: f32| {
                let k = along.partition_point(|&l| l < d).clamp(1, curve.len() - 1);
                let f = (d - along[k - 1]) / (along[k] - along[k - 1]).max(1e-6);
                curve[k - 1].lerp(curve[k], f.clamp(0.0, 1.0))
            };
            let mut path = vec![at_length(a)];
            path.extend((0..curve.len()).filter(|&k| along[k] > a && along[k] < b).map(|k| curve[k]));
            path.push(at_length(b));
            // `CalculateWaypointLineWidth` in world units, at least
            // `MIN_LINE_PX` pixels at a point.
            let width = (edge.count as f32).sqrt().min(10.0) * LINE_SCALE;
            let wide = |p: Vec3| width.max(MIN_LINE_PX * view.px_per_distance * (Vec3::new(p.x, ground(p.x, p.z), p.z) - view.eye).length().max(1e-3));
            let (color, glow) = if hovered { (style.line_highlight, style.glow_highlight) } else { (style.line_selected, style.glow_selected) };
            // The texture repeats every `width / aspect` along the line,
            // scrolling toward the waypoint.
            let scroll = view.time * style.anim_rate;
            out.lines.push((match from {
                From::Node(c) => Some(c),
                From::Unit(_) => None,
            }, cmd));
            let verts = batch(&mut out.batches, 1, &style.line_texture);
            let mut u = 0.0;
            let mut last: Option<(Vertex, Vertex)> = None;
            for k in 0..path.len() {
                if k > 0 {
                    let mid = path[k - 1].lerp(path[k], 0.5);
                    u += (path[k] - path[k - 1]).length() * style.uv_aspect / wide(mid);
                }
                let along = flat(path[(k + 1).min(path.len() - 1)] - path[k.saturating_sub(1)]);
                let d = if along.length() > 1e-6 { along.normalize() } else { dir };
                let side = Vec3::new(-d.z, 0.0, d.x) * (0.5 * wide(path[k]));
                let corner = |c: Vec3, v: f32| Vertex { pos: [c.x, ground(c.x, c.z), c.z], uv: [u - scroll, v], color, glow };
                let pair = (corner(path[k] - side, 1.0), corner(path[k] + side, 0.0));
                if let Some(prev) = last {
                    verts.extend_from_slice(&[prev.0, prev.1, pair.0, pair.0, prev.1, pair.1]);
                }
                last = Some(pair);
            }
        }
        // Waypoint icons: flat squares following the ground, north up.
        for (&cmd, &(kind, p, eta)) in &nodes {
            let Some(style) = style_of(kind) else { continue };
            let (size, hovered) = node_size[&cmd];
            let color = if hovered { style.waypoint_highlight } else { style.waypoint_selected };
            let glow = if hovered { style.glow_highlight } else { 0.0 };
            out.waypoints.push(Waypoint { kind, position: p, eta, size });
            let size_px = size / per_px(p);
            if !on_screen(&[p], size_px) {
                continue;
            }
            // A small icon is one quad (see `ICON_FLAT_PX`).
            let grid = if size_px < ICON_FLAT_PX { 1 } else { ICON_GRID };
            let verts = batch(&mut out.batches, 2, &style.waypoint_texture);
            let at = |i: usize, j: usize| {
                let (u, v) = (i as f32 / grid as f32, j as f32 / grid as f32);
                let (x, z) = (p.x + (u - 0.5) * size, p.z + (v - 0.5) * size);
                Vertex { pos: [x, ground(x, z), z], uv: [u, v], color, glow }
            };
            for j in 0..grid {
                for i in 0..grid {
                    let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
                    verts.extend_from_slice(&[a, b, c, c, b, d]);
                }
            }
        }
        // Each batch is uploaded from where it was built, one after another.
        let mut count = 0usize;
        for (&layer, batches) in &out.batches {
            for (texture, verts) in batches.iter().filter(|(_, v)| !v.is_empty()) {
                self.request_texture(texture);
                if matches!(self.textures.get(texture.as_str()), Some(Slot::Ready(_))) {
                    out.draws.push((layer, texture.clone(), count as u32..(count + verts.len()) as u32));
                    count += verts.len();
                }
            }
        }
        if out.buffer.is_none() || count > out.capacity {
            out.capacity = count.max(4096).next_power_of_two();
            out.buffer = Some(vertex_buffer::<Vertex>(device, "command graph: vertices", out.capacity));
        }
        if let Some(buffer) = &out.buffer {
            for (layer, texture, range) in &out.draws {
                let verts = &out.batches[layer][texture];
                queue.write_buffer(buffer, (range.start as usize * std::mem::size_of::<Vertex>()) as u64, bytemuck::cast_slice(verts));
            }
        }
        if out.bracket_buffer.is_none() || out.bracket_instances.len() > out.bracket_capacity {
            out.bracket_capacity = out.bracket_instances.len().max(256).next_power_of_two();
            out.bracket_buffer = Some(vertex_buffer::<BracketInstance>(device, "command graph: selection brackets", out.bracket_capacity));
        }
        if let (false, Some(buffer)) = (out.bracket_instances.is_empty(), &out.bracket_buffer) {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&out.bracket_instances));
        }
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, frame: &'a GraphFrame) {
        if let (false, Some(buffer)) = (frame.draws.is_empty(), &frame.buffer) {
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, buffer.slice(..));
            for (_, texture, range) in &frame.draws {
                if let Some(Slot::Ready(g)) = self.textures.get(texture) {
                    pass.set_bind_group(1, g, &[]);
                    pass.draw(range.clone(), 0..1);
                }
            }
        }
        if let (false, Some(buffer), Some(Slot::Ready(g))) = (frame.bracket_instances.is_empty(), &frame.bracket_buffer, self.textures.get(BRACKET_TEXTURE)) {
            pass.set_pipeline(&self.bracket_pipeline);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.set_bind_group(1, g, &[]);
            pass.draw(0..BRACKET_VERTICES, 0..frame.bracket_instances.len() as u32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Moho's bracket (0x103c72f0): `SelectionSize` x 0.75 per axis (the
    /// box x 1.85 without one), corner pieces of thickness x the larger
    /// extent centred on the corners, the texture's quarter toward each.
    #[test]
    fn brackets_are_moho_corner_pieces() {
        assert_eq!(bracket_half([0.7, 0.6], [9.0, 9.0]), [0.525, 0.45000002], "the UEF ACU");
        assert_eq!(bracket_half([0.0, 2.0], [1.0, 1.0]), [1.85, 1.5], "no SelectionSizeX: the box x the fudge");
        // Unturned at the origin: the front-right piece (second) spans
        // x 0.525 +- 0.2, z 0.45 +- 0.2, with the texture's top-right quarter.
        let v = bracket_pieces(Vec3::ZERO, glam::Quat::IDENTITY, [0.525, 0.45], 0.2);
        let front_right = &v[6..12];
        let (xs, zs): (Vec<f32>, Vec<f32>) = front_right.iter().map(|p| (p.pos[0], p.pos[2])).unzip();
        let span = |a: &[f32]| (a.iter().copied().fold(f32::MAX, f32::min), a.iter().copied().fold(f32::MIN, f32::max));
        let close = |(a, b): (f32, f32), (c, d): (f32, f32)| (a - c).abs() < 1e-5 && (b - d).abs() < 1e-5;
        assert!(close(span(&xs), (0.325, 0.725)), "{:?}", span(&xs));
        assert!(close(span(&zs), (0.25, 0.65)), "{:?}", span(&zs));
        let (us, vs): (Vec<f32>, Vec<f32>) = front_right.iter().map(|p| (p.uv[0], p.uv[1])).unzip();
        assert_eq!((span(&us), span(&vs)), ((0.5, 1.0), (0.0, 0.5)));
        // The outer corner (x and z furthest out) takes the texture's corner.
        let outer = front_right.iter().max_by(|a, b| (a.pos[0] + a.pos[2]).total_cmp(&(b.pos[0] + b.pos[2]))).unwrap();
        assert_eq!(outer.uv, [1.0, 0.0]);
        // Turned a quarter: the pieces turn with the unit.
        let turned = bracket_pieces(Vec3::ZERO, glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2), [0.525, 0.45], 0.2);
        let c = turned[6..12].iter().fold(Vec3::ZERO, |a, p| a + Vec3::from_array(p.pos)) / 6.0;
        assert!((c - Vec3::new(0.45, 0.0, -0.525)).length() < 1e-4, "{c:?}");
    }

    /// `vs_bracket`'s vertex numbering, written out here as the shader has
    /// it, gives [`bracket_pieces`]' vertices: corners in the same order,
    /// each the same quad (a, b, c, c, b, d) with the same texture quarter.
    #[test]
    fn instanced_brackets_match_the_corner_pieces() {
        let (centre, rotation, half, piece) = (Vec3::new(3.0, 1.0, -2.0), glam::Quat::from_rotation_y(0.7), [0.525, 0.45], 0.2);
        let shader = |vi: u32| -> ([f32; 3], [f32; 2]) {
            let k = vi / 6;
            let q = [0u32, 1, 2, 2, 1, 3][(vi % 6) as usize];
            let (u, v) = ((q & 1) as f32, (q >> 1) as f32);
            let sx = if k & 1 == 1 { 1.0 } else { -1.0 };
            let sz = if k >= 2 { -1.0 } else { 1.0 };
            let local = Vec3::new(sx * half[0] + (u - 0.5) * 2.0 * piece, 0.0, sz * half[1] + (0.5 - v) * 2.0 * piece);
            let r = Vec3::new(rotation.x, rotation.y, rotation.z);
            let t = 2.0 * r.cross(local);
            let turned = local + rotation.w * t + r.cross(t);
            ((centre + turned).to_array(), [if sx < 0.0 { 0.0 } else { 0.5 } + 0.5 * u, if sz > 0.0 { 0.0 } else { 0.5 } + 0.5 * v])
        };
        let reference = bracket_pieces(centre, rotation, half, piece);
        assert_eq!(reference.len() as u32, BRACKET_VERTICES);
        for (vi, r) in reference.iter().enumerate() {
            let (pos, uv) = shader(vi as u32);
            assert!((Vec3::from_array(pos) - Vec3::from_array(r.pos)).length() < 1e-5, "vertex {vi}: {pos:?} vs {:?}", r.pos);
            assert_eq!(uv, r.uv, "vertex {vi}");
        }
    }

    /// `inherit_from` chains and `default` fill an entry's missing keys;
    /// colours are `AARRGGBB`.
    #[test]
    fn params_resolve_inheritance() {
        let src = "CommandGraphParams = {
            default = { orderline_texture = '/line.dds', orderline_uv_aspect_ratio = 1.0, orderline_selected_color = 'dd00ffff', waypoint_selected_color = '88ffffff' },
            colors = { orderline_selected_color = 'ddff0000' },
            UNITCOMMAND_Attack = { inherit_from = 'colors', waypoint_texture = '/Attack.dds' },
            UNITCOMMAND_FormAttack = { inherit_from = 'UNITCOMMAND_Attack' },
            UNITCOMMAND_Stop = { inherit_from = 'default' },
        }";
        let p = parse_params(src).unwrap();
        let fa = &p["UNITCOMMAND_FormAttack"];
        assert_eq!(fa.waypoint_texture, "/attack.dds");
        assert_eq!(fa.line_texture, "/line.dds");
        assert_eq!(fa.line_selected, [1.0, 0.0, 0.0, 0xdd as f32 / 255.0]);
        assert_eq!(p["UNITCOMMAND_Stop"].waypoint_texture, "");
        assert!(!p.contains_key("colors"));
    }

    /// Units on one order standing together (a chain counts) share a line
    /// from their centre; a straggler, or a unit on another order, doesn't.
    #[test]
    fn close_units_on_one_order_share_a_first_line() {
        let at = |x: f32, z: f32| Vec3::new(x, 0.0, z);
        // A column 4 apart (each within 6 of the next), a straggler far off,
        // and a unit beside the column on another order.
        let units = [(7, 1, at(0.0, 0.0)), (3, 1, at(4.0, 0.0)), (9, 1, at(8.0, 0.0)), (5, 1, at(100.0, 0.0)), (4, 2, at(2.0, 0.0))];
        let (group, centre) = merge_starts(&units, &|_| 6.0);
        assert_eq!([group[&7], group[&3], group[&9]], [3, 3, 3], "the column is one group, led by its lowest id");
        assert_eq!(centre[&3], at(4.0, 0.0));
        assert_eq!(group[&5], 5, "the straggler draws its own line");
        assert_eq!(group[&4], 4, "another order, another line");
        assert_eq!(centre.len(), 3);
        // Zoomed out (a longer link), the straggler joins.
        let (group, _) = merge_starts(&units, &|_| 100.0);
        assert_eq!(group[&5], 3);
    }

    #[test]
    fn kinds_name_their_entries() {
        assert_eq!(command_name(CommandKind::FormAggressiveMove as u8).as_deref(), Some("UNITCOMMAND_FormAggressiveMove"));
        assert_eq!(command_name(CommandKind::BuildSiloNuke as u8).as_deref(), Some("UNITCOMMAND_BuildSilo"));
    }
}
