//! The XACT runtime and mixer: cue instances, categories, RPC curves, 3D
//! attenuation / panning, voice queue and ducking, mixed to stereo f32.
//!
//! Synchronous and single-threaded: the audio thread (`audio::output`)
//! feeds it [`Command`]s and pulls blocks with [`AudioEngine::render`];
//! tests and `--audio-dump` drive the same calls directly. Nothing here is
//! visible to the sim (audio is presentation only).

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use super::xact::xgs::{GlobalSettings, RpcParameter};
use super::xact::xsb::{PlayEvent, SoundBank};
use super::xact::xwb::{self, BankBytes, WaveBank, WaveEntry, WaveFormat};
use super::xact::db_to_gain;
use crate::engine::vfs::Vfs;

/// Frames mixed per inner block: RPCs, 3D and gains are evaluated once per
/// block and ramped across it.
pub const BLOCK: usize = 256;

/// Where banks come from.
#[derive(Clone)]
pub enum Source {
    /// The game's file system (`/sounds`; folder-mounted banks are mapped).
    Vfs(Arc<RwLock<Vfs>>),
    /// A folder standing for `/sounds` (tests, tools).
    Dir(PathBuf),
    /// In-memory files by `/sounds/...` path (tests).
    Memory(Arc<HashMap<String, BankBytes>>),
}

impl Source {
    fn open(&self, path: &str) -> Option<BankBytes> {
        match self {
            Source::Vfs(vfs) => {
                let vfs = vfs.read().ok()?;
                if let Some(real) = vfs.real_path(path) {
                    return map_file(&real);
                }
                vfs.read(path).ok().map(|b| Arc::new(b) as BankBytes)
            }
            Source::Dir(dir) => {
                let rel = path.trim_start_matches('/').strip_prefix("sounds/").unwrap_or(path);
                find_case_insensitive(dir, rel).and_then(|p| map_file(&p))
            }
            Source::Memory(files) => files.get(&path.to_ascii_lowercase()).cloned(),
        }
    }
}

fn map_file(path: &Path) -> Option<BankBytes> {
    let file = std::fs::File::open(path).ok()?;
    // Safety: the game's bank files aren't modified while it runs.
    let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    Some(Arc::new(map))
}

fn find_case_insensitive(dir: &Path, rel: &str) -> Option<PathBuf> {
    let mut p = dir.to_path_buf();
    for part in rel.split('/').filter(|s| !s.is_empty()) {
        let direct = p.join(part);
        if direct.exists() {
            p = direct;
            continue;
        }
        let found = std::fs::read_dir(&p).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
        p = found.path();
    }
    p.is_file().then_some(p)
}

/// What a Lua handle can see of its cue (shared with the user layer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueStatus {
    /// Asked for, not yet loaded (or a queued voice).
    Pending,
    /// Loaded, waiting for `StartSound`.
    Prepared,
    Playing,
    Done,
}

/// Cue states by handle id, written by the audio thread, read by Lua.
#[derive(Default)]
pub struct StatusBoard(Mutex<HashMap<u64, CueStatus>>);

impl StatusBoard {
    pub fn insert(&self, id: u64, s: CueStatus) {
        if let Ok(mut m) = self.0.lock() {
            m.insert(id, s);
        }
    }
    /// Updates an id still tracked (a dropped handle stays forgotten).
    fn update(&self, id: u64, s: CueStatus) {
        if let Ok(mut m) = self.0.lock()
            && let Some(v) = m.get_mut(&id) {
            *v = s;
        }
    }
    pub fn get(&self, id: u64) -> Option<CueStatus> {
        self.0.lock().ok().and_then(|m| m.get(&id).copied())
    }
    pub fn forget(&self, id: u64) {
        if let Ok(mut m) = self.0.lock() {
            m.remove(&id);
        }
    }
    pub fn len(&self) -> usize {
        self.0.lock().map_or(0, |m| m.len())
    }
}

/// The camera, for 3D sounds (render-side floats).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub position: [f32; 3],
    /// Unit view direction.
    pub forward: [f32; 3],
    /// Unit screen-right.
    pub right: [f32; 3],
    /// Eye to look-at point (XACT's `CameraDistance` variable).
    pub camera_distance: f32,
}

impl Default for Listener {
    fn default() -> Self {
        Self { position: [0.0; 3], forward: [0.0, -1.0, 0.0], right: [1.0, 0.0, 0.0], camera_distance: 0.0 }
    }
}

/// A one-shot world sound from the sim (`Entity:PlaySound`).
#[derive(Clone, Debug, PartialEq)]
pub struct WorldSound {
    pub bank: Arc<str>,
    pub cue: Arc<str>,
    pub position: Option<[f32; 3]>,
    /// Name of the variable holding the camera distance past which it
    /// isn't played (`Sound{LodCutoff = ...}`).
    pub lod_cutoff: Option<Arc<str>>,
}

/// A looping sound on an entity (`Entity:SetAmbientSound`).
#[derive(Clone, Debug, PartialEq)]
pub struct Ambient {
    pub entity: u32,
    pub bank: Arc<str>,
    pub cue: Arc<str>,
    pub position: [f32; 3],
}

#[derive(Clone, Debug)]
pub enum Command {
    SetSource(Source),
    SetLanguage(String),
    /// `PlaySound` / `PlayVoice`: `start` false prepares only (`StartSound`
    /// later). Voices queue behind the one playing unless only prepared.
    Play { id: u64, bank: String, cue: String, start: bool, voice: bool, duck: bool },
    Start(u64),
    Stop { id: u64, immediate: bool },
    Pause { category: String, paused: bool },
    SetVolume { category: String, volume: f32 },
    StopAll,
    Listener(Listener),
    World(Vec<WorldSound>),
    /// Every ambient loop now (sorted by entity); others stop.
    Ambients(Vec<Ambient>),
    WorldEnabled(bool),
    Shutdown,
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Source::Vfs(_) => "Source::Vfs",
            Source::Dir(_) => "Source::Dir",
            Source::Memory(_) => "Source::Memory",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Prepared,
    Playing,
    /// Stopping: loops break; ends when the release curve goes silent.
    Releasing,
}

struct Cursor {
    bank: Arc<WaveBank>,
    entry: WaveEntry,
    /// Source frames.
    pos: f64,
    /// Source frames per output frame at no pitch change.
    step: f64,
    choice: usize,
}

struct TrackState {
    delay: u32,
    loops: u32,
    cursor: Option<Cursor>,
    volume_db: f32,
    pitch_cents: f32,
    /// Gains applied at the end of the last block (ramped from).
    gains: [f32; 2],
    started: bool,
}

struct Instance {
    id: u64,
    bank: Arc<SoundBank>,
    cue: usize,
    sound: usize,
    category: u16,
    state: State,
    age_ms: f32,
    release_ms: f32,
    /// Releasing: silent (done) once `release_ms` reaches this.
    release_end_ms: Option<f32>,
    /// A linear fade-out of this many ms (instance limits, fade-out cues).
    linear_fade_ms: Option<f32>,
    fade_in_ms: f32,
    emitter: Option<[f32; 3]>,
    voice: bool,
    duck: bool,
    tracks: Vec<TrackState>,
    started_order: u64,
}

#[derive(Default)]
struct VarIndex {
    distance: Option<u16>,
    angle: Option<u16>,
    attack: Option<u16>,
    release: Option<u16>,
    duck: Option<u16>,
    duck_length: Option<u16>,
    camera_distance: Option<u16>,
}

struct QueuedVoice {
    id: u64,
    bank: String,
    cue: String,
    duck: bool,
}

pub struct AudioEngine {
    rate: u32,
    source: Option<Source>,
    language: String,
    settings: Arc<GlobalSettings>,
    vars: VarIndex,
    globals: Vec<f32>,
    sound_banks: HashMap<String, Option<Arc<SoundBank>>>,
    wave_banks: HashMap<String, Option<Arc<WaveBank>>>,
    /// Where each loaded sound bank lives (its wave banks are looked up
    /// beside it first).
    bank_dirs: HashMap<String, String>,
    instances: Vec<Instance>,
    volumes: BTreeMap<String, f32>,
    paused: BTreeSet<String>,
    cat_gain: Vec<f32>,
    cat_paused: Vec<bool>,
    /// The `VO` volume (SupCom.xgs has no VO category; `Global` still
    /// applies through the voice cue's own category).
    voice_gain: f32,
    voices_paused: bool,
    listener: Listener,
    voice_queue: VecDeque<QueuedVoice>,
    duck: f32,
    world_enabled: bool,
    ambients: BTreeMap<u32, (u64, Arc<str>, Arc<str>)>,
    status: Arc<StatusBoard>,
    rng: u64,
    next_internal: u64,
    order: u64,
    /// Last wave chosen per (bank, sound, track) for variations.
    last_choice: HashMap<(usize, usize, usize), usize>,
    warned: BTreeSet<String>,
    limiter: f32,
    mix: Vec<f32>,
}

/// Ids at or above this are the engine's own (world sounds, ambients);
/// Lua handles count up from 1.
const INTERNAL_IDS: u64 = 1 << 62;

impl AudioEngine {
    pub fn new(rate: u32, status: Arc<StatusBoard>) -> Self {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0x9E37_79B9, |d| d.as_nanos() as u64);
        Self {
            rate: rate.max(8000),
            source: None,
            language: "us".into(),
            settings: Arc::new(GlobalSettings::default()),
            vars: VarIndex::default(),
            globals: Vec::new(),
            sound_banks: HashMap::new(),
            wave_banks: HashMap::new(),
            bank_dirs: HashMap::new(),
            instances: Vec::new(),
            volumes: BTreeMap::new(),
            paused: BTreeSet::new(),
            cat_gain: Vec::new(),
            cat_paused: Vec::new(),
            voice_gain: 1.0,
            voices_paused: false,
            listener: Listener::default(),
            voice_queue: VecDeque::new(),
            duck: 0.0,
            world_enabled: true,
            ambients: BTreeMap::new(),
            status,
            rng: seed | 1,
            next_internal: INTERNAL_IDS,
            order: 0,
            last_choice: HashMap::new(),
            warned: BTreeSet::new(),
            limiter: 1.0,
            mix: vec![0.0; BLOCK * 2],
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Fixed variation choices (tests).
    pub fn seed(&mut self, seed: u64) {
        self.rng = seed | 1;
    }

    /// Cues playing or prepared.
    pub fn active(&self) -> usize {
        self.instances.len()
    }

    pub fn settings(&self) -> &GlobalSettings {
        &self.settings
    }

    fn random(&mut self) -> u64 {
        // xorshift64*: presentation only, never the sim's.
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn random_f32(&mut self) -> f32 {
        (self.random() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn warn_once(&mut self, key: String, msg: impl FnOnce() -> String) {
        if self.warned.insert(key) {
            tracing::warn!("audio: {}", msg());
        }
    }

    pub fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::SetSource(s) => self.set_source(s),
            Command::SetLanguage(l) => self.language = l.to_ascii_lowercase(),
            Command::Play { id, bank, cue, start, voice, duck } => {
                if voice && start && self.voice_busy() {
                    self.status.update(id, CueStatus::Pending);
                    self.voice_queue.push_back(QueuedVoice { id, bank, cue, duck });
                } else {
                    self.play(id, &bank, &cue, start, voice, duck, None);
                }
            }
            Command::Start(id) => {
                self.start(id);
            }
            Command::Stop { id, immediate } => self.stop(id, immediate),
            Command::Pause { category, paused } => {
                let key = category.to_ascii_lowercase();
                if paused {
                    self.paused.insert(key);
                } else {
                    self.paused.remove(&key);
                }
                self.refresh_categories();
            }
            Command::SetVolume { category, volume } => {
                self.volumes.insert(category.to_ascii_lowercase(), volume.clamp(0.0, 2.0));
                self.refresh_categories();
            }
            Command::StopAll => {
                for i in std::mem::take(&mut self.instances) {
                    self.status.update(i.id, CueStatus::Done);
                }
                for v in std::mem::take(&mut self.voice_queue) {
                    self.status.update(v.id, CueStatus::Done);
                }
                self.ambients.clear();
            }
            Command::Listener(l) => {
                self.listener = l;
                if let Some(v) = self.vars.camera_distance {
                    self.globals[v as usize] = l.camera_distance;
                }
            }
            Command::World(sounds) => {
                if self.world_enabled {
                    for s in sounds {
                        self.world_sound(s);
                    }
                }
            }
            Command::Ambients(list) => self.set_ambients(list),
            Command::WorldEnabled(on) => {
                self.world_enabled = on;
                if !on {
                    self.set_ambients(Vec::new());
                }
            }
            Command::Shutdown => {}
        }
    }

    fn set_source(&mut self, source: Source) {
        let xgs = ["/sounds/supcom.xgs"].into_iter().find_map(|p| source.open(p));
        match xgs.map(|b| GlobalSettings::parse((*b).as_ref())) {
            Some(Ok(s)) => {
                tracing::info!(categories = s.categories.len(), variables = s.variables.len(), rpcs = s.rpcs.len(), "audio: global settings loaded");
                self.settings = Arc::new(s);
            }
            Some(Err(e)) => tracing::warn!("audio: SupCom.xgs: {e}"),
            None => tracing::warn!("audio: no /sounds/SupCom.xgs; categories and 3D curves are off"),
        }
        let s = &self.settings;
        self.vars = VarIndex {
            distance: s.variable("Distance"),
            angle: s.variable("Angle"),
            attack: s.variable("AttackTime"),
            release: s.variable("ReleaseTime"),
            duck: s.variable("Duck"),
            duck_length: s.variable("DuckLength"),
            camera_distance: s.variable("CameraDistance"),
        };
        self.globals = s.variables.iter().map(|v| v.initial).collect();
        self.source = Some(source);
        self.sound_banks.clear();
        self.wave_banks.clear();
        self.refresh_categories();
    }

    /// Per-category gain (the category's authored volume times its runtime
    /// volume, down the parent chain) and pause.
    fn refresh_categories(&mut self) {
        let s = self.settings.clone();
        self.cat_gain = (0..s.categories.len())
            .map(|i| {
                let mut g = 1.0;
                let mut c = Some(i as u16);
                let mut guard = 0;
                while let Some(ci) = c {
                    let cat = &s.categories[ci as usize];
                    g *= db_to_gain(cat.volume_db) * self.volumes.get(&cat.name.to_ascii_lowercase()).copied().unwrap_or(1.0);
                    c = cat.parent;
                    guard += 1;
                    if guard > 64 {
                        break;
                    }
                }
                g
            })
            .collect();
        self.voice_gain = self.volumes.get("vo").copied().unwrap_or(1.0);
        self.voices_paused = self.paused.contains("vo");
        self.cat_paused = (0..s.categories.len())
            .map(|i| self.paused.iter().any(|p| s.category(p).is_some_and(|pc| s.is_within(i as u16, pc))))
            .collect();
    }

    fn sound_bank(&mut self, name: &str) -> Option<Arc<SoundBank>> {
        let key = name.to_ascii_lowercase();
        if let Some(b) = self.sound_banks.get(&key) {
            return b.clone();
        }
        let source = self.source.clone()?;
        let candidates = [format!("/sounds/voice/{}/{key}.xsb", self.language), format!("/sounds/voice/us/{key}.xsb"), format!("/sounds/{key}.xsb")];
        let mut found = None;
        for path in candidates {
            if let Some(bytes) = source.open(&path) {
                match SoundBank::parse((*bytes).as_ref()) {
                    Ok(b) => {
                        let dir = path.rsplit_once('/').map_or("/sounds".to_string(), |(d, _)| d.to_string());
                        self.bank_dirs.insert(key.clone(), dir);
                        found = Some(Arc::new(b));
                    }
                    Err(e) => tracing::warn!("audio: {path}: {e}"),
                }
                break;
            }
        }
        if found.is_none() {
            tracing::warn!("audio: no sound bank {name}");
        }
        self.sound_banks.insert(key, found.clone());
        found
    }

    fn wave_bank(&mut self, sound_bank: &str, name: &str) -> Option<Arc<WaveBank>> {
        let key = name.to_ascii_lowercase();
        if let Some(b) = self.wave_banks.get(&key) {
            return b.clone();
        }
        let source = self.source.clone()?;
        let dir = self.bank_dirs.get(&sound_bank.to_ascii_lowercase()).cloned().unwrap_or_else(|| "/sounds".into());
        let candidates = [format!("{dir}/{key}.xwb"), format!("/sounds/{key}.xwb"), format!("/sounds/voice/{}/{key}.xwb", self.language), format!("/sounds/voice/us/{key}.xwb")];
        let mut found = None;
        for path in candidates {
            if let Some(bytes) = source.open(&path) {
                match WaveBank::parse(bytes) {
                    Ok(b) => {
                        tracing::debug!(bank = %b.name, waves = b.entries.len(), streaming = b.streaming, "audio: wave bank loaded");
                        found = Some(Arc::new(b));
                    }
                    Err(e) => tracing::warn!("audio: {path}: {e}"),
                }
                break;
            }
        }
        if found.is_none() {
            tracing::warn!("audio: no wave bank {name} (for {sound_bank})");
        }
        self.wave_banks.insert(key, found.clone());
        found
    }

    fn voice_busy(&self) -> bool {
        self.instances.iter().any(|i| i.voice && i.state != State::Prepared)
    }

    /// Creates an instance (prepared, or started); `false` if nothing plays.
    #[allow(clippy::too_many_arguments)]
    fn play(&mut self, id: u64, bank: &str, cue: &str, start: bool, voice: bool, duck: bool, emitter: Option<[f32; 3]>) -> bool {
        let Some(sb) = self.sound_bank(bank) else {
            self.status.update(id, CueStatus::Done);
            return false;
        };
        let Some(ci) = sb.cue(cue) else {
            self.warn_once(format!("{bank}/{cue}"), || format!("no cue {cue} in bank {bank}"));
            self.status.update(id, CueStatus::Done);
            return false;
        };
        let Some(si) = sb.cues[ci].sound else {
            self.status.update(id, CueStatus::Done);
            return false;
        };
        let sound = &sb.sounds[si];
        let mut tracks = Vec::with_capacity(sound.tracks.len());
        for (ti, t) in sound.tracks.iter().enumerate() {
            let Some(event) = &t.event else {
                tracks.push(TrackState { delay: 0, loops: 0, cursor: None, volume_db: t.volume_db, pitch_cents: 0.0, gains: [0.0; 2], started: true });
                continue;
            };
            let pitch = match event.pitch_variation {
                Some((a, b)) => a as f32 + (b - a) as f32 * self.random_f32(),
                None => 0.0,
            };
            let vol = match event.volume_variation {
                Some((a, b)) => a + (b - a) * self.random_f32(),
                None => 0.0,
            };
            let random_ms = if event.random_offset_ms > 0 { (self.random() % (u64::from(event.random_offset_ms) + 1)) as u32 } else { 0 };
            let delay = ((u32::from(event.timestamp_ms) + random_ms) as u64 * u64::from(self.rate) / 1000) as u32;
            let choice = self.choose(&sb, si, ti, event);
            let cursor = self.cursor(&sb, bank, event, choice);
            tracks.push(TrackState { delay, loops: 0, cursor, volume_db: t.volume_db + vol, pitch_cents: pitch, gains: [0.0; 2], started: false });
        }
        if tracks.iter().all(|t| t.cursor.is_none()) {
            self.status.update(id, CueStatus::Done);
            return false;
        }
        self.order += 1;
        let cue_info = &sb.cues[ci];
        let inst = Instance {
            id,
            bank: sb.clone(),
            cue: ci,
            sound: si,
            category: sound.category,
            state: State::Prepared,
            age_ms: 0.0,
            release_ms: 0.0,
            release_end_ms: None,
            linear_fade_ms: None,
            fade_in_ms: f32::from(cue_info.fade_in_ms),
            emitter,
            voice,
            duck,
            tracks,
            started_order: self.order,
        };
        self.instances.push(inst);
        if start {
            self.start(id)
        } else {
            self.status.update(id, CueStatus::Prepared);
            true
        }
    }

    /// The wave a track's event plays (variations by their type).
    fn choose(&mut self, sb: &Arc<SoundBank>, sound: usize, track: usize, event: &PlayEvent) -> usize {
        let n = event.waves.len();
        if n <= 1 {
            return 0;
        }
        let key = (Arc::as_ptr(sb) as usize, sound, track);
        let last = self.last_choice.get(&key).copied();
        let pick = match event.variation & 0xF {
            0 | 1 => last.map_or(0, |l| (l + 1) % n),
            3 | 4 => {
                // Random without an immediate repeat (shuffle approximated
                // the same way).
                let r = (self.random() % (n as u64 - 1)) as usize;
                match last {
                    Some(l) if r >= l => r + 1,
                    _ => r,
                }
            }
            _ => (self.random() % n as u64) as usize,
        };
        self.last_choice.insert(key, pick);
        pick
    }

    fn cursor(&mut self, sb: &Arc<SoundBank>, bank: &str, event: &PlayEvent, choice: usize) -> Option<Cursor> {
        let w = event.waves.get(choice)?;
        let wb_name = sb.wave_banks.get(w.bank as usize)?.clone();
        let wb = self.wave_bank(bank, &wb_name)?;
        let Some(entry) = wb.entries.get(w.index as usize).copied() else {
            self.warn_once(format!("{wb_name}#{}", w.index), || format!("wave bank {wb_name} has no wave {}", w.index));
            return None;
        };
        if entry.format != WaveFormat::Pcm {
            self.warn_once(format!("{wb_name} format"), || format!("wave bank {wb_name}: {:?} waves aren't decoded (silent)", entry.format));
            return None;
        }
        Some(Cursor { step: f64::from(entry.sample_rate) / f64::from(self.rate), bank: wb, entry, pos: 0.0, choice })
    }

    /// Starts a prepared cue; whether it now plays.
    fn start(&mut self, id: u64) -> bool {
        let Some(idx) = self.instances.iter().position(|i| i.id == id) else { return false };
        if self.instances[idx].state != State::Prepared {
            return true;
        }
        if !self.admit(idx) {
            let i = self.instances.remove(idx);
            self.status.update(i.id, CueStatus::Done);
            return false;
        }
        self.order += 1;
        let order = self.order;
        let inst = &mut self.instances[idx];
        inst.state = State::Playing;
        inst.started_order = order;
        self.status.update(id, CueStatus::Playing);
        true
    }

    /// Instance limits (the cue's, then its category's): over the limit the
    /// new one fails or the oldest gives way, as the limit's behaviour says.
    fn admit(&mut self, idx: usize) -> bool {
        let (bank, cue, category) = {
            let i = &self.instances[idx];
            (i.bank.clone(), i.cue, i.category)
        };
        let c = &bank.cues[cue];
        let same_cue = |i: &Instance| Arc::ptr_eq(&i.bank, &bank) && i.cue == cue;
        if !self.make_room(idx, c.instance_limit, c.behavior, c.fade_out_ms, same_cue) {
            return false;
        }
        let settings = self.settings.clone();
        if let Some(cat) = settings.categories.get(category as usize)
            && !self.make_room(idx, cat.instance_limit, cat.behavior, cat.fade_out_ms, |i: &Instance| i.category == category) {
            return false;
        }
        true
    }

    fn make_room(&mut self, idx: usize, limit: u8, behavior: u8, fade_ms: u16, same: impl Fn(&Instance) -> bool) -> bool {
        if limit == 255 {
            return true;
        }
        let live: Vec<usize> = (0..self.instances.len()).filter(|&j| j != idx && self.instances[j].state == State::Playing && same(&self.instances[j])).collect();
        if live.len() < usize::from(limit) {
            return true;
        }
        if behavior <= 1 {
            return false;
        }
        // Replace the oldest (quietest / lowest priority approximated so).
        let excess = live.len() + 1 - usize::from(limit);
        let mut oldest = live;
        oldest.sort_by_key(|&j| self.instances[j].started_order);
        for &j in oldest.iter().take(excess) {
            let inst = &mut self.instances[j];
            inst.state = State::Releasing;
            inst.release_end_ms = None;
            inst.linear_fade_ms = Some(f32::from(fade_ms).max(5.0));
            inst.release_ms = 0.0;
        }
        true
    }

    fn stop(&mut self, id: u64, immediate: bool) {
        if let Some(q) = self.voice_queue.iter().position(|v| v.id == id) {
            self.voice_queue.remove(q);
            self.status.update(id, CueStatus::Done);
            return;
        }
        let Some(idx) = self.instances.iter().position(|i| i.id == id) else { return };
        if immediate || self.instances[idx].state == State::Prepared {
            let i = self.instances.remove(idx);
            self.status.update(i.id, CueStatus::Done);
            self.next_voice();
            return;
        }
        self.release(idx);
    }

    /// A stop with release: loops end, the release curve (`ReleaseTime`
    /// RPCs) or the cue's fade-out plays; neither: it stops now.
    fn release(&mut self, idx: usize) {
        let settings = self.settings.clone();
        let inst = &mut self.instances[idx];
        if inst.state == State::Releasing {
            return;
        }
        let sound = &inst.bank.sounds[inst.sound];
        let release_var = self.vars.release;
        let mut end: Option<f32> = None;
        for code in sound.rpcs.iter().chain(sound.tracks.iter().flat_map(|t| t.rpcs.iter())) {
            if let Some(r) = settings.rpcs.get(code)
                && Some(r.variable) == release_var
                && let Some(x) = r.silent_from() {
                end = Some(end.map_or(x, |e: f32| e.max(x)));
            }
        }
        let fade = inst.bank.cues[inst.cue].fade_out_ms;
        inst.state = State::Releasing;
        inst.release_ms = 0.0;
        inst.release_end_ms = end;
        if end.is_none() {
            if fade > 0 {
                inst.linear_fade_ms = Some(f32::from(fade));
            } else {
                let i = self.instances.remove(idx);
                self.status.update(i.id, CueStatus::Done);
                self.next_voice();
            }
        }
    }

    fn next_voice(&mut self) {
        while !self.voice_busy() {
            let Some(v) = self.voice_queue.pop_front() else { return };
            if self.play(v.id, &v.bank, &v.cue, true, true, v.duck, None) {
                return;
            }
        }
    }

    fn world_sound(&mut self, s: WorldSound) {
        if let (Some(cut), Some(p)) = (&s.lod_cutoff, s.position)
            && let Some(v) = self.settings.variable(cut) {
            let limit = self.globals.get(v as usize).copied().unwrap_or(-1.0);
            if limit > 0.0 && dist(self.listener.position, p) > limit {
                return;
            }
        }
        let id = self.next_internal;
        self.next_internal += 1;
        self.play(id, &s.bank, &s.cue, true, false, false, s.position.or(Some(self.listener_ground())));
    }

    /// Where a world sound without a position sits: under the camera.
    fn listener_ground(&self) -> [f32; 3] {
        let l = &self.listener;
        let d = l.camera_distance;
        [l.position[0] + l.forward[0] * d, l.position[1] + l.forward[1] * d, l.position[2] + l.forward[2] * d]
    }

    fn set_ambients(&mut self, list: Vec<Ambient>) {
        let mut keep = BTreeSet::new();
        for a in list {
            keep.insert(a.entity);
            match self.ambients.get(&a.entity) {
                Some((id, bank, cue)) if *bank == a.bank && *cue == a.cue => {
                    let id = *id;
                    if let Some(i) = self.instances.iter_mut().find(|i| i.id == id) {
                        i.emitter = Some(a.position);
                    }
                }
                other => {
                    if let Some((old, _, _)) = other {
                        let old = *old;
                        self.stop(old, false);
                    }
                    let id = self.next_internal;
                    self.next_internal += 1;
                    if self.world_enabled && self.play(id, &a.bank, &a.cue, true, false, false, Some(a.position)) {
                        self.ambients.insert(a.entity, (id, a.bank, a.cue));
                    } else {
                        self.ambients.remove(&a.entity);
                    }
                }
            }
        }
        let gone: Vec<(u32, u64)> = self.ambients.iter().filter(|(e, _)| !keep.contains(e)).map(|(e, (id, _, _))| (*e, *id)).collect();
        for (e, id) in gone {
            self.ambients.remove(&e);
            self.stop(id, false);
        }
    }

    /// Mixes the next `out.len() / 2` stereo frames (interleaved L, R).
    pub fn render(&mut self, out: &mut [f32]) {
        for chunk in out.chunks_mut(BLOCK * 2) {
            self.render_block(chunk);
        }
    }

    fn render_block(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        let block_ms = frames as f32 * 1000.0 / self.rate as f32;
        out.fill(0.0);
        // Ducking: towards 1 while a ducking voice plays, over `DuckLength`.
        let ducking = self.instances.iter().any(|i| i.duck && i.state == State::Playing && !self.paused_instance(i));
        let duck_len = self.vars.duck_length.and_then(|v| self.globals.get(v as usize).copied()).unwrap_or(0.5).max(0.01);
        let step = block_ms / 1000.0 / duck_len;
        self.duck = if ducking { (self.duck + step).min(1.0) } else { (self.duck - step).max(0.0) };
        if let Some(v) = self.vars.duck {
            self.globals[v as usize] = self.duck * 0.53;
        }
        let settings = self.settings.clone();
        let mut mix = std::mem::take(&mut self.mix);
        mix.resize(frames * 2, 0.0);
        let mut finished = Vec::new();
        for n in 0..self.instances.len() {
            let paused = self.paused_instance(&self.instances[n]);
            let inst = &self.instances[n];
            if inst.state == State::Prepared || paused {
                continue;
            }
            // Variables this block.
            let (distance, angle) = match inst.emitter {
                Some(p) => {
                    let l = &self.listener;
                    let d = [p[0] - l.position[0], p[1] - l.position[1], p[2] - l.position[2]];
                    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                    let cos = if len > 1e-3 { (d[0] * l.forward[0] + d[1] * l.forward[1] + d[2] * l.forward[2]) / len } else { 1.0 };
                    (len, cos.clamp(-1.0, 1.0).acos().to_degrees())
                }
                None => (0.0, 0.0),
            };
            let var = |v: u16| -> f32 {
                if Some(v) == self.vars.attack {
                    inst.age_ms
                } else if Some(v) == self.vars.release {
                    inst.release_ms
                } else if Some(v) == self.vars.distance {
                    distance
                } else if Some(v) == self.vars.angle {
                    angle
                } else {
                    self.globals.get(v as usize).copied().unwrap_or(0.0)
                }
            };
            let rpc_sum = |codes: &[u32]| -> (f32, f32) {
                let (mut db, mut cents) = (0.0, 0.0);
                for c in codes {
                    if let Some(r) = settings.rpcs.get(c) {
                        let y = r.eval(var(r.variable));
                        match r.parameter {
                            RpcParameter::Volume => db += y / 100.0,
                            RpcParameter::Pitch => cents += y,
                            RpcParameter::Other(_) => {}
                        }
                    }
                }
                (db, cents)
            };
            let sound = &inst.bank.sounds[inst.sound];
            let (s_db, s_cents) = rpc_sum(&sound.rpcs);
            let mut gain = self.cat_gain.get(inst.category as usize).copied().unwrap_or(1.0);
            if inst.voice {
                gain *= self.voice_gain;
            }
            if let Some(total) = inst.linear_fade_ms {
                gain *= (1.0 - inst.release_ms / total).clamp(0.0, 1.0);
            }
            if inst.fade_in_ms > 0.0 {
                gain *= (inst.age_ms / inst.fade_in_ms).clamp(0.0, 1.0);
            }
            // Panning: by the emitter's side of the screen (never fully hard).
            let pan = match inst.emitter {
                Some(p) if distance > 1e-3 => {
                    let l = &self.listener;
                    let x = ((p[0] - l.position[0]) * l.right[0] + (p[1] - l.position[1]) * l.right[1] + (p[2] - l.position[2]) * l.right[2]) / distance;
                    let x = x.clamp(-1.0, 1.0) * 0.75;
                    [(1.0 - x).min(1.0), (1.0 + x).min(1.0)]
                }
                _ => [1.0, 1.0],
            };
            let track_params: Vec<(f32, f32)> = sound.tracks.iter().map(|t| rpc_sum(&t.rpcs)).collect();
            let base_db = sound.volume_db + s_db;
            let base_cents = f32::from(sound.pitch_cents) + s_cents;
            let releasing = inst.state == State::Releasing;
            let (bank, sound_idx) = (inst.bank.clone(), inst.sound);
            let inst = &mut self.instances[n];
            let mut all_done = true;
            for (ti, ts) in inst.tracks.iter_mut().enumerate() {
                let (t_db, t_cents) = track_params[ti];
                let g = db_to_gain(base_db + ts.volume_db + t_db) * gain;
                let target = [g * pan[0], g * pan[1]];
                let ratio = 2f64.powf(f64::from(base_cents + ts.pitch_cents + t_cents) / 1200.0);
                let Some(event) = bank.sounds[sound_idx].tracks[ti].event.as_ref() else { continue };
                let done = mix_track(ts, event, &mut mix[..frames * 2], target, ratio, releasing, &mut self.rng);
                all_done &= done;
            }
            inst.age_ms += block_ms;
            if releasing {
                inst.release_ms += block_ms;
            }
            let faded = inst.linear_fade_ms.is_some_and(|t| inst.release_ms >= t) || inst.release_end_ms.is_some_and(|t| inst.release_ms >= t);
            if all_done || faded {
                finished.push(inst.id);
            }
        }
        out[..frames * 2].copy_from_slice(&mix[..frames * 2]);
        mix.iter_mut().for_each(|s| *s = 0.0);
        self.mix = mix;
        if !finished.is_empty() {
            self.instances.retain(|i| {
                if finished.contains(&i.id) {
                    self.status.update(i.id, CueStatus::Done);
                    false
                } else {
                    true
                }
            });
            self.ambients.retain(|_, (id, _, _)| !finished.contains(id));
            self.next_voice();
        }
        self.limit(out, block_ms);
    }

    fn paused_cat(&self, category: u16) -> bool {
        self.cat_paused.get(category as usize).copied().unwrap_or(false)
    }

    /// Paused by its category, or a voice while `VO` is paused
    /// (`PauseVoice("VO", true)`: SupCom.xgs has no VO category, voices are
    /// the engine's own queue).
    fn paused_instance(&self, i: &Instance) -> bool {
        self.paused_cat(i.category) || (i.voice && self.voices_paused)
    }

    /// A peak limiter (instant attack, ~0.5 s release) then a hard clip, so
    /// a big battle saturates instead of wrapping.
    fn limit(&mut self, out: &mut [f32], block_ms: f32) {
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let want = if peak > 0.98 { 0.98 / peak } else { 1.0 };
        let from = self.limiter;
        let to = if want < from { want } else { (from + block_ms / 500.0).min(want) };
        let frames = out.len() / 2;
        for (f, pair) in out.chunks_exact_mut(2).enumerate() {
            let g = if want < from { to } else { from + (to - from) * (f as f32 / frames.max(1) as f32) };
            pair[0] = (pair[0] * g).clamp(-1.0, 1.0);
            pair[1] = (pair[1] * g).clamp(-1.0, 1.0);
        }
        self.limiter = to;
    }
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Mixes one track into `mix` (interleaved stereo), gains ramped from the
/// last block's. Returns whether the track has finished.
fn mix_track(ts: &mut TrackState, event: &PlayEvent, mix: &mut [f32], target: [f32; 2], ratio: f64, releasing: bool, rng: &mut u64) -> bool {
    let frames = mix.len() / 2;
    let from = if ts.started { ts.gains } else { target };
    ts.started = true;
    let mut f = 0usize;
    if ts.delay > 0 {
        let skip = (ts.delay as usize).min(frames);
        ts.delay -= skip as u32;
        f = skip;
    }
    let inv = 1.0 / frames.max(1) as f32;
    while f < frames {
        let Some(c) = ts.cursor.as_mut() else { break };
        let entry = c.entry;
        let bytes = c.bank.bytes(&entry);
        let total = entry.frames();
        let (loop_start, loop_end) = if entry.loop_length > 0 { (entry.loop_start, (entry.loop_start + entry.loop_length).min(total)) } else { (0, total) };
        let step = c.step * ratio;
        let stereo = entry.channels >= 2;
        while f < frames {
            let i = c.pos as u32;
            if i >= loop_end {
                break;
            }
            let frac = (c.pos - f64::from(i)) as f32;
            let next = if i + 1 < loop_end { i + 1 } else { i };
            let t = f as f32 * inv;
            let gl = from[0] + (target[0] - from[0]) * t;
            let gr = from[1] + (target[1] - from[1]) * t;
            let l0 = xwb::sample(bytes, &entry, i, 0);
            let l1 = xwb::sample(bytes, &entry, next, 0);
            let l = l0 + (l1 - l0) * frac;
            let r = if stereo {
                let r0 = xwb::sample(bytes, &entry, i, 1);
                let r1 = xwb::sample(bytes, &entry, next, 1);
                r0 + (r1 - r0) * frac
            } else {
                l
            };
            mix[2 * f] += l * gl;
            mix[2 * f + 1] += r * gr;
            c.pos += step;
            f += 1;
        }
        if (c.pos as u32) < loop_end {
            continue;
        }
        // The wave ended: loop, or this track is done.
        let forever = event.loop_count == 255;
        if !releasing && (forever || ts.loops < u32::from(event.loop_count)) {
            ts.loops += 1;
            if event.new_wave_per_loop() {
                // A new variation each loop (the music's playlists).
                let n = event.waves.len();
                *rng ^= *rng >> 12;
                *rng ^= *rng << 25;
                *rng ^= *rng >> 27;
                let r = (rng.wrapping_mul(0x2545_F491_4F6C_DD1D) % (n as u64 - 1).max(1)) as usize;
                let pick = if r >= c.choice { r + 1 } else { r }.min(n - 1);
                c.choice = pick;
                let w = event.waves[pick];
                if let Some(e) = c.bank.entries.get(w.index as usize).copied().filter(|e| e.format == WaveFormat::Pcm && w.bank == event.waves[0].bank) {
                    c.entry = e;
                    c.step = c.step / f64::from(entry.sample_rate) * f64::from(e.sample_rate);
                }
                c.pos = 0.0;
            } else {
                c.pos = f64::from(loop_start) + (c.pos - f64::from(loop_end));
                if c.pos < f64::from(loop_start) {
                    c.pos = f64::from(loop_start);
                }
            }
        } else {
            ts.cursor = None;
        }
    }
    ts.gains = target;
    ts.cursor.is_none() && ts.delay == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::xact::test_banks;

    fn rms(s: &[f32]) -> f32 {
        (s.iter().map(|x| x * x).sum::<f32>() / s.len().max(1) as f32).sqrt()
    }

    fn game_engine() -> Option<(AudioEngine, Arc<StatusBoard>)> {
        let dir = crate::audio::xact::tests::sounds_dir()?;
        let status = Arc::new(StatusBoard::default());
        let mut e = AudioEngine::new(48000, status.clone());
        e.seed(7);
        e.handle(Command::SetSource(Source::Dir(dir)));
        Some((e, status))
    }

    #[test]
    fn menu_music_plays_loud_and_stops_with_its_release_fade() {
        let Some((mut e, status)) = game_engine() else { return };
        status.insert(1, CueStatus::Pending);
        e.handle(Command::Play { id: 1, bank: "Music".into(), cue: "Main_Menu".into(), start: true, voice: false, duck: false });
        assert_eq!(status.get(1), Some(CueStatus::Playing));
        let mut out = vec![0.0; 48000 * 2 * 2];
        e.render(&mut out);
        let level = rms(&out[48000..]);
        assert!(level > 0.01, "menu music rms {level}");
        // Stop with release: ReleaseTime's ~6 s fade, then done.
        e.handle(Command::Stop { id: 1, immediate: false });
        let mut fade = vec![0.0; 48000 * 2];
        e.render(&mut fade);
        assert_eq!(status.get(1), Some(CueStatus::Playing));
        for _ in 0..6 {
            e.render(&mut fade);
        }
        assert_eq!(status.get(1), Some(CueStatus::Done));
        assert_eq!(e.active(), 0);
    }

    #[test]
    fn interface_click_is_short_and_finishes() {
        let Some((mut e, status)) = game_engine() else { return };
        status.insert(5, CueStatus::Pending);
        e.handle(Command::Play { id: 5, bank: "Interface".into(), cue: "UI_Menu_MouseDown_Sml".into(), start: true, voice: false, duck: false });
        let mut out = vec![0.0; 48000 * 2 * 3];
        e.render(&mut out);
        assert!(rms(&out[..9600]) > 0.005, "click rms {}", rms(&out[..9600]));
        assert_eq!(status.get(5), Some(CueStatus::Done));
    }

    #[test]
    fn prepared_cues_wait_for_start() {
        let Some((mut e, status)) = game_engine() else { return };
        status.insert(2, CueStatus::Pending);
        e.handle(Command::Play { id: 2, bank: "Interface".into(), cue: "UI_Menu_MouseDown_Sml".into(), start: false, voice: false, duck: false });
        assert_eq!(status.get(2), Some(CueStatus::Prepared));
        let mut out = vec![0.0; 9600];
        e.render(&mut out);
        assert_eq!(rms(&out), 0.0);
        e.handle(Command::Start(2));
        e.render(&mut out);
        assert!(rms(&out) > 0.0);
    }

    #[test]
    fn paused_and_muted_categories_are_silent() {
        let Some((mut e, _)) = game_engine() else { return };
        e.handle(Command::Play { id: 3, bank: "Music".into(), cue: "Main_Menu".into(), start: true, voice: false, duck: false });
        let mut out = vec![0.0; 48000];
        e.render(&mut out);
        e.handle(Command::Pause { category: "Music".into(), paused: true });
        e.render(&mut out);
        assert_eq!(rms(&out), 0.0);
        e.handle(Command::Pause { category: "Music".into(), paused: false });
        e.handle(Command::SetVolume { category: "Global".into(), volume: 0.0 });
        // One block ramps the gain down.
        e.render(&mut out[..2 * BLOCK]);
        e.render(&mut out);
        assert_eq!(rms(&out), 0.0);
        e.handle(Command::SetVolume { category: "Global".into(), volume: 1.0 });
        e.render(&mut out);
        assert!(rms(&out) > 0.0);
    }

    #[test]
    fn voices_queue_one_after_another() {
        let Some((mut e, status)) = game_engine() else { return };
        for id in [10, 11] {
            status.insert(id, CueStatus::Pending);
            e.handle(Command::Play { id, bank: "XGG".into(), cue: "XGG_Computer__04716".into(), start: true, voice: true, duck: true });
        }
        assert_eq!(status.get(10), Some(CueStatus::Playing));
        assert_eq!(status.get(11), Some(CueStatus::Pending));
        let mut out = vec![0.0; 48000 * 2];
        for _ in 0..30 {
            e.render(&mut out);
            if status.get(10) == Some(CueStatus::Done) {
                break;
            }
        }
        assert_eq!(status.get(10), Some(CueStatus::Done));
        assert_eq!(status.get(11), Some(CueStatus::Playing));
    }

    /// `PauseVoice("VO", true)` / `SetVolume("VO", 0)` silence voices (no
    /// XACT category holds them).
    #[test]
    fn vo_pause_and_volume_apply_to_voices() {
        let Some((mut e, _)) = game_engine() else { return };
        e.handle(Command::Play { id: 12, bank: "XGG".into(), cue: "XGG_Computer__04716".into(), start: true, voice: true, duck: false });
        let mut out = vec![0.0; 9600];
        e.render(&mut out);
        assert!(rms(&out) > 0.0);
        e.handle(Command::Pause { category: "VO".into(), paused: true });
        e.render(&mut out);
        assert_eq!(rms(&out), 0.0);
        e.handle(Command::Pause { category: "VO".into(), paused: false });
        e.handle(Command::SetVolume { category: "VO".into(), volume: 0.0 });
        e.render(&mut out[..2 * BLOCK]);
        e.render(&mut out);
        assert_eq!(rms(&out), 0.0);
    }

    #[test]
    fn world_sounds_attenuate_with_distance_and_pan() {
        let Some((mut e, _)) = game_engine() else { return };
        let listener = Listener { position: [0.0, 100.0, 0.0], forward: [0.0, -1.0, 0.0], right: [1.0, 0.0, 0.0], camera_distance: 100.0 };
        e.handle(Command::Listener(listener));
        let level = |e: &mut AudioEngine, at: [f32; 3]| {
            e.seed(3);
            e.handle(Command::World(vec![WorldSound { bank: "Impacts".into(), cue: "Impact_Land_Gen_UEF".into(), position: Some(at), lod_cutoff: None }]));
            let mut out = vec![0.0; 48000];
            e.render(&mut out);
            e.handle(Command::StopAll);
            let l: Vec<f32> = out.iter().step_by(2).copied().collect();
            let r: Vec<f32> = out.iter().skip(1).step_by(2).copied().collect();
            (rms(&l), rms(&r))
        };
        let near = level(&mut e, [0.0, 0.0, 0.0]);
        let far = level(&mut e, [0.0, 0.0, 2500.0]);
        let right = level(&mut e, [80.0, 0.0, 0.0]);
        assert!(near.0 > 0.0, "{near:?}");
        assert!(far.0 < near.0 * 0.2, "near {near:?} far {far:?}");
        assert!(right.1 > right.0 * 1.5, "{right:?}");
    }

    #[test]
    fn synthetic_bank_loops_and_resamples() {
        // A 32 kHz mono square wave in a wave bank, played through a
        // hand-made sound bank isn't needed: the cursor mixer alone.
        let samples: Vec<i16> = (0..3200).map(|i| if (i / 16) % 2 == 0 { 16000 } else { -16000 }).collect();
        let wb = Arc::new(WaveBank::parse(Arc::new(test_banks::wave_bank(&[(1, 32000, samples)]))).unwrap());
        let entry = wb.entries[0];
        let event = PlayEvent { timestamp_ms: 0, random_offset_ms: 0, loop_count: 2, flags: 0, waves: vec![super::super::xact::xsb::WaveRef { bank: 0, index: 0, weight: (0, 255) }], variation: 0, pitch_variation: None, volume_variation: None };
        let mut ts = TrackState { delay: 0, loops: 0, cursor: Some(Cursor { step: 32000.0 / 48000.0, bank: wb, entry, pos: 0.0, choice: 0 }), volume_db: 0.0, pitch_cents: 0.0, gains: [0.0; 2], started: false };
        let mut rng = 1;
        // 3200 frames at 32 kHz = 0.1 s = 4800 output frames; played 3 times.
        let mut mix = vec![0.0; 2 * 4800 * 3 - 64];
        let done = mix_track(&mut ts, &event, &mut mix, [1.0, 1.0], 1.0, false, &mut rng);
        assert!(!done);
        assert!(rms(&mix[mix.len() - 200..]) > 0.3);
        let mut tail = vec![0.0; 2 * 256];
        assert!(mix_track(&mut ts, &event, &mut tail, [1.0, 1.0], 1.0, false, &mut rng));
    }
}
