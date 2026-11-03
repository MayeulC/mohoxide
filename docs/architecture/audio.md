# Audio

SupCom's sound is Microsoft XACT: `/sounds/SupCom.xgs` (global settings),
`*.xsb` sound banks (cue names -> sounds -> tracks -> play-wave events) and
`*.xwb` wave banks (the samples); voice-overs are in `/sounds/voice/<lang>/`.
`src/audio/` reads them and plays them through the OS output with `cpal`.

**Audio is presentation only.** Nothing the sim can observe depends on it:
the sim only *queues* world sounds (presentation data, outside the
checksum), the user layer's globals only send commands, and no audio state
flows back into the sim. The golden fight test proves the sim is unchanged.

## Formats found

| File | Magic / version | Contents |
|---|---|---|
| `SupCom.xgs` | `XGSF`, v43 | 39 categories (`Global` > `Default`, `Music`, `World` > `Units*`, `Weapons`, `Destroy`, ..., `Interface`, `FMV`, `Op_Briefing`), 22 variables (`Distance`, `Duck`, `*_LodCutoff`, `AttackTime`, `ReleaseTime`), 20 RPC curves |
| `*.xsb` (163) | `SDBK`, v43 | cues, sounds, tracks, events (play wave, track variation, pitch / volume variation), instance limits, fades |
| `*.xwb` | `WBND`, tool 43 / header 42 | **every wave (3043) is 16-bit PCM**, mono or stereo, 24 / 32 / 44.1 kHz. Streaming banks (flag 1: `Music`, `*Stream`, voices) are read from disk as they play |

So the only decoder is PCM (8 / 16 bit). ADPCM / XMA / WMA entries parse
but play silent (warned once); none ship with FA. The layouts are written
up in the module docs of `src/audio/xact/{xgs,xsb,xwb}.rs` (reverse
engineered; every shipped bank parses).

## Threads and data flow

```
sim thread                       render thread                       audio thread          device callback
Entity/Weapon:PlaySound  --+     Scene::receive(snapshot)            AudioEngine::handle   (cpal, OS thread)
SetAmbientSound            |       Command::World(sounds)  ------->  mix 256-frame blocks  pops f32 frames
  -> Presentation.sounds   +-->    Command::Ambients(list) ------->  into an rtrb ring  --> from the ring
  -> Snapshot.sounds /             Scene::update_listener (camera)    (30 ms ahead)          (no locks, no
     Snapshot.ambients             UserVm: PlaySound, PlayVoice, ...                          allocation)
                                     -> Command::Play / Start / ...
                                   <- StatusBoard (cue state per handle)
```

- **Sim side.** `Entity:PlaySound` / `Weapon:PlaySound` push a
  `SoundEvent` (shared `Arc<str>` names from the `Sound{}` handle, the
  entity's position as f32) onto `Presentation.sounds`; nothing else in
  the sim. The snapshot drains them (`Snapshot::sounds`) and lists every
  live ambient loop at its entity's position (`Snapshot::ambients`). A
  snapshot the channel can't take keeps its sounds for the next one (the
  latest 256); a run without a renderer (headless) drops them each tick.
- **Render thread.** `Scene::receive` forwards a snapshot's sounds and
  ambients to the audio thread; each frame `Scene::update_listener` sends
  the camera (eye, view direction, screen-right, zoom distance) when it
  moved.
- **User layer** (`src/ui/sound.rs`, both the front end's and the game's
  VMs): the moho globals send commands; a Lua handle (`CueHandle`
  userdata) carries the cue id, and its state (`Pending`, `Prepared`,
  `Playing`, `Done`) is read back from a shared `StatusBoard`. A collected
  handle is forgotten (the cue plays on).
- **Audio thread** (`audio::output::run`): owns the `AudioEngine` (bank
  cache, cue instances, mixer) and the device. It drains commands, then
  renders ahead into a lock-free `rtrb` ring that the device callback only
  copies out of: no locks, allocation or file access in the callback.
  Streamed banks are read here, from memory-mapped files. Without a device
  it renders against the wall clock and drops the samples, so cues still
  finish and scripts waiting on them carry on.

`Audio` (`src/audio/mod.rs`) is the cloneable handle. `main.rs` starts one
for windowed runs and installs it (`Audio::install`); a `UserVm` takes the
installed one, or `Audio::disabled()` (no thread; every cue is done at
once) in headless runs and tests.

## The XACT runtime (`src/audio/engine.rs`)

- **Cues.** A cue's sound has tracks; each track's event plays a wave
  (with a variation choice: ordered, random, random without repeat), after
  its timestamp plus a random offset, with pitch / volume variation and a
  loop count (255 = forever). Waves are resampled linearly to the output
  rate, pitch included.
- **Categories.** Gain is the category's authored volume times the
  script's `SetVolume` volume, down the parent chain (`Global` scales
  everything). `PauseSound(category)` pauses that category and its
  children. Instance limits per category and per cue: fail, queue,
  replace oldest / quietest / lowest priority, with the category's fade.
- **RPCs.** Volume and pitch curves on `Distance`, `Angle`,
  `AttackTime`, `ReleaseTime`, `Duck`, `CameraDistance` and the other
  global variables, evaluated once per 256-frame block and ramped across
  it. `StopSound(h)` (not immediate) releases: loops break and
  `ReleaseTime` curves fade (the music's ~6 s fade).
- **3D.** Distance from the camera eye drives the `Distance` RPCs (the
  game's attenuation); pan is the emitter's side of the screen (never hard
  left / right). A `Sound{LodCutoff = 'Weapon_LodCutoff'}` isn't played
  past that variable's distance.
- **Voices.** `PlayVoice` queues: one voice at a time, the next when it
  ends. Ducking voices (`PlayVoice(s, true)`) raise the `Duck` variable,
  which the music's RPC turns down. SupCom.xgs has no VO category, so
  `SetVolume('VO')` / `PauseVoice('VO')` are applied to voices directly.
- **Output.** Stereo f32, summed, then a peak limiter (instant attack,
  ~0.5 s release) and a hard clip.
- Bank lookup: `/sounds/voice/<lang>/` (`AudioSetLanguage`), then
  `/sounds/voice/us/`, then `/sounds/`. Banks load on first use and stay
  cached; folder-mounted banks are memory-mapped, archive ones read whole.

## Lua API

| Global | Behaviour |
|---|---|
| `PlaySound(sound, prepareOnly)` | starts the cue (or loads it only); returns a handle |
| `PlayVoice(sound, duck, prepareOnly)` | queued voice; FAF's `Movie` prepares one with `PlayVoice(v, false, true)` |
| `StartSound(h)`, `StopSound(h, immediate)` | start a prepared cue; stop with release (or at once) |
| `SoundIsPrepared(h)` | loaded (or playing / done): FAF's `Movie` waits on it before `OnLoaded` |
| `WaitFor(h)` | user layer: yields until the cue ends (`UserMusic.PlayMusic`'s fade between tracks) |
| `PauseSound(cat, b)`, `PauseVoice('VO', b)` | `gamemain.OnPause` / `OnResume`, the briefing |
| `SetVolume(cat, v)`, `GetVolume(cat)` | 0..2; categories `Global`, `World`, `Interface`, `Music`, `VO`, ... |
| `SetMovieVolume`, `GetMovieVolume` | kept as the `Movie` volume for the video player (FMVs' own soundtracks) |
| `StopAllSounds`, `Enable/DisableWorldSounds` | score screen; `OnFirstUpdate` |
| `AudioSetLanguage(lang)` | also picks the voice banks |

At start the user layer runs the profile's sound options' setters (moho's
`optionslogic.Apply(true)`, sound section only): master, FX, music and VO
volumes. Music then follows FAF: the main menu's `Main_Menu` cue, in game
`UserMusic.StartPeaceMusic` from `OnFirstUpdate`, and battle music once
`gamemain.OnFocusArmyUnitDamaged` (called each beat for the focus army's
units whose health dropped) has counted enough hits.

## Checking it without speakers

```sh
cargo run -- --no-audio --audio-dump menu.wav --audio-dump-seconds 8 --screenshot menu.png --screenshot-tick 120
cargo run -- --script dev/scenes/fight.lua --no-ai --camera 512,512,176 --no-audio --audio-dump fight.wav --audio-dump-seconds 16 --screenshot f.png --screenshot-tick 200
```

The dump is the mixed output (16-bit stereo WAV); the log line `audio dump
written` gives its RMS (0 means nothing played). The
dump is written when its seconds are up, so keep it shorter than the run.

## Performance

- Sim: one `Vec` push per `PlaySound` (names are `Arc<str>` clones);
  the snapshot moves them out. Nothing per tick otherwise.
- Mixer: a small fraction of one core; benched by `audio/mix block, music +
  32 world cues` (`benches/engine.rs`).
- Short banks are memory-mapped and read in place (no decode cache
  needed: PCM is read straight from the map); streamed banks are paged in
  by the OS as they play.

## Line-of-sight culling

As moho's `CUserSoundManager::FilterSound` (`snd_CheckLOS`; see
docs/research/verifying-against-the-binary.md, Confirmed): while fog of war is on (the
game's `FogOfWar` option isn't `'none'`), world sounds and ambient loops
at a spot the listener army's side can't see are dropped when the
snapshot is published (`SoundSight` in `engine/snapshot/fog.rs`). The
listener is the UI's focus army (`DebugRequest::view_army`, set each
frame by the renderer; the fog of war's view army, `engine/snapshot/fog.rs`); observers hear everything. Vision decides, water
vision for units on the Sub / Seabed layers; omni and radar don't count.
Moho checks a loop once when it starts; ours are re-listed each snapshot,
so a loop that leaves vision stops.

## Open

- `PlayTutorialVO` is a no-op; `Rumble*` variables (controller rumble) are
  ignored.
- Ambient loops follow their entity at snapshot rate (10 Hz), not the
  interpolated render position.
- The device is the system default, opened once; a device change (headphones
  unplugged) isn't followed yet.
- Loudness against the real game is unverified (no reference capture yet).
