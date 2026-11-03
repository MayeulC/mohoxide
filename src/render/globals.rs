//! The per-frame uniforms every scene shader shares (bind group 0), in one
//! place for Rust and WGSL so the declarations can't drift: camera, light,
//! overlay, and the sun shadows (cascade matrices + the terrain horizon map).

use bytemuck::{Pod, Zeroable};

/// Cascades the sun shadow map has (texture array layers).
pub const CASCADES: usize = 3;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    /// From the sun (models' lighting).
    pub light_dir: [f32; 4],
    /// x = water height (or -1e9 without water), y = time (s), z = game
    /// time the frame shows (s; tread marks fade by it).
    pub params: [f32; 4],
    /// x = overlay on (0/1), y = mode (0 nav, 1 threat), zw = world extent.
    pub overlay: [f32; 4],
    /// Eye position.
    pub camera: [f32; 4],
    /// Camera forward (unit); w: its LOD scale (`cam_SetLOD`), which the
    /// shaders' LOD cutoffs multiply view depth by.
    pub forward: [f32; 4],
    /// Sun view-projection per cascade.
    pub light_vp: [[[f32; 4]; 4]; CASCADES],
    /// View depth where cascades 0 and 1 end, cascades in use (0 = no
    /// shadows), 1 / shadow map size.
    pub shadow: [f32; 4],
    /// Terrain horizon map: 1 / map width, 1 / map height, on (0/1).
    pub horizon: [f32; 4],
    /// The map's sun colour (FA's `sunDiffuse`); w = `lightMultiplier`.
    pub sun_color: [f32; 4],
    /// The map's ambient light (`sunAmbient`); w = log2(environment cube
    /// face size / 256): FAF picks cube mips as if faces were 256 texels.
    pub ambience: [f32; 4],
    /// The map's shadow fill colour (`shadowFill`).
    pub shadow_fill: [f32; 4],
    /// Fog of war: x = on (0/1), yz = the fog grid's world extent.
    pub fog: [f32; 4],
    /// The terrain mesh this frame: x = its sample step (level of detail),
    /// yz = the map's width and height in cells (0 before it loads).
    pub terrain: [f32; 4],
    /// Sun shadows: x = view depth where they end (the last cascade's end:
    /// `ren_ShadowLOD` over the LOD scale, or where the view meets the
    /// ground); nothing past it is shadowed.
    pub shadow_reach: [f32; 4],
    /// x = `graphics_Fidelity` (0 low, 1 medium, 2 high).
    pub graphics: [f32; 4],
    /// Fog of war's vision circles' grid (`render/vision.rs`): origin
    /// (world xz), cell size, circles (0: none).
    pub vision: [f32; 4],
    /// Its cells across and down.
    pub vision_dims: [u32; 4],
}

/// The scene frame's format: gamma-space colour, as D3D9 FA's framebuffer
/// held it, so blending (additive effects above all) matches FA; the bloom's
/// final add converts it for the sRGB surface.
pub const FRAME_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The scene pass's second colour target: each pixel's glow, which the
/// bloom (`bloom.rs`) blurs and adds back. FA keeps it in the frame's alpha;
/// a separate target leaves our alpha free for blending.
pub const GLOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// What a scene pipeline does to the glow target (FA's `Write_RGB` versus
/// `Write_RGBA` alpha states).
#[derive(Clone, Copy)]
pub enum GlowWrite {
    /// Leaves it (`Write_RGB`): terrain, water, particles, blended meshes.
    Keep,
    /// Writes `@location(1)`'s red.
    Replace,
    /// Blends `@location(1)` in with this state (shields).
    Blend(wgpu::BlendState),
}

/// The scene pass's colour targets for a pipeline: the frame (`format`,
/// `blend`) and the glow.
pub fn scene_targets(format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>, glow: GlowWrite) -> [Option<wgpu::ColorTargetState>; 2] {
    let (glow_blend, write_mask) = match glow {
        GlowWrite::Keep => (None, wgpu::ColorWrites::empty()),
        GlowWrite::Replace => (None, wgpu::ColorWrites::RED),
        GlowWrite::Blend(b) => (Some(b), wgpu::ColorWrites::RED),
    };
    [Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL }), Some(wgpu::ColorTargetState { format: GLOW_FORMAT, blend: glow_blend, write_mask })]
}

