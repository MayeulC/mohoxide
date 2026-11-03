//! GPU timing (on adapters with `TIMESTAMP_QUERY`): every frame's GPU time
//! for the status bar, and per-pass zones for Tracy (`--features tracy`).
//!
//! The frame is bracketed by two tiny timestamped passes (each clears a 1x1
//! target: Metal drops empty passes, timestamps and all)
//! ([`GpuProfiler::start_bracket`], [`GpuProfiler::end_bracket`]), so its GPU time covers every pass between
//! them (shadows, terrain, scene, post, interface), and the named passes
//! get begin/end timestamps of their own (`RenderPassDescriptor::
//! timestamp_writes`). After submit, the frame's queries resolve into a
//! readback buffer that is mapped asynchronously; a later frame reads it:
//! the bracket gives [`GpuProfiler::frame_ms`], the zones go to a Tracy GPU
//! context, so the GPU track lines up under the CPU frame spans. Timing
//! never stalls the frame: a ring of readback buffers, and a frame whose
//! slot is still in flight just isn't timed. Without the adapter support
//! there is no profiler (the bar shows no GPU time).
//!
//! `--gpu-passes`: every pass ([`pass_writes`], [`compute_pass_writes`])
//! and every debug group inside one ([`Timed::timed_push`]; adapters with
//! `TIMESTAMP_QUERY_INSIDE_PASSES`) gets timestamps from a query set of its
//! own, and every [`REPORT_FRAMES`] timed frames the mean GPU ms of each is
//! logged (target `gpu_passes`), passes in frame order with their groups
//! under them. On NVIDIA and AMD the stamps are exact. On Apple GPUs passes
//! overlap and stamps lag: the first heavy pass reads about right, later
//! passes absorb the time they wait behind it, and the frame total reads
//! low (0.7-1.6 ms against ~6 ms in a Metal System Trace, M5 Pro,
//! 2026-10-24); use Instruments or an Xcode frame capture there. Recording happens on the render thread (a thread-local
//! recorder per frame), so the passes need nothing passed down.

/// A zone name for [`GpuProfiler::zone`]: `gpu_zone!("scene")`.
macro_rules! gpu_zone {
    ($name:literal) => {{
        #[cfg(feature = "tracy")]
        let l = tracy_client::span_location!($name);
        #[cfg(not(feature = "tracy"))]
        let l = $name;
        l
    }};
}
pub(crate) use gpu_zone;

#[cfg(feature = "tracy")]
pub type ZoneName = &'static tracy_client::SpanLocation;
#[cfg(not(feature = "tracy"))]
pub type ZoneName = &'static str;

/// Timestamped passes per frame: the two bracket passes, then named zones.
const MAX_ZONES: u32 = 6;
/// `--gpu-passes`: timestamps per frame for passes and groups (two each).
const GROUP_QUERIES: u32 = 256;
/// `--gpu-passes`: timed frames per logged report.
pub const REPORT_FRAMES: u32 = 120;

/// The pass and group timestamps' query set (one device per process).
static GROUP_SET: std::sync::OnceLock<wgpu::QuerySet> = std::sync::OnceLock::new();

/// A timed pass or group this frame: its begin query (end = begin + 1).
#[derive(Clone, Debug)]
struct Entry {
    name: String,
    depth: u8,
    begin: u32,
}

/// This frame's pass and group timestamps, while one is being recorded.
struct Recorder {
    /// The frame slot's first query.
    base: u32,
    used: u32,
    /// Timestamps inside passes (`TIMESTAMP_QUERY_INSIDE_PASSES`).
    inside: bool,
    /// Open groups (their entry, or `None` when untimed).
    stack: Vec<Option<usize>>,
    entries: Vec<Entry>,
}

thread_local! {
    static RECORDER: std::cell::RefCell<Option<Recorder>> = const { std::cell::RefCell::new(None) };
}

/// Two queries for an entry named `name` at `depth`, if a frame is being
/// recorded and has room; the begin query.
fn allocate(name: &str, depth: Option<u8>) -> Option<u32> {
    RECORDER.with_borrow_mut(|r| {
        let r = r.as_mut()?;
        if r.used + 2 > GROUP_QUERIES {
            return None;
        }
        let begin = r.base + r.used;
        r.used += 2;
        let depth = depth.unwrap_or(r.stack.len() as u8);
        r.entries.push(Entry { name: name.to_string(), depth, begin });
        Some(begin)
    })
}

/// `--gpu-passes`: the timestamps for a render pass labelled `label` (`None`
/// when off: put it in the pass's `timestamp_writes`).
pub fn pass_writes(label: &str) -> Option<wgpu::RenderPassTimestampWrites<'static>> {
    let set = GROUP_SET.get()?;
    let begin = allocate(label, None)?;
    Some(wgpu::RenderPassTimestampWrites { query_set: set, beginning_of_pass_write_index: Some(begin), end_of_pass_write_index: Some(begin + 1) })
}

/// `--gpu-passes`: as [`pass_writes`], for a compute pass.
pub fn compute_pass_writes(label: &str) -> Option<wgpu::ComputePassTimestampWrites<'static>> {
    let set = GROUP_SET.get()?;
    let begin = allocate(label, None)?;
    Some(wgpu::ComputePassTimestampWrites { query_set: set, beginning_of_pass_write_index: Some(begin), end_of_pass_write_index: Some(begin + 1) })
}

/// A debug group that `--gpu-passes` also times (inside a pass:
/// `pass.timed_push("models")` ... `pass.timed_pop()`).
pub trait Timed {
    fn timed_push(&mut self, name: &str);
    fn timed_pop(&mut self);
}

macro_rules! timed_pass {
    ($pass:ty) => {
        impl Timed for $pass {
            fn timed_push(&mut self, name: &str) {
                self.push_debug_group(name);
                let Some(set) = GROUP_SET.get() else { return };
                let inside = RECORDER.with_borrow(|r| r.as_ref().is_some_and(|r| r.inside));
                let depth = RECORDER.with_borrow(|r| r.as_ref().map(|r| r.stack.len() as u8 + 1));
                let begin = if inside { allocate(name, depth) } else { None };
                if let Some(b) = begin {
                    self.write_timestamp(set, b);
                }
                RECORDER.with_borrow_mut(|r| {
                    if let Some(r) = r.as_mut() {
                        let entry = begin.map(|_| r.entries.len() - 1);
                        r.stack.push(entry);
                    }
                });
            }

            fn timed_pop(&mut self) {
                let begin = RECORDER.with_borrow_mut(|r| r.as_mut().and_then(|r| r.stack.pop().flatten().map(|e| r.entries[e].begin)));
                if let (Some(b), Some(set)) = (begin, GROUP_SET.get()) {
                    self.write_timestamp(set, b + 1);
                }
                self.pop_debug_group();
            }
        }
    };
}
timed_pass!(wgpu::RenderPass<'_>);
timed_pass!(wgpu::ComputePass<'_>);

/// Mean GPU ms per pass and group over the timed frames since the last
/// report (`--gpu-passes`).
#[derive(Default)]
struct PassStats {
    /// (depth, name) in first-seen order, with total ms and count.
    rows: Vec<(u8, String, f64, u32)>,
    frames: u32,
    frame_ms: f64,
}
/// Frames whose readbacks may be in flight at once.
const FRAMES: usize = 4;

struct Slot {
    /// The zones' times, then (`--gpu-passes`) the passes' and groups'.
    readback: wgpu::Buffer,
    /// `--gpu-passes`: this frame's passes and groups, and queries used.
    entries: Vec<Entry>,
    used: u32,
    /// Spans recorded into this slot (zone i = queries 2i, 2i+1).
    #[cfg(feature = "tracy")]
    spans: Vec<Option<tracy_client::GpuSpan>>,
    zones: u32,
    mapped: std::sync::Arc<std::sync::atomic::AtomicBool>,
    in_flight: bool,
}

pub struct GpuProfiler {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: Vec<Slot>,
    /// This frame's slot, if it is being timed.
    current: Option<usize>,
    frame: usize,
    #[cfg(feature = "tracy")]
    context: Option<tracy_client::GpuContext>,
    /// Nanoseconds per timestamp tick.
    period: f32,
    /// The latest timed frame's GPU time (ms), 0 until one is read back.
    pub frame_ms: f32,
    /// The brackets' 1x1 target.
    bracket_target: wgpu::TextureView,
    /// `--gpu-passes`: where the pass and group queries resolve, whether
    /// groups inside passes are timed, and the running means.
    groups: Option<(wgpu::Buffer, bool)>,
    stats: PassStats,
    /// The frame's size in pixels (for the `--gpu-passes` report).
    pub size: (u32, u32),
}

/// The features to request for GPU timing (`passes`: `--gpu-passes`, also
/// timestamps inside passes where the adapter has them).
pub fn features(adapter: &wgpu::Adapter, passes: bool) -> wgpu::Features {
    let inside = if passes { wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES } else { wgpu::Features::empty() };
    adapter.features() & (wgpu::Features::TIMESTAMP_QUERY | inside)
}

impl GpuProfiler {
    /// `None` unless the device has `TIMESTAMP_QUERY`. `passes`: time every
    /// pass and group (`--gpu-passes`).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, passes: bool) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let groups = passes.then(|| {
            let _ = GROUP_SET.set(device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("GPU timing: passes"), ty: wgpu::QueryType::Timestamp, count: GROUP_QUERIES * FRAMES as u32 }));
            let resolve = device.create_buffer(&wgpu::BufferDescriptor { label: Some("GPU timing: passes resolve"), size: GROUP_QUERIES as u64 * 8 * FRAMES as u64, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
            let inside = device.features().contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES);
            tracing::info!(inside_passes = inside, "GPU timing per pass (--gpu-passes): a report every {REPORT_FRAMES} timed frames");
            (resolve, inside)
        });
        let readback_size = MAX_ZONES as u64 * 16 + if passes { GROUP_QUERIES as u64 * 8 } else { 0 };
        let count = MAX_ZONES * 2 * FRAMES as u32;
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("GPU timing: queries"), ty: wgpu::QueryType::Timestamp, count });
        // Each slot resolves at its own 256-byte-aligned offset.
        let size = FRAMES as u64 * wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT;
        let resolve = device.create_buffer(&wgpu::BufferDescriptor { label: Some("GPU timing: resolve"), size, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let slots = (0..FRAMES)
            .map(|_| Slot {
                readback: device.create_buffer(&wgpu::BufferDescriptor { label: Some("GPU timing: readback"), size: readback_size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }),
                entries: Vec::new(),
                used: 0,
                #[cfg(feature = "tracy")]
                spans: Vec::new(),
                zones: 0,
                mapped: Default::default(),
                in_flight: false,
            })
            .collect();
        Some(Self {
            queries,
            resolve,
            slots,
            current: None,
            frame: 0,
            #[cfg(feature = "tracy")]
            context: None,
            period: queue.get_timestamp_period(),
            frame_ms: 0.0,
            bracket_target: device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("GPU timing: bracket target"),
                    size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::R8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor { label: Some("GPU timing: bracket target"), ..Default::default() }),
            groups,
            stats: PassStats::default(),
            size: (0, 0),
        })
    }

    /// Starts timing this frame (skipped while its slot is still in flight).
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        let i = self.frame % FRAMES;
        self.current = (!self.slots[i].in_flight).then_some(i);
        if let Some(i) = self.current {
            self.slots[i].zones = 0;
            #[cfg(feature = "tracy")]
            self.slots[i].spans.clear();
        }
        // The pass recorder for this frame (none while its slot is busy).
        let recorder = match (self.current, &self.groups) {
            (Some(i), Some((_, inside))) => Some(Recorder { base: i as u32 * GROUP_QUERIES, used: 0, inside: *inside, stack: Vec::new(), entries: Vec::new() }),
            _ => None,
        };
        RECORDER.with_borrow_mut(|r| *r = recorder);
    }

    /// The frame's start: a bracket pass, before any other
    /// (zone 0; zone 1 is kept for [`Self::end_bracket`]).
    pub fn start_bracket(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(i) = self.current else { return };
        if self.slots[i].zones != 0 {
            return;
        }
        self.slots[i].zones = 2;
        #[cfg(feature = "tracy")]
        self.slots[i].spans.extend([None, None]);
        self.empty_pass(encoder, i, 0);
    }

    /// The frame's end: a bracket pass after every other.
    pub fn end_bracket(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(i) = self.current.filter(|&i| self.slots[i].zones >= 2) else { return };
        self.empty_pass(encoder, i, 1);
    }

    fn empty_pass(&self, encoder: &mut wgpu::CommandEncoder, slot: usize, zone: u32) {
        let base = (slot as u32 * MAX_ZONES + zone) * 2;
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(if zone == 0 { "GPU timing: frame start" } else { "GPU timing: frame end" }),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.bracket_target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: Some(wgpu::RenderPassTimestampWrites { query_set: &self.queries, beginning_of_pass_write_index: Some(base), end_of_pass_write_index: Some(base + 1) }),
            occlusion_query_set: None,
            multiview_mask: None,
        }));
    }

    /// Opens a zone for one pass (after the start bracket); pass the result
    /// to [`Self::writes`].
    pub fn zone(&mut self, name: ZoneName) -> Option<u32> {
        // (`--gpu-passes` times these passes itself.)
        if self.groups.is_some() {
            return None;
        }
        let i = self.current?;
        let slot = &mut self.slots[i];
        if slot.zones < 2 || slot.zones == MAX_ZONES {
            return None;
        }
        let z = slot.zones;
        slot.zones += 1;
        #[cfg(feature = "tracy")]
        slot.spans.push(self.context.as_ref().filter(|_| tracy_client::Client::running().is_some()).and_then(|c| c.span(name).ok()));
        #[cfg(not(feature = "tracy"))]
        let _ = name;
        Some(z)
    }

    pub fn writes(&self, zone: Option<u32>) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let (i, z) = (self.current?, zone?);
        let base = (i as u32 * MAX_ZONES + z) * 2;
        Some(wgpu::RenderPassTimestampWrites { query_set: &self.queries, beginning_of_pass_write_index: Some(base), end_of_pass_write_index: Some(base + 1) })
    }

    /// After the timed passes: resolve this frame's queries for readback.
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let recorded = RECORDER.with_borrow_mut(|r| r.take());
        let Some(i) = self.current else { return };
        if let (Some(r), Some((resolve, _)), Some(set)) = (recorded, &self.groups, GROUP_SET.get()) {
            let slot = &mut self.slots[i];
            slot.used = r.used;
            slot.entries = r.entries;
            if r.used > 0 {
                let offset = r.base as u64 * 8;
                encoder.resolve_query_set(set, r.base..r.base + r.used, resolve, offset);
                encoder.copy_buffer_to_buffer(resolve, offset, &slot.readback, MAX_ZONES as u64 * 16, r.used as u64 * 8);
            }
        }
        let slot = &mut self.slots[i];
        if slot.zones < 2 {
            self.current = None;
            return;
        }
        let first = i as u32 * MAX_ZONES * 2;
        let offset = i as u64 * wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT;
        encoder.resolve_query_set(&self.queries, first..first + slot.zones * 2, &self.resolve, offset);
        encoder.copy_buffer_to_buffer(&self.resolve, offset, &slot.readback, 0, slot.zones as u64 * 16);
        #[cfg(feature = "tracy")]
        for s in slot.spans.iter_mut().flatten() {
            s.end_zone();
        }
    }

    /// After submit: start mapping this frame's readback.
    pub fn after_submit(&mut self) {
        let Some(i) = self.current.take() else { return };
        let slot = &mut self.slots[i];
        slot.in_flight = true;
        slot.mapped.store(false, std::sync::atomic::Ordering::Release);
        let flag = slot.mapped.clone();
        slot.readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            if r.is_ok() {
                flag.store(true, std::sync::atomic::Ordering::Release);
            }
        });
    }

    /// Reads finished readbacks (never waits on the GPU): the frame's GPU
    /// time, and the zones for Tracy.
    pub fn collect(&mut self, device: &wgpu::Device) {
        let _s = rts_core_engine::prof_span!("gpu zones");
        let _ = device.poll(wgpu::PollType::Poll);
        // Oldest first, so Tracy gets timestamps in increasing order.
        for k in 1..=FRAMES {
            let i = (self.frame + k) % FRAMES;
            let slot = &mut self.slots[i];
            if !slot.in_flight || !slot.mapped.load(std::sync::atomic::Ordering::Acquire) {
                continue;
            }
            let (times, passes): (Vec<u64>, Vec<u64>) = {
                let data = slot.readback.slice(..).get_mapped_range();
                let all = bytemuck::cast_slice::<u8, u64>(&data);
                let zones = slot.zones as usize * 2;
                let first = MAX_ZONES as usize * 2;
                (all[..zones].to_vec(), all[first..(first + slot.used as usize).min(all.len())].to_vec())
            };
            slot.readback.unmap();
            slot.in_flight = false;
            if self.groups.is_some() {
                let entries = std::mem::take(&mut slot.entries);
                let base = (i as u32 * GROUP_QUERIES) as usize;
                let period = self.period as f64;
                let stats = &mut self.stats;
                for e in &entries {
                    let k = e.begin as usize - base;
                    if let (Some(&t0), Some(&t1)) = (passes.get(k), passes.get(k + 1)) {
                        let ms = t1.saturating_sub(t0) as f64 * period / 1e6;
                        match stats.rows.iter_mut().find(|r| r.0 == e.depth && r.1 == e.name) {
                            Some(r) => {
                                r.2 += ms;
                                r.3 += 1;
                            }
                            None => stats.rows.push((e.depth, e.name.clone(), ms, 1)),
                        }
                    }
                }
                if let (Some(&t0), Some(&t1)) = (times.first(), times.get(3)) {
                    stats.frame_ms += t1.saturating_sub(t0) as f64 * period / 1e6;
                }
                stats.frames += 1;
                if stats.frames >= REPORT_FRAMES {
                    let n = stats.frames as f64;
                    let mut report = format!("GPU ms per frame over {} frames at {}x{}: frame {:.3}", stats.frames, self.size.0, self.size.1, stats.frame_ms / n);
                    for (depth, name, total, count) in &stats.rows {
                        report.push_str(&format!("\n  {:indent$}{:7.3}  {name}{}", "", total / n, if *count as f64 > n { format!(" (x{:.1})", *count as f64 / n) } else { String::new() }, indent = *depth as usize * 2));
                    }
                    tracing::info!(target: "gpu_passes", "{report}");
                    *stats = PassStats::default();
                }
            }
            // Start bracket's begin to end bracket's end.
            if let (Some(&t0), Some(&t1)) = (times.first(), times.get(3))
                && t1 > t0 {
                self.frame_ms = ((t1 - t0) as f64 * self.period as f64 / 1e6) as f32;
            }
            #[cfg(feature = "tracy")]
            {
                if self.context.is_none() {
                    // The first frame calibrates the GPU clock.
                    if let (Some(c), Some(&t0)) = (tracy_client::Client::running(), times.first()) {
                        self.context = c.new_gpu_context(Some("wgpu"), tracy_client::GpuContextType::Invalid, t0 as i64, self.period).ok();
                    }
                }
                for (z, span) in slot.spans.drain(..).enumerate() {
                    if let Some(span) = span {
                        span.upload_timestamp_start(times[z * 2] as i64);
                        span.upload_timestamp_end(times[z * 2 + 1] as i64);
                    }
                }
            }
        }
    }
}
