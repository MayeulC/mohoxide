# Architecture overview

A one-page map of the engine. Each section links to the doc that goes deeper.

## The big picture

The engine does what moho (Forged Alliance's engine) does: it loads the game's archives, runs FAF's Lua unchanged on top of a native sim, and draws the result.

```
            player input                      network peers / replay
                 │                                     │
                 ▼                                     ▼
   ┌──────────────────────────┐   PlayerCommands   ┌─────────────────────────┐
   │ main thread              │ ─────────────────▶ │ sim thread              │
   │  window, input, egui     │  (stamped with     │  fixed-point sim        │
   │  user-layer Lua VM       │   the tick they    │  sim Lua VM (FAF)       │
   │  (FAF's interface)       │   run on)          │  stages on rayon pool   │
   │                          │ ◀───────────────── │                         │
   └──────────────────────────┘  Snapshots + Sync  └─────────────────────────┘
          ▲            │         (bounded channel,
          │            │          dropped if behind)
          │            │
          │            │ each frame: scene changes + input,
          │            ▼ then the interface's draw list
   ┌──────────────────────────┐
   │ present thread           │
   │  GPU: interpolation,     │
   │  posing, scene, interface│
   └──────────────────────────┘
```

- **The sim is the only thing that changes game state.** Input from the window, a network peer or a replay arrives as `PlayerCommand`s (`src/engine/sim/input.rs`) that hold only sim values and are stamped with the tick they run on. In multiplayer, every peer runs the same sim on the same commands, so the sim must be deterministic: see [determinism.md](determinism.md).
- **Rendering never writes back.** At the end of a tick the sim packs what the renderer needs into a snapshot (`src/engine/snapshot.rs`) and offers it on a bounded channel. If the renderer is behind, the snapshot is dropped and the sim doesn't wait. The main thread receives snapshots (the interface needs them too) and passes the scene changes on; the present thread interpolates between ticks when drawing. See [rendering.md](rendering.md).
- **Each frame goes to the present thread in two parts.** First the scene changes and the frame's input, so the present thread can start the heavy preparation (interpolation, posing, particles); then, once the interface's Lua has run, its draw list. The preparation overlaps the interface work, and a late second part (a Lua hitch) doesn't freeze the picture: the frame is drawn with the previous draw list.
- **Two Lua VMs, as in moho.** The sim VM runs FAF's sim scripts. The user-layer VM (`src/ui/`) runs FAF's interface scripts on the main thread. It reads the sim only through snapshots and the `Sync` table (`src/engine/sync.rs`), and acts only through player commands. See [lua-bridge.md](lua-bridge.md) and [ui.md](ui.md).

## Threads

| Thread | What it does |
|---|---|
| main | The window's events, input, cameras, picking, the user-layer Lua VM, egui (winit requires this on macOS) |
| `present` | Owns the GPU surface: interpolation, posing, particles, encoding and presenting. Paced by the swapchain |
| `sim` | The tick loop and the sim's Lua VM |
| `worker N` (global rayon pool) | The sim's parallel stages, plus startup work (script compiles, VFS reads). One per core (`src/threads.rs`) |
| render pool | The present thread's parallel work. Kept separate so render tasks don't queue behind sim stages |
| `audio` | The mixer (see [audio.md](audio.md)) |
| `net` | The lockstep link to peers (see [networking.md](networking.md)) |
| `sim-watchdog` | Exits if the sim stalls, so long runs can't hang forever |
| `keyframe writer` | Writes sim keyframes off the sim thread (see [replays.md](replays.md)) |

## Startup

The VFS mounts the game's `.scd` archives and FAF's `.nx2` patches. FAF's Lua is translated to Luau and compiled, blueprints are parsed, then the map loads and the session launches the way moho's does. All of this runs in parallel where it can. [game-start-sequence.md](game-start-sequence.md) walks through it step by step.

## Where the code lives

| Path | What's there |
|---|---|
| `src/main.rs`, `src/cli.rs` | Entry point, command-line options, thread start-up |
| `src/engine/sim/` | The sim: entities, movement, pathing (HPA*, flow fields, formations), weapons, projectiles, economy, intel, the coroutine scheduler, checksums |
| `src/engine/api/` | The moho bindings: every native function and method FAF's sim Lua calls |
| `src/lua_api/`, `crates/lua-api-macros/` | The typed Lua API: how bindings are declared, and the Luau/EmmyLua type output (see [lua-api-types.md](../guides/lua-api-types.md)) |
| `src/engine/lua/`, `src/engine/lua_translation/` | Sim VM setup, and the source transforms from FAF's Lua dialect (`!=`, `#` comments, bitwise ops) to Luau |
| `src/lua_overrides/` | Small Lua overrides layered over FAF's scripts, for the sim and the interface |
| `src/engine/vfs/`, `src/engine/assets/` | The virtual file system and the asset decoders (see [vfs-assets.md](vfs-assets.md)) |
| `src/engine/session.rs`, `src/engine/mods.rs` | Session launch and mod loading |
| `src/engine/snapshot*`, `src/engine/sync.rs` | What crosses from the sim to the render and user-layer side |
| `src/engine/keyframe/`, `src/engine/replay.rs` | Saving and loading the sim, and replays |
| `src/render/` | The renderer (wgpu) |
| `src/ui/` | The user-layer Lua VM and the interface's controls and draw pass |
| `src/audio/` | Sound |
| `src/net/` | Networking and lockstep |
| `src/install.rs` | Finding the game and FAF folders |
| `src/weapon_audit.rs`, `src/audit.rs` | The engagement audits run by the tests |
| `benches/` | Criterion benchmarks |
| `tests/` | Integration tests and their fixtures: scenario scripts (`tests/scenarios/`), goldens, keyframes, API surface, the test vault |
| `dev/` | Dev tooling: hand-run scenes (`dev/scenes/`: `visual/`, `play/`, `ui/`, `watch/`, `stress/`), audits, probes for the real game, packaging, the test vault |
| `vendor/` | The patched Luau source |
