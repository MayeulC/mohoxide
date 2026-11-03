//! The window's GPU side: the scene, the interface's and egui's passes, the
//! surface and its present. The main thread (events, input, the interface's
//! Lua) hands it each frame in two halves: [`Early`] (the snapshots taken and
//! this frame's cameras) as soon as the input is in, so the heavy prepare
//! (interpolation, posing, particles) runs beside the interface's Lua, and
//! [`Late`] (the interface's draw list, ghosts, egui's output) once that is
//! done. It hands back [`PresentOut`]: what the frame worked out (entities,
//! icons' rectangles for picking) and its timings.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use super::scene::{Scene, Stage};
use super::world::{FrameInput, FrameResults, SceneDelta};

/// The first half of a frame: the snapshots taken since the last (in
/// order) and the input as it stands after the camera moved.
pub struct Early {
    pub deltas: Vec<SceneDelta>,
    pub input: FrameInput,
}

/// The second half: the input with the interface's part (views, ghosts,
/// its meshes), the interface's draw list, egui's output, and whether to
/// save this frame (`--screenshot`).
pub struct Late {
    pub input: FrameInput,
    /// The interface's draw list (physical pixels) and its scale.
    pub ui_items: Vec<rts_core_engine::ui::controls::DrawItem>,
    pub ui_scale: f32,
    /// egui's debug windows and status bar: tessellated, with their texture
    /// changes, at `pixels_per_point`.
    pub egui: Vec<egui::ClippedPrimitive>,
    pub egui_textures: egui::TexturesDelta,
    pub pixels_per_point: f32,
    /// Save this frame (`--screenshot`).
    pub capture: Option<Capture>,
}

/// Where to save a frame, and whether the process exits once it's saved
/// (the main head's, last; a second head's goes first and doesn't).
#[derive(Clone, Debug)]
pub struct Capture {
    pub path: std::path::PathBuf,
    pub exit: bool,
}

/// Changes outside the frame's flow.
pub enum Command {
    /// The window's new size (physical pixels).
    Resize(u32, u32),
    /// The surface's present mode, and whether it stands for vsync on
    /// (Fifo; Mailbox on Wayland, [`super::state::present_mode`]).
    PresentMode { mode: wgpu::PresentMode, vsync: bool },
    /// The window is hidden (occluded, minimised), or shown again; while
    /// hidden the surface isn't touched (an acquire or present on a hidden
    /// window can wait on a vblank that never comes, holding the device
    /// both heads share) and frames draw offscreen, one per interval.
    Hidden(Option<Duration>),
    /// FAF's Frametime option: the shortest a frame may take.
    MinFrame(Duration),
    /// The monitor's refresh period, for a head that paces its own frames
    /// ([`Presenter::pace`]); `None`: it doesn't.
    VsyncPeriod(Option<Duration>),
    /// The interface's textures come from here.
    UiVfs(std::sync::Arc<std::sync::RwLock<crate::engine::vfs::Vfs>>),
    /// An image the interface draws by name: made (`replace`) or its pixels
    /// rewritten (same size).
    Image { name: String, w: u32, h: u32, rgba: Vec<u8>, replace: bool },
    /// Load the models' textures through this cache, which another head's
    /// models share (each texture in GPU memory once).
    ShareTextures(super::texture_load::Cache),
}

/// What a frame hands back to the main thread.
#[derive(Clone, Default)]
pub struct PresentOut {
    /// `None`: nothing was prepared (no snapshot yet).
    pub results: Option<FrameResults>,
    /// The strategic icons drawn, bottom to top: id and window rectangle.
    pub icon_rects: Vec<(u32, [f32; 4])>,
    /// Bounds of meshes loaded since the last frame (`Models::take_new_bounds`).
    pub mesh_bounds: Vec<(u32, [f32; 6])>,
    pub ui_draws: u32,
    /// GPU time per frame (eased; 0: not measured) and the frame's CPU time
    /// here (not waiting on the swapchain).
    pub gpu_ms: f32,
    pub cpu_ms: f32,
    /// This frame was saved (`Late::capture`).
    pub captured: bool,
}

/// Frame interval while drawing offscreen: a `--screenshot` run's or a
/// head without a window's, and a hidden window's (occluded, minimised,
/// another desktop: the interface keeps running, but at ~10 Hz rather than
/// spinning a core).
pub const OFFSCREEN_FRAME: Duration = Duration::from_millis(16);
pub const HIDDEN_FRAME: Duration = Duration::from_millis(100);
/// How long before a drawable is due a paced head asks for it
/// ([`Presenter::pace`]): its slack for a late estimate.
const PACE_MARGIN: Duration = Duration::from_millis(2);

/// A window's GPU side: see the module docs.
pub struct Presenter {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// `None`: a head with no window (`--second-head offscreen`), drawn
    /// offscreen.
    surface: Option<wgpu::Surface<'static>>,
    config: wgpu::SurfaceConfiguration,
    pub scene: Scene,
    ui_renderer: super::ui::UiRenderer,
    egui_renderer: egui_wgpu::Renderer,
    /// The world overlays (lifebars, order ETAs, the drag box): their own
    /// egui context, drawn between the scene and the interface.
    world_ctx: egui::Context,
    world_renderer: egui_wgpu::Renderer,
    gpu_prof: Option<super::gpu_prof::GpuProfiler>,
    gpu_ms: f32,
    icon_cache: super::overlays::IconCache,
    /// The prepare's heavy part, running on the render pool since `early`.
    stage: Option<Receiver<Stage>>,
    /// When the current frame began (its CPU time), and the time spent
    /// waiting on the swapchain in it.
    started: Instant,
    min_frame: Duration,
    /// When the Frametime cap last let a frame start: the next may start
    /// one `min_frame` after it (a schedule, so the time outside a frame
    /// and sleep's overshoot don't add to the cap: 8 ms measured from each
    /// frame's start ran 120 Hz at ~116).
    frame_due: Instant,
    /// [`Command::VsyncPeriod`], and when the last acquire returned (a
    /// drawable was free then; the next is about a period later).
    vsync_period: Option<Duration>,
    last_acquired: Option<Instant>,
    /// The present mode stands for vsync on ([`Command::PresentMode`]).
    vsync: bool,
    /// [`Command::Hidden`]: the frame interval while the window is hidden.
    hidden: Option<Duration>,
    /// `--screenshot` runs: offscreen frames paced faster than a hidden
    /// window's.
    screenshot_run: bool,
    /// A texture cache to share, applied at the next frame's start (the
    /// models are lent to the prepare between a frame's halves).
    share: Option<super::texture_load::Cache>,
}

impl Presenter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, surface: Option<wgpu::Surface<'static>>, config: wgpu::SurfaceConfiguration, scene: Scene, ui_renderer: super::ui::UiRenderer, screenshot_run: bool, gpu_passes: bool) -> Self {
        let egui_renderer = egui_wgpu::Renderer::new(&device, config.format, egui_wgpu::RendererOptions::default());
        let world_renderer = egui_wgpu::Renderer::new(&device, config.format, egui_wgpu::RendererOptions::default());
        let gpu_prof = super::gpu_prof::GpuProfiler::new(&device, &queue, gpu_passes);
        tracing::info!(gpu_timing = gpu_prof.is_some(), "graphics timing");
        Self {
            device,
            queue,
            surface,
            config,
            scene,
            ui_renderer,
            egui_renderer,
            world_ctx: egui::Context::default(),
            world_renderer,
            gpu_prof,
            gpu_ms: 0.0,
            icon_cache: Default::default(),
            stage: None,
            started: Instant::now(),
            min_frame: Duration::ZERO,
            frame_due: Instant::now(),
            vsync_period: None,
            last_acquired: None,
            // Surfaces are made with vsync on (a second head made after
            // it was turned off is told so).
            vsync: true,
            hidden: None,
            screenshot_run,
            share: None,
        }
    }

    pub fn command(&mut self, c: Command) {
        match c {
            Command::Resize(w, h) => {
                if w == 0 || h == 0 {
                    return;
                }
                (self.config.width, self.config.height) = (w, h);
                if let Some(s) = &self.surface {
                    s.configure(&self.device, &self.config);
                }
                self.scene.resize(&self.device, w, h);
            }
            Command::PresentMode { mode, vsync } => {
                self.vsync = vsync;
                if mode != self.config.present_mode {
                    tracing::info!(?mode, "vertical sync");
                    self.config.present_mode = mode;
                    if let Some(s) = &self.surface {
                        s.configure(&self.device, &self.config);
                    }
                }
            }
            Command::MinFrame(d) => self.min_frame = d,
            Command::VsyncPeriod(p) => self.vsync_period = p,
            Command::Hidden(interval) => {
                if interval.is_some() != self.hidden.is_some() {
                    tracing::info!(thread = std::thread::current().name().unwrap_or_default(), hidden = interval.is_some(), "window visibility");
                }
                self.hidden = interval;
                // Shown again: the next acquire starts a fresh estimate.
                self.last_acquired = None;
            }
            Command::UiVfs(vfs) => self.ui_renderer.set_vfs(vfs),
            Command::Image { name, w, h, rgba, replace: true } => self.ui_renderer.set_image(&name, w, h, rgba),
            Command::Image { name, w, h, rgba, replace: false } => self.ui_renderer.update_image(&self.queue, &name, w, h, &rgba),
            Command::ShareTextures(cache) => self.share = Some(cache),
        }
    }

    /// The frame's first half: the snapshots in, the input as it stands, and
    /// the heavy prepare started on the render pool (`late` joins it).
    pub fn early(&mut self, early: Early) {
        let _s = rts_core_engine::prof_span!("present: early");
        self.started = Instant::now();
        if let Some(cache) = self.share.take() {
            self.scene.stage.models.share_textures(cache);
        }
        for delta in early.deltas {
            if delta.new_session {
                self.new_session();
            }
            self.scene.apply(&self.device, delta);
        }
        self.scene.set_input(&self.device, early.input);
        if let Some(mut stage) = self.scene.prepare_begin(&self.device, &self.queue) {
            let (device, queue) = (self.device.clone(), self.queue.clone());
            let (tx, rx) = std::sync::mpsc::channel();
            super::pool().spawn(move || {
                let _s = rts_core_engine::prof_span!("prepare scene (worker)");
                stage.run(&device, &queue);
                let _ = tx.send(stage);
            });
            self.stage = Some(rx);
        }
    }

    /// Another session's engine started: a fresh scene (nothing of the last
    /// session's meshes, terrain, effects or decals), still sharing the
    /// head's texture cache, so textures loaded before stay loaded.
    fn new_session(&mut self) {
        let _s = rts_core_engine::prof_span!("present: new session");
        let share = self.scene.stage.models.texture_share();
        self.scene = Scene::new(&self.device, &self.queue, self.config.format, self.config.width, self.config.height);
        if let Some(cache) = share {
            self.scene.stage.models.share_textures(cache);
        }
    }

    /// The frame's second half: the prepare finished with the interface's
    /// input, the strategic icons and world overlays laid out at its
    /// positions, then everything encoded, submitted and presented. Returns
    /// what the frame worked out and how long to wait before the next (the
    /// Frametime option's cap; offscreen frames' pacing).
    pub fn late(&mut self, late: Late) -> (PresentOut, Duration) {
        let _frame = rts_core_engine::prof_span!("present: late");
        let mut waited = Duration::ZERO;
        self.scene.set_input(&self.device, late.input);
        let mut results = None;
        if let Some(rx) = self.stage.take() {
            let wait = Instant::now();
            let stage = rx.recv().expect("prepare worker panicked");
            waited += wait.elapsed();
            let _s = rts_core_engine::prof_span!("prepare scene");
            self.scene.prepare_finish(stage, &self.device, &self.queue);
            results = Some(self.scene.results());
        }
        let input = self.scene.input().clone();
        let screen = [self.config.width as f32, self.config.height as f32];
        // The strategic icons at this frame's positions, under the interface.
        let icon_rects = {
            let _s = rts_core_engine::prof_span!("strategic icons");
            let sources = super::overlays::Sources { scene: &self.scene, input: &input };
            let (mut items, rects) = super::overlays::icon_items(&sources, &mut self.icon_cache);
            items.extend(late.ui_items);
            let _u = rts_core_engine::prof_span!("interface upload");
            self.ui_renderer.scale = late.ui_scale;
            self.ui_renderer.prepare(&self.device, &self.queue, &items, screen);
            rects
        };
        // The world overlays, in their own egui context (logical points).
        let ppp = late.pixels_per_point;
        if self.world_ctx.pixels_per_point() != ppp {
            self.world_ctx.set_pixels_per_point(ppp);
        }
        let points = [screen[0] / ppp, screen[1] / ppp];
        let world_input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(points[0], points[1]))), ..Default::default() };
        let world_output = {
            let _s = rts_core_engine::prof_span!("world overlays");
            let sources = super::overlays::Sources { scene: &self.scene, input: &input };
            self.world_ctx.run_ui(world_input, |ui| super::overlays::draw_world_overlays(ui.ctx(), &sources))
        };
        let world_primitives = self.world_ctx.tessellate(world_output.shapes, world_output.pixels_per_point);

        let acquire = Instant::now();
        let current = match &self.surface {
            Some(s) if self.hidden.is_none() => {
                self.pace();
                let current = s.get_current_texture();
                self.last_acquired = Some(Instant::now());
                Some(current)
            }
            _ => None,
        };
        waited += acquire.elapsed();
        let surface_texture = match current {
            Some(wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t)) => Some(t),
            // Stale or lost surface config: reconfigure and retry promptly.
            Some(wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost) => {
                if let Some(s) = &self.surface {
                    s.configure(&self.device, &self.config);
                }
                return (self.out(results, icon_rects, 0.0), Duration::from_millis(16));
            }
            // Window hidden (or a transient timeout / validation error, or
            // `Command::Hidden`, which doesn't ask the surface): draw
            // offscreen, so the interface keeps running (a hidden lobby still
            // answers its peers) and `--screenshot` works without a visible
            // window. Paced below: there is no vsync to wait on.
            _ => None,
        };
        let offscreen = surface_texture.is_none().then(|| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("frame: offscreen target"),
                size: wgpu::Extent3d { width: self.config.width, height: self.config.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        });
        let frame_texture: &wgpu::Texture = match (&surface_texture, &offscreen) {
            (Some(t), _) => &t.texture,
            (None, Some(t)) => t,
            (None, None) => unreachable!(),
        };
        let view = super::view(frame_texture, "frame: target");
        let screen_descriptor = egui_wgpu::ScreenDescriptor { size_in_pixels: [self.config.width, self.config.height], pixels_per_point: ppp };

        // The frame's command buffers, submitted in order: the GPU timing
        // bracket's start, the scene's (two per world view, then its post
        // passes), then the interface's.
        let encode = rts_core_engine::prof_span!("encode");
        let mut bracket = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame: start (GPU timing)") });
        if let Some(g) = self.gpu_prof.as_mut() {
            g.collect(&self.device);
            if g.frame_ms > 0.0 {
                self.gpu_ms += (g.frame_ms - self.gpu_ms) * if self.gpu_ms == 0.0 { 1.0 } else { 0.1 };
            }
            g.size = (self.config.width, self.config.height);
            g.begin_frame();
            g.start_bracket(&mut bracket);
        }
        // Only when the pass runs (unwritten queries would read as garbage).
        let normals_zone = if self.scene.map.is_ready() { self.gpu_prof.as_mut().and_then(|g| g.zone(super::gpu_prof::gpu_zone!("terrain normals pass"))) } else { None };
        let scene_zone = self.gpu_prof.as_mut().and_then(|g| g.zone(super::gpu_prof::gpu_zone!("scene pass")));
        let ui_zone = self.gpu_prof.as_mut().and_then(|g| g.zone(super::gpu_prof::gpu_zone!("ui pass")));
        for (id, delta) in &late.egui_textures.set {
            self.egui_renderer.update_texture(&self.device, &self.queue, *id, delta);
        }
        let scene_encoders = self.scene.draw(&self.device, &view, self.gpu_prof.as_ref().and_then(|g| g.writes(scene_zone)), self.gpu_prof.as_ref().and_then(|g| g.writes(normals_zone)));
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame: overlays + interface + debug windows") });
        // The world overlays over the scene, under the interface.
        for (id, delta) in &world_output.textures_delta.set {
            self.world_renderer.update_texture(&self.device, &self.queue, *id, delta);
        }
        let world_cmd_bufs = self.world_renderer.update_buffers(&self.device, &self.queue, &mut encoder, &world_primitives, &screen_descriptor);
        if !world_primitives.is_empty() {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("world overlays (selection, drag box)"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
                    depth_stencil_attachment: None,
                    timestamp_writes: super::gpu_prof::pass_writes("world overlays (selection, drag box)"),
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            self.world_renderer.render(&mut pass, &world_primitives, &screen_descriptor);
        }
        for id in &world_output.textures_delta.free {
            self.world_renderer.free_texture(id);
        }
        // The SupCom interface over the scene (under the debug windows).
        {
            let _s = rts_core_engine::prof_span!("encode interface");
            self.ui_renderer.draw(&mut encoder, &view);
        }
        let extra_cmd_bufs = self.egui_renderer.update_buffers(&self.device, &self.queue, &mut encoder, &late.egui, &screen_descriptor);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("debug windows (egui)"),
                    // Over the scene.
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
                    depth_stencil_attachment: None,
                    timestamp_writes: self.gpu_prof.as_ref().and_then(|g| g.writes(ui_zone)).or_else(|| super::gpu_prof::pass_writes("debug windows (egui)")),
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            self.egui_renderer.render(&mut pass, &late.egui, &screen_descriptor);
        }
        for id in &late.egui_textures.free {
            self.egui_renderer.free_texture(id);
        }
        // `--screenshot`: copy this frame out.
        let readback = late.capture.as_ref().map(|_| {
            let (w, h) = (self.config.width, self.config.height);
            let row = (w * 4).div_ceil(256) * 256;
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("frame: screenshot readback"), size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            encoder.copy_texture_to_buffer(
                frame_texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            (buffer, w, h, row)
        });
        if let Some(g) = self.gpu_prof.as_mut() {
            g.end_bracket(&mut encoder);
            g.resolve(&mut encoder);
        }
        let _ = encode;
        let submit_span = rts_core_engine::prof_span!("submit");
        // Finishing an encoder is where wgpu replays its recorded passes:
        // the frame's encoders finish in parallel on the render pool, in
        // order (the bracket, each view's two, the post passes, the
        // interface).
        let mut encoders = vec![bracket];
        encoders.extend(scene_encoders);
        encoders.push(encoder);
        let n = encoders.len();
        let finished: Vec<wgpu::CommandBuffer> = super::pool().install(|| {
            use rayon::prelude::*;
            encoders
                .into_par_iter()
                .enumerate()
                .map(|(i, e)| {
                    // One span per encoder, so a trace shows which pass costs.
                    let _s = match i {
                        0 => rts_core_engine::prof_span!("finish bracket (worker)"),
                        i if i + 1 == n => rts_core_engine::prof_span!("finish interface (worker)"),
                        i if i + 2 == n => rts_core_engine::prof_span!("finish post (worker)"),
                        i if i % 2 == 1 => rts_core_engine::prof_span!("finish shadows + terrain (worker)"),
                        _ => rts_core_engine::prof_span!("finish scene (worker)"),
                    };
                    e.finish()
                })
                .collect()
        });
        self.queue.submit(world_cmd_bufs.into_iter().chain(extra_cmd_bufs).chain(finished));
        if let Some(g) = self.gpu_prof.as_mut() {
            g.after_submit();
        }
        let mut captured = false;
        if let (Some(capture), Some((buffer, w, h, row))) = (late.capture, readback) {
            let r = results.clone().unwrap_or_default();
            tracing::info!(draws_models = r.model_draws, draws_particles = r.particle_draws, draws_ui = self.ui_renderer.draw_count(), particles = r.particles, entities = r.entities.len(), gpu_ms = self.gpu_ms, "screenshot frame");
            save_png(&self.device, self.config.format, &buffer, w, h, row, &capture.path);
            if capture.exit {
                std::process::exit(0);
            }
            captured = true;
        }
        let _ = submit_span;
        let presented = surface_texture.is_some();
        if let Some(t) = surface_texture {
            let _s = rts_core_engine::prof_span!("present");
            let present = Instant::now();
            t.present();
            waited += present.elapsed();
        }
        let cpu = self.started.elapsed().saturating_sub(waited).as_secs_f32() * 1000.0;
        // A live view: the next frame at once. With vsync on, the display
        // paces it and FAF's Frametime cap is ignored (it only ever made
        // frames later than the refresh); off, the cap's shortest frame
        // first. Offscreen frames have no vsync: paced, rather than
        // spinning and taking CPU from the sim.
        let delay = if presented && self.vsync {
            Duration::ZERO
        } else if presented {
            let now = Instant::now();
            let due = self.frame_due + self.min_frame;
            // A frame or more behind: the schedule starts again from now.
            self.frame_due = if due + self.min_frame < now { now } else { due };
            self.frame_due.saturating_duration_since(now)
        } else if self.screenshot_run || self.surface.is_none() {
            OFFSCREEN_FRAME
        } else {
            self.hidden.unwrap_or(HIDDEN_FRAME)
        };
        let mut out = self.out(results, icon_rects, cpu);
        out.captured = captured;
        out.mesh_bounds = self.scene.take_new_mesh_bounds();
        (out, delay)
    }

    /// A head with a [`Command::VsyncPeriod`] (the second) waits for its
    /// drawable here, until [`PACE_MARGIN`] before it is due, rather than in
    /// `get_current_texture`: wgpu-core holds the device's fence lock through
    /// that wait, and every `Queue::submit` on the shared device (the main
    /// window's) waits on it (a 60 Hz head held the main window's submits
    /// ~13 ms a frame). Each acquire re-anchors the estimate, so it doesn't
    /// drift; vsync off (no wait in the acquire): no pacing. Vsync by
    /// Mailbox (Wayland) has no wait in the acquire either: this is all its
    /// pacing, one whole period from the last acquire.
    fn pace(&self) {
        let (Some(period), Some(at), Some(_)) = (self.vsync_period, self.last_acquired, &self.surface) else { return };
        if !self.vsync {
            return;
        }
        let margin = if self.config.present_mode == wgpu::PresentMode::Mailbox { Duration::ZERO } else { PACE_MARGIN };
        let due = at + period.saturating_sub(margin);
        let now = Instant::now();
        if due > now {
            let _s = rts_core_engine::prof_span!("pace (vsync)");
            std::thread::sleep(due - now);
        }
    }

    /// A second half that came too late to draw: its input and egui's
    /// texture changes still apply.
    fn absorb(&mut self, late: &Late) {
        self.scene.set_input(&self.device, late.input.clone());
        for (id, delta) in &late.egui_textures.set {
            self.egui_renderer.update_texture(&self.device, &self.queue, *id, delta);
        }
        for id in &late.egui_textures.free {
            self.egui_renderer.free_texture(id);
        }
    }

    fn out(&self, results: Option<FrameResults>, icon_rects: Vec<(u32, [f32; 4])>, cpu_ms: f32) -> PresentOut {
        PresentOut { results, icon_rects, mesh_bounds: Vec::new(), ui_draws: self.ui_renderer.draw_count(), gpu_ms: self.gpu_ms, cpu_ms, captured: false }
    }
}

/// Maps the readback buffer and writes it as an RGBA PNG.
fn save_png(device: &wgpu::Device, format: wgpu::TextureFormat, buffer: &wgpu::Buffer, w: u32, h: u32, row: u32, path: &std::path::Path) {
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let data = slice.get_mapped_range();
    let bgra = matches!(format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let line = &data[(y * row) as usize..(y * row + w * 4) as usize];
        for px in line.chunks_exact(4) {
            if bgra {
                pixels.extend_from_slice(&[px[2], px[1], px[0], 255]);
            } else {
                pixels.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
    }
    let file = std::fs::File::create(path).expect("screenshot file");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut wr| wr.write_image_data(&pixels)).expect("write png");
    tracing::info!(path = %path.display(), "screenshot saved");
}

/// What the main thread sends the present thread, in order.
pub enum Message {
    Early(Early),
    Late(Box<Late>),
    Command(Command),
}

/// How long the present thread waits for the main thread's half of a frame
/// before drawing the last one again (interpolated afresh, so units keep
/// moving): a slow interface frame (a Lua hitch, the interface starting up)
/// doesn't stop the picture.
const MAIN_WAIT: Duration = Duration::from_millis(50);

/// The main thread's handle on the present thread.
pub struct PresentThread {
    tx: std::sync::mpsc::Sender<Message>,
    out: std::sync::Arc<std::sync::Mutex<Option<PresentOut>>>,
    /// A frame's "ready" is out and the main thread hasn't taken it.
    pending: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl PresentThread {
    /// Starts `presenter` on its own thread, paced by its swapchain. `ready`
    /// tells the main thread to make the next frame (each time the thread
    /// starts one).
    pub fn spawn(presenter: Presenter, name: &str, ready: impl Fn() + Send + 'static) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let out = std::sync::Arc::new(std::sync::Mutex::new(None));
        let pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (o, p) = (out.clone(), pending.clone());
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                rts_core_engine::engine::tracy::name_current();
                super::prioritise_render_thread();
                run(presenter, rx, o, p, ready);
            })
            .expect("present thread");
        Self { tx, out, pending }
    }

    pub fn command(&self, c: Command) {
        let _ = self.tx.send(Message::Command(c));
    }

    pub fn early(&self, e: Early) {
        let _ = self.tx.send(Message::Early(e));
    }

    pub fn late(&self, l: Late) {
        let _ = self.tx.send(Message::Late(Box::new(l)));
    }

    /// The latest frame's output, if one finished since the last call; the
    /// main thread is making the frame its "ready" asked for.
    pub fn take_out(&self) -> Option<PresentOut> {
        self.pending.store(false, std::sync::atomic::Ordering::Release);
        self.out.lock().ok()?.take()
    }
}

/// What the present thread waits for.
enum Got {
    Early(Early),
    Late(Box<Late>),
    Timeout,
    Closed,
}

/// The last frame's second half, drawn again when the main thread is late.
#[derive(Default)]
struct LastLate {
    ui_items: Vec<rts_core_engine::ui::controls::DrawItem>,
    ui_scale: f32,
    egui: Vec<egui::ClippedPrimitive>,
    pixels_per_point: f32,
}

impl LastLate {
    fn keep(&mut self, l: &Late) {
        self.ui_items.clone_from(&l.ui_items);
        self.ui_scale = l.ui_scale;
        self.egui.clone_from(&l.egui);
        self.pixels_per_point = l.pixels_per_point;
    }

    fn again(&self, input: FrameInput) -> Late {
        Late {
            input,
            ui_items: self.ui_items.clone(),
            ui_scale: self.ui_scale.max(0.25),
            egui: self.egui.clone(),
            egui_textures: Default::default(),
            pixels_per_point: self.pixels_per_point.max(0.25),
            capture: None,
        }
    }
}

/// The present thread: each frame asks the main thread for it, takes its
/// first half (starting the prepare), then its second half, and draws;
/// halves that don't come in time are the last frame's again.
fn run(mut p: Presenter, rx: Receiver<Message>, out: std::sync::Arc<std::sync::Mutex<Option<PresentOut>>>, pending: std::sync::Arc<std::sync::atomic::AtomicBool>, ready: impl Fn()) {
    let mut last = LastLate::default();
    // A first half that came while the thread waited for a second.
    let mut early_next: Option<Early> = None;
    loop {
        if !pending.swap(true, std::sync::atomic::Ordering::AcqRel) {
            ready();
        }
        let early = match early_next.take() {
            Some(e) => Some(e),
            None => match wait(&rx, &mut p) {
                Got::Early(e) => Some(e),
                // A late second half: kept for the frames to come.
                Got::Late(l) => {
                    p.absorb(&l);
                    last.keep(&l);
                    None
                }
                Got::Timeout => None,
                Got::Closed => return,
            },
        };
        let input = early.as_ref().map_or_else(|| p.scene.input().clone(), |e| e.input.clone());
        let fresh = early.is_some();
        p.early(early.unwrap_or(Early { deltas: Vec::new(), input }));
        let late = if fresh {
            match wait(&rx, &mut p) {
                Got::Late(l) => *l,
                Got::Early(e) => {
                    early_next = Some(e);
                    last.again(p.scene.input().clone())
                }
                Got::Timeout => last.again(p.scene.input().clone()),
                Got::Closed => return,
            }
        } else {
            last.again(p.scene.input().clone())
        };
        last.keep(&late);
        let (o, delay) = p.late(late);
        if let Ok(mut slot) = out.lock() {
            // (Results not yet taken are kept if this frame prepared none.)
            let kept = slot.take();
            let results = o.results.clone().or_else(|| kept.as_ref().and_then(|s| s.results.clone()));
            let captured = o.captured || kept.as_ref().is_some_and(|s| s.captured);
            // (Mesh bounds not yet taken are kept too: each comes once.)
            let mut mesh_bounds = kept.map(|s| s.mesh_bounds).unwrap_or_default();
            mesh_bounds.extend_from_slice(&o.mesh_bounds);
            *slot = Some(PresentOut { results, captured, mesh_bounds, ..o });
        }
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }
}

/// The next half of a frame, applying commands as they come; `Timeout`
/// after [`MAIN_WAIT`].
fn wait(rx: &Receiver<Message>, p: &mut Presenter) -> Got {
    let deadline = Instant::now() + MAIN_WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(Message::Command(c)) => p.command(c),
            Ok(Message::Early(e)) => return Got::Early(e),
            Ok(Message::Late(l)) => return Got::Late(l),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => return Got::Timeout,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Got::Closed,
        }
    }
}
