//! Movies (moho's `CMauiMovie`, `moho.movie_methods`; FAF's
//! `/lua/maui/movie.lua` builds on it): SupCom's `.sfd` videos playing in
//! the interface.
//!
//! Each movie decodes on a thread of its own (`engine::assets::mpeg`,
//! streaming from the file), a few frames ahead through a bounded channel;
//! the interface takes them as its clock (`CurrentTime`, frame time) reaches
//! each frame's time and the renderer draws the newest one
//! (`DrawItem::Movie`). Decoded planes go back to the decoder for reuse.
//! When decoding falls behind, playback waits for it instead of skipping
//! ahead. Interface-only: nothing here reaches the sim.
//!
//! As moho: `InternalSet` opens the file and sets `MovieWidth/Height`,
//! `IsLoaded` once the first frame is decoded, `Play` / `Stop` (a pause),
//! `Loop`, and the engine calls `OnFinished` when a movie that isn't
//! looping ends. A movie that is missing or can't be decoded loads at once
//! and finishes as soon as it's played, so FAF's flows waiting on one (the
//! campaign timeline, briefings, transmissions) carry on.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
use std::sync::{Arc, RwLock};

use mlua::prelude::*;

use super::controls::{self, ControlId};
use crate::engine::api::moho;
use crate::lua_api::{Obj, lua_method};
use crate::engine::assets::mpeg::{Mpeg, Planes};
use crate::engine::vfs::Vfs;

/// Frames decoded ahead of the one shown.
const AHEAD: usize = 3;

/// Size FAF lays out a movie that can't be played at (as the old stub).
const MISSING_SIZE: (u32, u32) = (640, 480);

/// A decoded frame as the renderer gets it. `serial` is unique per frame
/// (the renderer uploads a movie's planes when it changes).
#[derive(Debug)]
pub struct VideoFrame {
    pub serial: u64,
    pub planes: Planes,
}

/// A movie's current frame in the draw list.
#[derive(Clone)]
pub struct FrameRef(pub Arc<VideoFrame>);

impl PartialEq for FrameRef {
    fn eq(&self, other: &Self) -> bool {
        self.0.serial == other.0.serial
    }
}

impl std::fmt::Debug for FrameRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Frame#{} {}x{}", self.0.serial, self.0.planes.width, self.0.planes.height)
    }
}

static SERIAL: AtomicU64 = AtomicU64::new(1);

enum Msg {
    /// Frame `n` from the start (times run on across loops).
    Frame(u64, Planes),
    End,
}

/// What `InternalSet` learns from the file's headers.
#[derive(Clone, Copy, Debug)]
struct Info {
    width: u32,
    height: u32,
    fps: f64,
    frames: u32,
}

/// The decoder thread: decodes ahead until the channel is full, rewinds at
/// the end while looping, and stops when the player drops its receiver.
fn decode(mut m: Mpeg, tx: SyncSender<Msg>, recycle: Receiver<Planes>, looping: Arc<AtomicBool>) {
    let mut n = 0u64;
    let mut since_rewind = 0u64;
    loop {
        let mut planes = recycle.try_recv().unwrap_or_default();
        if m.decode(&mut planes) {
            if tx.send(Msg::Frame(n, planes)).is_err() {
                return;
            }
            n += 1;
            since_rewind += 1;
            continue;
        }
        // A movie that decodes nothing after a rewind ends rather than spins.
        if looping.load(Ordering::Relaxed) && since_rewind > 0 {
            m.rewind();
            since_rewind = 0;
            continue;
        }
        let _ = tx.send(Msg::End);
        return;
    }
}

/// One movie control's playback.
struct Player {
    path: String,
    info: Option<Info>,
    rx: Option<Receiver<Msg>>,
    recycle: Option<Sender<Planes>>,
    looping: Arc<AtomicBool>,
    current: Option<(u64, Arc<VideoFrame>)>,
    next: Option<(u64, Planes)>,
    /// The decoder has sent its last frame.
    ended: bool,
    playing: bool,
    /// Seconds played (interface time).
    clock: f64,
}

impl Player {
    /// Opens `path` and starts decoding (paused at the first frame).
    fn open(vfs: &RwLock<Vfs>, path: &str, looping: bool) -> Self {
        let looping = Arc::new(AtomicBool::new(looping));
        let mut p = Player { path: path.to_string(), info: None, rx: None, recycle: None, looping, current: None, next: None, ended: true, playing: false, clock: 0.0 };
        // The guard is dropped before decoding: the file streams on its own.
        let reader = vfs.read().ok().and_then(|v| v.open(path).ok());
        let Some(m) = reader.and_then(Mpeg::open) else {
            tracing::warn!(path, "movie missing or not MPEG-1: finishes when played");
            return p;
        };
        p.info = Some(Info { width: m.width(), height: m.height(), fps: m.frame_rate(), frames: m.frame_count() });
        let (tx, rx) = mpsc::sync_channel(AHEAD);
        let (recycle_tx, recycle_rx) = mpsc::channel();
        let looping = p.looping.clone();
        let spawned = std::thread::Builder::new().name("movie decode".into()).spawn(move || {
            crate::engine::tracy::name_current();
            decode(m, tx, recycle_rx, looping)
        });
        if spawned.is_ok() {
            p.rx = Some(rx);
            p.recycle = Some(recycle_tx);
            p.ended = false;
        }
        p
    }

    fn fps(&self) -> f64 {
        self.info.map_or(30.0, |i| i.fps)
    }

    /// Takes frames from the decoder up to the clock (the first one at
    /// once, so a paused movie shows it).
    fn pull(&mut self) {
        let Some(rx) = &self.rx else { return };
        loop {
            if self.next.is_none() {
                match rx.try_recv() {
                    Ok(Msg::Frame(n, planes)) => self.next = Some((n, planes)),
                    Ok(Msg::End) | Err(TryRecvError::Disconnected) => {
                        self.ended = true;
                        self.rx = None;
                        return;
                    }
                    Err(TryRecvError::Empty) => return,
                }
            }
            let n = self.next.as_ref().map_or(0, |(n, _)| *n);
            if self.current.is_some() && n as f64 / self.fps() > self.clock {
                return;
            }
            let (n, planes) = self.next.take().expect("next frame");
            let frame = Arc::new(VideoFrame { serial: SERIAL.fetch_add(1, Ordering::Relaxed), planes });
            if let Some((_, old)) = self.current.replace((n, frame)) {
                // The renderer has let go of it by now (the draw list is
                // rebuilt each frame); if not, it's simply freed.
                if let (Ok(old), Some(tx)) = (Arc::try_unwrap(old), &self.recycle) {
                    let _ = tx.send(old.planes);
                }
            }
        }
    }

    /// Advances by `dt` seconds. `true` when a non-looping movie has just
    /// played to its end.
    fn advance(&mut self, dt: f64) -> bool {
        if self.playing {
            self.clock += dt;
        }
        self.pull();
        if self.playing && self.next.is_none() {
            // When the last frame has had its time (at once if nothing
            // decoded).
            let shown_until = self.current.as_ref().map_or(0.0, |(n, _)| (n + 1) as f64 / self.fps());
            if self.ended {
                if self.clock >= shown_until {
                    self.playing = false;
                    return true;
                }
            } else {
                // Decoding is behind: wait for it rather than skip ahead.
                self.clock = self.clock.min(shown_until);
            }
        }
        false
    }

    fn loaded(&mut self) -> bool {
        self.pull();
        self.current.is_some() || self.ended
    }
}

/// Every movie control's player (app data of the user VM).
#[derive(Default)]
pub struct Movies {
    players: BTreeMap<ControlId, Player>,
}

fn movies(lua: &Lua) -> mlua::AppDataRefMut<'_, Movies> {
    lua.app_data_mut::<Movies>().expect("Movies not in app_data")
}

fn vfs(lua: &Lua) -> LuaResult<Arc<RwLock<Vfs>>> {
    lua.app_data_ref::<Arc<RwLock<Vfs>>>().map(|v| v.clone()).ok_or_else(|| LuaError::runtime("no VFS"))
}

/// Runs `f` on `this` movie's player, if it has one.
fn with<R>(lua: &Lua, this: &LuaTable, f: impl FnOnce(&mut Player) -> R) -> LuaResult<Option<R>> {
    let id = controls::id_of(this)?;
    Ok(movies(lua).players.get_mut(&id).map(f))
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua.set_app_data(Movies::default());
    lua_method!(lua, moho::Movie,
        /// Opens the movie at `path` (a VFS path) on this control, keeping its
        /// looping flag, and sets the control's `MovieWidth` / `MovieHeight`.
        /// Always returns true, also when the file is missing or unreadable.
        InternalSet = |lua, (this, path): (Obj<moho::Movie>, String)| {
            let id = controls::id_of(&this)?;
            let looping = movies(lua).players.get(&id).is_some_and(|p| p.looping.load(Ordering::Relaxed));
            let player = Player::open(&*vfs(lua)?, &path, looping);
            let (w, h) = player.info.map_or(MISSING_SIZE, |i| (i.width, i.height));
            movies(lua).players.insert(id, player);
            controls::set_var(&this, "MovieWidth", w)?;
            controls::set_var(&this, "MovieHeight", h)?;
            Ok(true)
        })?;
    lua_method!(lua, moho::Movie,
        /// Whether the movie has loaded (false when it has no player).
        IsLoaded = |lua, this: Obj<moho::Movie>| Ok(with(lua, &this, |p| p.loaded())?.unwrap_or(false)))?;
    lua_method!(lua, moho::Movie,
        /// Sets whether the movie loops (nil means false).
        Loop = |lua, (this, on): (Obj<moho::Movie>, Option<bool>)| {
            with(lua, &this, |p| p.looping.store(on.unwrap_or(false), Ordering::Relaxed))?;
            Ok(())
        })?;
    lua_method!(lua, moho::Movie,
        /// Frames per second (30 when there is no player).
        GetFrameRate = |lua, this: Obj<moho::Movie>| Ok(with(lua, &this, |p| p.fps())?.unwrap_or(30.0)))?;
    lua_method!(lua, moho::Movie,
        /// The number of frames (0 when unknown).
        GetNumFrames = |lua, this: Obj<moho::Movie>| Ok(with(lua, &this, |p| p.info.map_or(0, |i| i.frames))?.unwrap_or(0)))?;
    lua_method!(lua, moho::Movie,
        /// Stops playback.
        Stop = |lua, this: Obj<moho::Movie>| {
            with(lua, &this, |p| p.playing = false)?;
            Ok(())
        })?;
    lua_method!(lua, moho::Movie,
        /// Starts playback; a movie that played to its end starts again from
        /// the beginning.
        Play = |lua, this: Obj<moho::Movie>| {
            let id = controls::id_of(&this)?;
            let restart = movies(lua).players.get(&id).filter(|p| p.info.is_some() && p.ended && p.next.is_none()).map(|p| (p.path.clone(), p.looping.load(Ordering::Relaxed)));
            // Played to its end before (or it ended before `Loop(true)`
            // arrived): from the start again.
            if let Some((path, looping)) = restart {
                let player = Player::open(&*vfs(lua)?, &path, looping);
                movies(lua).players.insert(id, player);
            }
            with(lua, &this, |p| p.playing = true)?;
            Ok(())
        })?;
    Ok(())
}

/// Advances every playing movie by `dt` (interface time) and calls
/// `OnFinished` on those that just ended. Players of destroyed controls
/// stop (their decoder threads end with them).
pub fn tick(lua: &Lua, dt: f32) -> LuaResult<()> {
    let finished: Vec<ControlId> = {
        let mut m = movies(lua);
        if m.players.is_empty() {
            return Ok(());
        }
        let live = controls::live(lua);
        m.players.retain(|id, _| live.binary_search(id).is_ok());
        m.players.iter_mut().filter_map(|(&id, p)| p.advance(dt as f64).then_some(id)).collect()
    };
    for id in finished {
        let Some(obj) = controls::object(lua, id)? else { continue };
        if let Some(f) = obj.get::<Option<LuaFunction>>("OnFinished")?
            && let Err(e) = f.call::<()>(obj.clone()) {
            tracing::warn!(target: "lua", "movie OnFinished failed: {e}");
        }
    }
    Ok(())
}

/// Control `id`'s current frame, if it's a movie with one.
pub fn frame(lua: &Lua, id: ControlId) -> Option<FrameRef> {
    let m = lua.app_data_ref::<Movies>()?;
    m.players.get(&id)?.current.as_ref().map(|(_, f)| FrameRef(f.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::vfs::MountPoint;

    /// The FA install's movies mounted at `/movies`, next to the repo (or
    /// the worktree). Fails the test without them; `None` only when skipping
    /// (`RTS_SKIP_DATA_TESTS=1`).
    fn vfs() -> Option<RwLock<Vfs>> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let dirs = ["..", "../../../.."].map(|up| root.join(up).join("Supreme Commander Forged Alliance/movies"));
        let dir = match dirs.iter().find(|d| d.join("Abasi.sfd").exists()) {
            Some(d) => d.clone(),
            None => {
                crate::test_data::require("FA movies", &[dirs[0].join("Abasi.sfd")]);
                return None;
            }
        };
        Some(RwLock::new(Vfs::new(vec![MountPoint { source: dir, mountpoint: "/movies".into() }], vec![]).ok()?))
    }

    /// Plays at 60 frames a second of interface time until it finishes or
    /// `seconds` pass, giving the decoder time like a frame would.
    /// Returns (finished at, frames shown).
    fn play(p: &mut Player, seconds: f64) -> (Option<f64>, u64) {
        p.playing = true;
        let mut shown = 0;
        let mut last = None;
        for _ in 0..(seconds * 60.0) as usize * 20 {
            if p.clock >= seconds {
                break;
            }
            let done = p.advance(1.0 / 60.0);
            let serial = p.current.as_ref().map(|(_, f)| f.serial);
            if serial != last {
                shown += 1;
                last = serial;
            }
            if done {
                return (Some(p.clock), shown);
            }
            std::thread::sleep(std::time::Duration::from_micros(200));
        }
        (None, shown)
    }

    #[test]
    fn a_movie_plays_to_its_end_once() {
        let Some(vfs) = vfs() else { return };
        let mut p = Player::open(&vfs, "/movies/Abasi.sfd", false);
        let info = p.info.expect("opened");
        assert_eq!((info.width, info.height, info.frames), (192, 192, 149));
        for _ in 0..1000 {
            if p.loaded() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(p.current.is_some(), "the first frame shows before Play");
        let (finished, shown) = play(&mut p, 10.0);
        let end = finished.expect("finished");
        assert!((end - 149.0 / 29.97).abs() < 0.05, "ends after its last frame's time: {end}");
        // At 60 interface frames a second every one of its frames is shown.
        assert_eq!(shown, 149);
        assert!(!p.advance(1.0), "finishes once");
    }

    #[test]
    fn a_looping_movie_never_finishes() {
        let Some(vfs) = vfs() else { return };
        // 60 frames (2 s), played for 5 s.
        let mut p = Player::open(&vfs, "/movies/menu_background.sfd", true);
        let (finished, _) = play(&mut p, 5.0);
        assert_eq!(finished, None);
        let n = p.current.as_ref().map_or(0, |(n, _)| *n);
        assert!(n >= 140, "frame numbers run on across loops: {n}");
    }

    #[test]
    fn a_missing_movie_loads_and_finishes_at_once() {
        let vfs = RwLock::new(Vfs::new(vec![], vec![]).unwrap());
        let mut p = Player::open(&vfs, "/movies/nope.sfd", false);
        assert!(p.info.is_none() && p.loaded());
        assert_eq!(play(&mut p, 1.0).0, Some(1.0 / 60.0), "on the first frame played");
    }
}
