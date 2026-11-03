# The engine API (moho bindings)

FAF's Lua calls into the engine through moho's native API: globals (`IssueMove`, `CreateUnitHPR`,
`Damage`, `GetFocusArmy`, ...), methods on engine objects (`unit:GetPosition()`,
`brain:GetArmyStat(...)`, `weapon:SetTargetEntity(...)`), and callbacks the engine makes into FAF's
scripts (`OnCreate`, `OnImpact`, `OnStopBuild`, ...). In the real game these are C++ in
`SupremeCommander.exe` / `MohoEngine.dll`. Here they're Rust, and FAF's Lua can't tell the
difference.

## Where the contract comes from

- **The binary first.** Argument counts and types come from moho's binding code, read through
  `../FA-Binary-Patches` (`include/LuaCFuncs.h`, `moho.h`) or disassembly
  ([verifying-against-the-binary.md](../research/verifying-against-the-binary.md)).
- **FAF's declarations** (`../fa/engine/*.lua`, `/engine/Sim.lua`, `/engine/User.lua`, ...) list
  every function and give documentation, but their annotations are sometimes wrong. Use them to find
  what exists, not as the final word on signatures.
- **Behaviour** comes from FAF's Lua (how it's called and what it expects back), the binary, and
  probes in the real game ([probing.md](../guides/probing.md)).

## Seeing the whole API

`cargo run -- --dump-api docs/api` writes the full API as Luau and EmmyLua types, one file per VM,
generated from the Rust registrations ([lua-api-types.md](../guides/lua-api-types.md)). Every entry
says whether it's implemented or a stub. That generated list is the reference; this page doesn't
repeat it.

How to add or change an API (the `lua_fn!` / `lua_method!` / `callbacks!` macros, typed tables):
[lua-api-types.md](../guides/lua-api-types.md).

## How it fits together

- **Script objects** (`api/script_object.rs`): every engine object (unit, weapon, projectile, brain,
  platoon, ...) is a plain Lua table, so FAF can `setmetatable` it to its own classes, carrying the
  native handle in `_c_object`. A method looks the handle up in Rust state. Lua never holds Rust
  memory.
- **Callbacks** (`api/callbacks.rs`): every callback the sim makes is declared once, with its
  arguments, so a misspelt name or wrong argument is a compile error. As in moho, a missing method is
  skipped and an error is logged, never thrown into the engine operation that fired it.
- **Values cross at the boundary.** The sim works in fixed point; numbers become Lua numbers when
  returned and are converted back when passed in ([determinism.md](../architecture/determinism.md)).
- **Stubs.** Everything FAF declares exists from the start, because FAF captures many functions at
  load time (`local X = X`). Unimplemented sim functions raise a clear error when called
  (`api/pending.rs`); unimplemented interface functions warn once and return nil
  (`src/ui/user_api.rs`). Open lists: #1 (sim), #2 (interface).

## Where each area lives

Sim side, `src/engine/api/`:

| Area | Files |
|---|---|
| Units, entities, props | `unit.rs`, `entity.rs`, `terrain.rs` (map props), `destroyed.rs`, `blip.rs` |
| Orders | `command.rs` (`Issue*`, queues), `orders.rs`, `player_input.rs`, `command_source.rs`, `formation.rs` |
| Unit work | `construction.rs`, `reclaim.rs`, `capture.rs`, `transport.rs`, `ferry.rs`, `silo.rs`, `teleport.rs`, `sacrifice.rs`, `overcharge.rs`, `special_action.rs`, `script_task.rs` |
| Movement and pathing | `movement.rs`, `navigation.rs`, `spec_footprints.rs` |
| Combat | `weapon.rs`, `projectile.rs`, `beam.rs`, `damage.rs`, `armor.rs`, `manipulator.rs` |
| Economy and armies | `army.rs` (brains, stats), `unit_stats.rs`, `economy_event.rs`, `adjacency.rs`, `deposits.rs` |
| AI | `threat.rs`, `personality.rs`, `platoon.rs`, `attack_vectors.rs`, `build.rs` |
| Blueprints and categories | `blueprints.rs`, `categories.rs`, `mesh.rs` |
| Presentation (never feeds the sim) | `effect.rs`, `presentation.rs`, `audio.rs`, `sound.rs`, `colors.rs` |
| Time, threads, randomness | `sim_time.rs`, `threads.rs`, `random.rs`, `replay.rs` |
| Lua runtime and files | `core.rs`, `globals.rs`, `loader.rs`, `dofile_or_script.rs`, `disk_*.rs`, `path_utils.rs`, `debug.rs`, `nil_metatable.rs`, `thread_metatable.rs` |
| Bookkeeping | `script_object.rs`, `callbacks.rs`, `signatures.rs`, `pending.rs` |

Interface side: `src/ui/user_api.rs` (globals), `src/ui/controls.rs` (maui controls), `src/ui/prefs.rs`,
`src/ui/camera.lua` ([ui.md](../architecture/ui.md)).

## Army stats

`brain:GetArmyStat(name)` reads stats moho keeps itself (`api/army.rs`, `Army::economy_stat`):

- **Economy:** `Economy_{TotalProduced,TotalConsumed,Income,Output,Stored,MaxStorage,PeakStorage,Reclaimed,AccumExcess,Shared,Ratio}_{Mass,Energy}`,
  worked out on read from the economy's running totals, so the tick only adds to the totals.
  Produced includes reclaim and what allies shared in; excess is the overflow no ally took; shared
  is what went to allies. Also `UnitCap_Current` / `UnitCap_MaxCap`.
- **Units** (`api/unit_stats.rs`, with moho's rules): `Units_Active`, `Units_History`,
  `Units_BeingBuilt`, `Units_Killed`, `Units_{Mass,Energy}Value_{Built,Lost}`, `Enemies_Killed`,
  `Enemies_{Mass,Energy}Value_Destroyed`, `Enemies_Commanders_Destroyed`, kept as units are made,
  finished, killed and destroyed, so `GetBlueprintStat` and stat triggers see them too.
- **Damage:** `DamageStats_TotalDamage{Dealt,Received}` per army and `Units_TotalDamage{Dealt,Receive}`
  per blueprint, counted after armour (`api/damage.rs`).

FAF's score thread (`/lua/sim/score.lua`) turns these into the scoreboard and the end-of-game screen.
Not kept yet: `Economy_Trend_*`.
