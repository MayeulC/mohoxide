//! Writing keyframes as a game plays: a replay being watched, or the
//! run-ahead process that plays the replay headless to write them before
//! the viewer gets there. The sim thread saves (the heap walk needs the VM
//! still); a writer thread compresses and writes, so the sim waits only for
//! the save.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use mlua::prelude::*;

use super::store::{Entry, Store};
use super::Keyframe;

/// Ticks between full keyframes: a diff's chain is at most this long.
pub const FULL_EVERY: u64 = 6000;

/// The player's keyframe options (the Options screen's Interface tab; the
/// renderer sets them from the profile when a replay starts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Play the replay ahead in another process, writing keyframes.
    pub precompute: bool,
    /// Ticks between keyframes (a multiple of the collection interval).
    pub every: u64,
    /// Disk for all replays' keyframes, MB (the oldest replays' go first).
    pub budget_mb: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self { precompute: true, every: 600, budget_mb: 2048 }
    }
}

static SETTINGS: std::sync::RwLock<Option<Settings>> = std::sync::RwLock::new(None);

/// The keyframe options in force.
pub fn settings() -> Settings {
    SETTINGS.read().ok().and_then(|s| *s).unwrap_or_default()
}

pub fn set_settings(s: Settings) {
    let every = (s.every / crate::engine::sim::gc::EVERY).max(1) * crate::engine::sim::gc::EVERY;
    if let Ok(mut w) = SETTINGS.write() {
        *w = Some(Settings { every, ..s });
    }
}

/// Where replays' keyframe folders live.
pub fn root() -> Option<PathBuf> {
    crate::install::keyframes_dir()
}

/// The keyframe folder of the replay at `path`: by its bytes and this
/// build (another build's keyframes don't load).
pub fn dir_for_replay(root: &Path, path: &Path) -> std::io::Result<PathBuf> {
    let bytes = std::fs::read(path)?;
    let hash = xxhash_rust::xxh3::xxh3_128(&bytes) ^ u128::from(xxhash_rust::xxh3::xxh3_64(crate::net::core::build_id().as_bytes()));
    Ok(root.join(format!("{hash:032x}")))
}

/// Deletes whole replays' keyframe folders, least recently written first,
/// until they fit in `budget` bytes; never `keep`.
pub fn evict(root: &Path, keep: &Path, budget: u64) {
    let mut dirs: Vec<(std::time::SystemTime, u64, PathBuf)> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let path = e.path();
            let store = Store::new(&path, FULL_EVERY);
            let newest = std::fs::read_dir(&path).into_iter().flatten().flatten().filter_map(|f| f.metadata().ok()?.modified().ok()).max().unwrap_or(std::time::UNIX_EPOCH);
            (newest, store.size(), path)
        })
        .collect();
    let mut total: u64 = dirs.iter().map(|d| d.1).sum();
    dirs.sort();
    for (_, size, path) in dirs {
        if total <= budget {
            break;
        }
        if path == keep {
            continue;
        }
        tracing::info!(dir = %path.display(), mb = size / 1_048_576, "keyframes: over budget, dropping an old replay's");
        if std::fs::remove_dir_all(&path).is_ok() {
            total -= size;
        }
    }
}

enum Job {
    Write(Keyframe),
    /// The keyframe loaded on resume (and its chain's full keyframe): the
    /// next diff is from it.
    Prime(Entry, Vec<u8>, u64),
}

/// Saves a keyframe every [`Recorder::every`] ticks into a store.
pub struct Recorder {
    store: std::sync::Arc<Store>,
    every: u64,
    jobs: Option<mpsc::SyncSender<Job>>,
    writer: Option<std::thread::JoinHandle<()>>,
}

impl Recorder {
    pub fn new(dir: PathBuf, every: u64) -> Self {
        let store = std::sync::Arc::new(Store::new(dir, FULL_EVERY));
        // Two in flight at most: a slow disk holds the sim back rather than
        // piling up 60 MB keyframes.
        let (jobs, rx) = mpsc::sync_channel::<Job>(2);
        let writing = store.clone();
        let writer = std::thread::Builder::new()
            .name("keyframe writer".into())
            .spawn(move || {
                let mut previous: Option<(Entry, Vec<u8>)> = None;
                let mut last_full: Option<u64> = None;
                for job in rx {
                    match job {
                        Job::Prime(entry, raw, full) => {
                            last_full = Some(full);
                            previous = Some((entry, raw));
                        }
                        Job::Write(k) => {
                            let t = std::time::Instant::now();
                            match writing.write(&k, previous.as_ref().map(|(e, r)| (e, r.as_slice())), last_full) {
                                Ok((raw, entry)) => {
                                    if entry.is_full() {
                                        last_full = Some(entry.tick);
                                    }
                                    tracing::debug!(tick = entry.tick, full = entry.is_full(), ms = t.elapsed().as_millis() as u64, "keyframe written");
                                    previous = Some((entry, raw));
                                }
                                Err(e) => tracing::warn!(tick = k.tick, "keyframe not written: {e}"),
                            }
                        }
                    }
                }
            })
            .expect("keyframe writer thread");
        Self { store, every: every.max(crate::engine::sim::gc::EVERY), jobs: Some(jobs), writer: Some(writer) }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Loads the latest keyframe at or before `tick` into `lua` (an engine
    /// at its base), and has the next keyframe written as a diff from it.
    /// Returns the tick loaded, if there was one.
    pub fn resume(&self, lua: &Lua, tick: u64) -> LuaResult<Option<u64>> {
        let Some(entry) = self.store.latest_at_or_before(tick) else { return Ok(None) };
        let t = std::time::Instant::now();
        let k = match self.store.read(entry) {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!(tick = entry.tick, "keyframe unreadable, playing from the start: {e}");
                return Ok(None);
            }
        };
        let read_ms = t.elapsed().as_millis() as u64;
        // Another build's or session's: the game plays from its start (a
        // load that fails part-way would leave the VM unusable, so check).
        if !super::fits(lua, &k) {
            tracing::warn!(tick = entry.tick, "keyframe from another build or start-up, playing from the start");
            return Ok(None);
        }
        super::load(lua, &k)?;
        tracing::info!(tick = entry.tick, read_ms, total_ms = t.elapsed().as_millis() as u64, "resumed from a keyframe");
        if let (Some(jobs), Ok(raw), Some(full)) = (&self.jobs, super::store::encode(&k), self.store.full_of(entry)) {
            let _ = jobs.send(Job::Prime(entry, raw, full));
        }
        Ok(Some(entry.tick))
    }

    /// End of tick `tick`: a keyframe if one is due and not on disk yet
    /// (the run-ahead process may have written it).
    pub fn after_tick(&self, lua: &Lua, tick: u64) -> LuaResult<()> {
        if tick % self.every != self.every - 1 || tick % crate::engine::sim::gc::EVERY != crate::engine::sim::gc::EVERY - 1 {
            return Ok(());
        }
        if self.store.entries().iter().any(|e| e.tick == tick) {
            return Ok(());
        }
        let _s = crate::prof_span!("keyframe save");
        let k = super::save(lua, tick)?;
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(Job::Write(k));
        }
        Ok(())
    }
}

impl Drop for Recorder {
    /// Finishes writing what's queued.
    fn drop(&mut self) {
        self.jobs.take();
        if let Some(w) = self.writer.take() {
            let _ = w.join();
        }
    }
}
