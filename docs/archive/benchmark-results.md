# Benchmark results (archive)

Results recorded during development, kept for reference: the evidence behind several parallel
thresholds in the code. They were measured on particular machines at particular times, so compare
your own changes against a baseline you record yourself ([how](../guides/benchmarks.md)).

`cargo bench` runs `benches/engine.rs` (criterion). Script benches need the FAF
checkout: `FAF_REPO=../fa cargo bench`. Filter with `cargo bench -- scheduler`.

Only hot paths are benchmarked: per-tick / per-unit sim code and startup
pipeline stages. Add a bench when you add or change such a path, and compare
against the baseline below (criterion also keeps per-machine history in
`target/criterion`).

## Baseline (Apple M-series, release, 2026-10-01)
| Bench | Time | Notes |
|-------|------|-------|
| scripts/translate Unit.lua | 6.65 ms | was 12.1 ms: the for-in pass scans tokens instead of a full_moon parse (2026-10-15); the rest is the vararg pass's parse |
| scripts/translate all FAF .lua (2798 files) | 129 ms | one thread, what a few-core machine pays at startup; was 538 ms with the for-in pass parsing every script with a generic `for` (1443 parses). Headless startup: 3.62 -> 2.68 s user CPU |
| scripts/translate all FAF .bp | 6.4 µs per file | blueprints go through the translation too since LOUD support (the base game's and LOUD's have `#` comments after fields): FAF's 4568 `.bp` files are ~29 ms on one thread, in the parallel prewarm. Measured 2026-10-28 on a `../fa` holding extra copies (150744 files, 964 ms), so per file |
| scripts/compile all FAF .lua (2798 files) | 193 ms | one thread, Luau bytecode compile |
| scripts/compile Unit.lua | 1.67 ms | Luau bytecode compile |
| blueprints/register uel0001 | 95 µs | was 4.5 µs before the table copy (2026-10-25): registration copies unit, projectile and prop blueprints as moho's per-state copy does, so `pairs` order matches the real game's. The copy is 86 µs of it (203 tables, 1187 keys, grown by insertion; GC isn't a factor) and keeps arrays, whose order can't change (95 -> 87 µs). Emitter, trail, beam and mesh blueprints aren't copied: emitters alone are 170k of the 229k blueprint tables. The interface's whole blueprint load, debug build: 164 ms before, 261 ms copying every kind, 189 ms now |
| mods/discover (100 mods) | 640 µs | every `mod_info.lua`, each in a throwaway VM, in parallel; once per session (2026-10-18) |
| mods/resolve (100 mods) | 19 µs | uid order + before/after; once per session |
| mods/hook check, unhooked script | 53 ns | per script load in a VM with mods: one normalise + hash lookup, then the shared cache |
| categories/(LAND * MOBILE) - ENGINEER | 34 ns | 1700-blueprint domain |
| categories/contains | 0.67 ns | single bit test |
| categories/build domain (1700 bps) | 0.53 ms | once per session |
| bounded_props/add at cap (1000) | 63 ns | `Prop:AddBoundedProp` with the list full: the front popped (moho's sift down) and the new prop pushed (2026-10-29) |
| bounded_props/remove (1000) | 0.54 µs | a destroyed prop leaving a full list: a scan for it, then a sift down; only props scan (2026-10-29) |
| rng/next_f64 | 0.75 ns | |
| terrain/height_at | 3.8 ns | bilinear, fixed-point |
| math/atan2 | 17.6 ns | integer octant reduction + A&S 4.4.49 polynomial (error < 5e-8); the `cordic` crate it replaced dominated weapon aiming |
| math/sin_cos | 11.5 ns | quarter-turn reduction + Taylor to r^12 (error < 2e-9); was 65 ns with `cordic` |
| scheduler/tick/1000 | 109 µs | ~110 ns per waiting thread |
| sim/tick/1000 threads + sync | 129 µs | full `sim::tick::step`: threads + `ResetSyncTable` beat (scheduler alone 103 µs on the same run) |
| economy/step/1000 units | 5.8 µs | two allocation-free passes, 4 armies (8.4 µs before the hot/cold entity split); 2026-10-06 with the army-stat running totals: 5.8 µs. 2026-10-28: armies now settle one at a time in index order (overflow to allies) with expense summed per army, not per unit; on a heavily loaded Mac (load 240) it ran 12-14 µs against 17-21 µs for the old step in the same process, so no slower |
| economy/step/1000 units, 4 allies sharing | 17 µs (loaded Mac, as above) | the same with all four armies allied and sharing: two full armies overflow to the other two every tick; finding the allies is a scan over the armies per donor, no allocation |
| unit_stats/damage event | 28 ns | the damage totals counted per `Damage` / `DamageArea` hit, beside the `OnDamage` Lua call it comes with (60-100 ns call overhead alone) |
| spatial/circle r50 (grid, 10k units) | 73 ns | vs 11 µs for the linear scan it replaced |
| spatial/damage area r4 origins (2000 units) | 13 µs | `in_shell`, 3D distance to each origin: `DamageArea`'s and `DamageRing`'s gather before |
| spatial/damage area r4 shapes (2000 units) | 5.1 µs | `DamageArea`'s gather: sphere vs each collision shape, as moho's; an XZ reach reject skips the rotate for most grid candidates |
| spatial/damage ring r2-4 shapes (2000 units) | 6.2 µs | `DamageRing`'s gather: outside the inner sphere's shape test, inside the outer's (two tests per candidate) |
| entities/lookup x10000 | 5.8 µs | ~0.58 ns per id lookup (sparse set) |
| entities/compact_now 10k (100 destroyed) | 29.5 µs | forced; `compact` only runs once 1/8 are destroyed |
| projectiles/straight/serial/2000 | 31.5 µs (2026-10-04; was 28.8) | ~16 ns each: cached speed, no re-alignment for straight flights. +1.3 ns each for missile defence (homing check, script-sphere and intercept gates, bigger per-projectile motion record). 2026-10-31 (the step on the mean of the old and new velocity, an unchanged velocity stepping as before): 33.9 µs against 32.7-33.3 on the same loaded machine before |
| projectiles/ballistic/serial/2000 | 165 µs | ~80 ns each: gravity + `look_along` (algebraic, no trig). 2026-10-31 (mean-velocity step for every axis, an unchanged axis stepping as before): 181.8 µs against 175.7-179.3 before on the same loaded machine |
| projectiles/homing_led/serial/2000 | 480 µs (2026-10-30, the lead takes a projectile target's velocity and a turn costs speed: 472 on the same run before; 464 on 2026-10-06; 2026-10-31, steering by the facing with the velocity projected onto it every tick: 464 against 504-506 before on the same machine) | homing missiles chasing moving units: ~230 ns each; 321 µs without `LeadTarget` (the lead is ~70 ns: two lengths and divides) |
| projectiles/homing_zigzag/serial/2000 | 513 µs (2026-10-06, load ~8) | the homing_led missiles zig-zagging (`MaxZigZag` 4, a draw every 4 ticks), draw pass included: +~9 ns each over homing_led's 496 µs measured alongside |
| projectiles/ballistic_detonating/serial/2000 | 202 µs (2026-10-06, load ~8) | ballistic shells with a `DetonateBelowHeight`: + ~13 ns each (a terrain height a tick) over ballistic's 175 µs measured alongside |
| projectiles/ballistic_spinning/serial/2000 | 331 µs (2026-10-22) | the ballistic shells spinning (`RotationalVelocity`, velocity-aligned): + ~80 ns each over ballistic's 173 µs measured alongside (the least-rotation turn keeping their roll, plus a CORDIC rotation a tick). Projectiles that don't spin: unchanged (ballistic 172.9 µs before and after) |
| projectiles/ballistic/parallel/10000 | 339 µs | vs 830 µs serial; parallel at 2000 is no faster (rayon wake ~100 µs), hence `PARALLEL_WORK` |
| weapons/serial/2000 | 205 µs (2026-10-31, no shot in the update that takes a target, 204-218 before on the same runs, rechecking/2000 325-327 vs 321-338, within noise; 201 on 2026-10-30, the `ChangeMaxHeightDiff` height test, unset in the bench: 195 on the same run before; 199 after moho's target search: 193 on the same run before it; the keep and fire range tests per tick; 192 after the posed-frame aim, 183 before it; 150 on 2026-10-04, was 320) | targeting + aim + fire clock, steady state (~75 ns per weapon). Was 190 earlier, 320 after turret arcs (an `atan2` per keep check and candidate) and the aim-frame rotate: arcs now skip full turns and test limited ones without `atan2`, and `atan2` / `sin_cos` are integer polynomials instead of the `cordic` crate |
| weapons/leading_direct/2000 | 226 µs (2026-10-06) | every target moving: + ~37 ns per weapon for the closed-form lead (455 µs with the iterative lead it replaced) |
| weapons/leading_ballistic/2000 | 776 µs (2026-10-06) | every target moving, ballistic: + ~310 ns per weapon (flight time refined through `solve_pitch`, up to 8 rounds, usually 3-4) |
| weapons/parallel/2000 | 122 µs | 500 weapons: 33 µs serial vs 95 µs parallel; 5000: 394 vs 184, so `PARALLEL_MIN` = 1536 |
| weapons/rechecking/2000 | 302 µs (2026-10-30, moho's search: 288 on the same run before it; 300 on 2026-10-21, serial/2000 158 on the same run) | every weapon `AlwaysRecheckTarget` with a 0.5 s check interval: each holds a target and searches again every 5 ticks (~375 ns per search). FAF leaves it on for direct and indirect fire, weapons over 50 range, structure anti-air, experimentals and bombers; off for the rest (anti-navy, mobile anti-air, anti-missile, death weapons) |
| weapons/rechecking_flat/2000 | 387 µs (2026-10-25; rechecking/2000 318 on the same run) | the same with the terrain blockage test on flat ground: every search's best candidate tested once (`Terrain::blocks_fire`, a 26-range line sampled every half cell): ~170 ns a search |
| weapons/rechecking_hills/2000 | 454 µs (2026-10-25) | the same on hills (2 high, in a 4-wide grid every 8): many best candidates blocked, so the rest are ranked and tested in turn |
| effects/expire 170k live, 1100 ending a tick | 217 µs (2026-10-26; was 1.147 ms, scanning every effect in parallel) | end-of-tick effect expiry with 20k units' 60k attached emitters / trails / beams and ~110k timed emitters (1100 made and 1100 ending a tick), including making them. Effects that end are found from an index (`Presentation`'s `EffectIndex`: timed ones bucketed by end tick, attached / beam ones by entity, the entities `EntityWorld::destroy` logged since the last tick) instead of a scan. Kept cheap per effect: in the 10k v 10k air duel a tenth of the live effects end every tick (3-6k of 15-47k: projectile trails), so a first version on ordered sets was slower there than the scan; this one is 7-20% faster there (2.38 -> 2.21 ms at its busiest) on one thread |
| weapons/dense_search/2000 | 17.1 ms (2026-10-30, moho's search: 21.3 on the same run before it: the heading arc is a cached vector test, not an `atan2` per candidate, and a candidate whose unpenalised key can't win skips the range test; 23.5 on 2026-10-26, was 52.0) | two armies of 1000 packed 1.5 apart in overlapping squares (the 10k v 10k air duel's blobs), 30 range, a forward 90 degree arc, every weapon due to search: hundreds of candidates each. The search streams the grid's candidates and keeps the least (priority class, distance, id), passing over one no better than the best so far before the targetable / intel / arc checks; it built, filtered and sorted the whole list. Same picks (golden tests; the duel replay's tick-1000 checksum unchanged). rechecking/2000 375 -> 314 µs, rechecking_flat 464 -> 401. Duel replay, 400 ticks: Weapons 46 -> 35 ms a tick, wall 39 -> 34-36 s |
| weapons/range_for/2000 | 83 µs (2026-10-19) | `Weapons::range_for` (moho's `GetTargetWeapon`) for 2000 units of four weapons, only the last able to hit the target: ~41 ns each (two binary searches into the weapon list, then the scan). Per tick only for hovering aircraft with a target and units on attack orders |
| projectiles/collision/serial/2000 | 143 µs | ~71 ns each with unit collision (2000 units nearby); 176 µs on 2026-10-04 (was 166 the same day before missile defence) |
| threat/refresh/2000 units | 15.6 µs | lazy, at most once per tick (8 armies, 20 km map) |
| threat/highest/ring 1 | 11.4 µs (2026-10-01); 17-24 µs on 2026-10-02 under a load average of 16 (another session building), develop measured 17.3 µs in the same conditions | full-map scan, 256 cells x 3x3 ring; serial (well under rayon's wake cost) |
| threat/at/ring 1 | 56 ns | |
| movement/serial/2000 | 215 µs (2026-10-04; was 334; 250 on 2026-10-22 on this machine, with and without `TurnRadius`: borrowing `MotionParams` instead of copying it paid for the new field) | ~107 ns per moving unit (2 atan2 + 1 sin_cos + sqrt, plus the motion model). The integer trig replaced CORDIC (334 -> 215); 289 µs before the motion model |
| movement/landing spot (crowded) | 9.4 µs (2026-10-23) | `sim::landing::find_spot` for a 1 x 4 transport whose goal is in a 20 x 20 block of landed units: rings out past them (moho's ring search; per cell the reservations, nav cells and a `units_in_rect`). Once per landing, not per tick. Adding `MotionState`'s landing fields (idle tick, hold point, spot) moved movement/serial to 257 µs and movement/terrain to 425 µs on this machine (from 250 / 415) |
| movement/landing spot (10k reserved) | 9.7 µs (2026-10-26) | `find_spot` from the centre of a block of 10k reserved 1 x 1 spots: every ring cell taken, all 900 checked, none free. The reservations are looked up by cell (`landing::ReservedCells`); before, each candidate scanned every spot, and the 10k v 10k air duel (`dev/scenes/stress/air_duel.lua`), its aircraft auto-landing over their start, spent 0.5–8 s a tick in movement (400 ticks: 241 s -> 35 s, movement 532 -> 17.6 ms/tick) |
| movement/landing spot cells (10k reserved) | 96 µs (2026-10-26) | building `ReservedCells` from 10k spots: once per tick that has a landing event |
| movement/dive_step 1000 subs | 18.6 µs (2026-10-25) | moho's eased dive / surface step (`movement::dive_step`, a `sin_cos` each): ~19 ns a sub, only for subs mid-dive (they're movement candidates while diving). Keeping a sub's depth and dive target on `UnitData`, not `MotionState`, left movement/serial at 281-286 µs and movement/terrain at 457-461 µs against 278 / 454 before; in `MotionState` they cost 291 / 462 |
| movement/dive_tilt 1000 subs | 101-120 µs (2026-10-28, noisy machine) | a sub's bow-first lean (`movement::dive_tilt`, moho's `CalcRollHack`) and its tilted orientation (`dive_orientation`: `from_heading`'s `sin_cos`, the shortest-arc tilt's sqrt and normalise): ~110 ns a sub, only for subs diving, surfacing or levelling off after (~2 s). The lean is on `UnitData` beside the depth, for the same reason |
| movement/chases/2000 | 69 µs (2026-10-22) | `movement::chases`: 2000 units on aggressive moves with a weapon target, half in scan range: ~35 ns each (the order lookup, two entity lookups, the leash bookkeeping). Serial: only engaged units on patrol / aggressive move / guard pass through it |
| movement/patrol_steps/2000 | 42 µs (2026-10-25) | `movement::patrol_steps`: the same 2000 units' task steps, every tick: ~21 ns each (the order lookup, the unit's layer, the step state carried over). Only units on patrol / aggressive move pass through it |
| movement/terrain/2000 | 415 µs (2026-10-07) | `serial` over terrain: each mover snaps to the ground (`snap_to_ground`: four corner heights, the diagonals' cross product, a tilt quaternion) — ~80 ns of the ~207 ns per mover. `serial` itself 228 -> 253 µs from the bigger `MotionState` (wobble, heading, circle) |
| movement/nav/2000 | 525-542 µs (2026-10-27) | `terrain` (flat) with a nav grid, each mover's path already planned (no search): ~270 ns per mover. (the step's open-cell check, moho's cell rule, footprints on moho's cells via `NavGrid::cell_for`, the mobile build range) measured against the commit before it, 3 interleaved rounds from separate checkouts: no change beyond noise anywhere (movement/serial -2.9%, terrain -2.8%, nav -1.9%, avoidance/* -0.4..-2.6%, path/cross-map +0.1%, path/local -0.3%, fit class 6x6 -0.7%, build grid -1.1%, flow steer the same) |
| movement/buildmobile/2000 | 655-664 µs (2026-10-27; 686-750 before on this machine, 1.01 ms on 2026-10-14) | builders with BuildMobile orders onto a standing structure (shared sites): per mover a `units_in_rect` on the order's site for the standing structure's skirt size (`goal()`; the bench has no `Env::build_skirts`, which the game fills per blueprint so skips it) |
| movement/facing/2000 | 137 µs (2026-10-06) | idle units turning on the spot toward a body-slaved weapon's target (`Env::face`): ~69 ns each |
| movement/hover/2000 | 773 µs (2026-10-22, ground collision fields: 754-761 without, interleaved; 745 with moho's attitude controller; 547 with the forces only, 581 before on this machine; 566 on 2026-10-19) | hovering aircraft (`hover_step`), half circling a target, half flying off, all accelerating (leaning): ~373 ns each — velocity steering, circling carrot, the attitude controller (target axes, error rotation, torque, integration: ~100 ns). No terrain, so the ground collision (only within 2 x max(SizeX, SizeZ) of the ground) never runs; the +2% is likely the larger `AirParams` (not measured apart). The working-state check was 4 name scans per tick (now a cached mask) |
| movement/winged/2000 | 916 µs (2026-10-23, idle planes steer for their point and land; 954 before on 2026-10-22) | bombers on attack runs (`ComputeAirCombatTactics` states, steering for the target 1 s ahead): ~464 ns each — the attitude controller ~100 ns, the target attitude (`CalcWingedOrientation`: two rotations, three normalisations, axes to quaternion) ~180 ns, the KMove force ~40 ns; trimmed from 1102 µs by a series rotation, a Newton renormalisation and fewer normalisations |
| movement/crash/200 | 4.3 µs (2026-10-06) | dead aircraft falling and tumbling (`crash_step`): ~21 ns each (gravity, the surface check, a quaternion product + normalise; no terrain in the bench); serial, only the few aircraft dying at once |
| movement/parallel/2000 | 182 µs | `PARALLEL_MIN` = 1024 movers (was 512 at ~167 ns each) |
| nav/path/local 60 | 3.3 µs | A* in a window round start and goal (short trips, 4096-expansion budget) |
| nav/path/mid 150 | 29 µs | |
| nav/path/mid 300 | 32 µs | sector graph (HPA*); 96 µs with JPS |
| nav/path/blocked goal 60 | 92 µs | goal inside a 12x12 structure, snapped to its nearest open cell; 118 ms before (searched the whole map for the blocked cell) |
| formation plan (in-game, 20-minute census) | ~76 µs | one `formations.lua` call per formation order of 20+ units (~48 µs for small groups); per order, not per tick |
| nav/path/unreachable pocket 60 | 107 µs | goal in an open pocket ringed by structures; a full search of the start region before region labels (~120-150 ms) |
| nav/refresh after a structure 1024 | 0.82 ms (2026-10-07; 1.05 ms) | 4 classes: each rebuilds the 5 sectors round it, then its components (union-find over every sector's regions); whole-map relabelling was 3.1 ms |
| nav/flow field cross-map 1024 (64 units, 1 sector) | 250 µs (2026-10-07; 744 µs before node costs went dense: a SipHash map keyed by cell, linear node lookup) | graph-wide costs to the goal + the sector the group stands in; ~5 cross-map searches, so a field pays off from a handful of units on a 1024 map; the graph is 16x bigger at 4096 and short orders favour searches, hence `FLOW_MIN` = 32 |
| nav/flow steer (8-cell lookahead) | 210 ns | per follower per tick: walk 8 cells downhill, steer for the farthest in sight |
| nav/fit class 6x6 1024 | 2.9 ms | once per class, when its first unit appears (rows in parallel; two run passes, independent of the size) |
| nav/place structure 2x2, 20 classes | 10.8 µs | place + remove, every class fitted: refits each class's cells around it, serially (4.2 ms with rayon per refit) |
| attachments/follow 1000 of 10k | 19 µs | per tick after movement; includes moving the 1000 parents |
| construction/tick/40 factories assisting (nothing to take), 200 engineers guarding | 65 µs (2026-10-28) | the whole construction stage (bench profile, opt 3) with 40 land factories assisting an air factory whose queue they can't build: each looks up its assist chain and checks `CanBuild` on the offered orders every tick, the steady worst case |
| transport/tick/2000 (200 ferrying, 1800 loading) | 270 µs (2026-10-31, busy machine) | ~135 ns each: the order scan, the reach check, the `Ferrying` state pass and the ferry routes and waits (`api::ferry`: the 200 ferrying fly a leg queued ahead of their route, so the steady pass only checks it's still running; the loads' beacon check is one category look-up per pass). Was 545 µs (2026-10-12) when the ferrying units' current order was the ferry leg itself |
| attachments/follow 100 on posed walker bones | 55 µs | per tick: children on a bone of a parent with manipulators follow the posed bone (`pose::bone_world`, ~0.5 µs each on the walker's deepest bone); parents without manipulators keep the rest-pose path. Few in play: collision beams on aimed turrets, tractor-claw victims, cargo on animated transports |
| UI frame (FAF's whole interface, one unit selected) | 0.93 ms (was 13, then 1.1) | `cargo test --release --test ui ui_frame_cost -- --ignored --nocapture`: user threads, `OnFrame`, draw list (292 items). Control layout is read in one Luau call per control with Luau's own `pcall`: controls FAF never laid out (circular default relations) used to raise an error per var per frame, and mlua builds a traceback for every error crossing into Rust (13 ms). Unlaid-out controls are skipped (they were drawn at the origin). Edges are read Left, Right, Top, Bottom with an early out at the first that fails (1.1 -> 0.93 ms). 2026-10-05: 1.0 ms with the mouse over the world (FAF's `OnUpdateCursor` every frame, 1.3 µs); finding the world view by hit testing the tree each frame cost 0.35 ms, so the cursor uses the view the last mouse event found |
| formations.lua AttackFormation, 1000 mixed land units | 6.1 ms per layout (was 10.0) | `cargo test --release --test scenarios formation_cost -- --ignored --nocapture`. `EntityCategoryContains` (called per unit per formation category, ~25k calls) now resolves engine entities through their handle to their category index, with the `_c_object` key string made once (a `&str` key is interned on every `raw_get`), and fetches the category domain only for non-entities. The rest is FAF's Lua and mlua call overhead; a native port would be the next step (at the cost of tracking FAF's file) |
| intel/update 2000 units (1000 moving) | 59 µs (2026-10-25, discs redrawn only after a third of a radius as moho, so the 0.4/tick movers rarely re-stamp; vision on 2-unit cells. Was 108 µs (2026-10-04, A/B vs 100 µs on a loaded machine: per-unit LOS/radar/sonar/omni masks diffed for `OnIntelChange` / `OnDetectedBy`, ~4 ns a unit; was 94, and 78 before cover masks: keeping them costs a pass over each stamped span) | 2 armies, vision 20-35, radar 60 on a tenth; moving 0.4/tick (T1 tank speed); incremental stamps + seen-ever record (~40 µs); stamps are a span per row (half-width stepped in from the radius) then a contiguous u16 saturating add that auto-vectorises: was 169 µs testing every cell's distance; 290 µs with a per-tick BTreeMap rebuild |
| intel/fog view 1024 map, all rows | 117 µs (2026-10-25; 21 µs at 4-unit cells) | the player's fog texture for a new view: `IntelGrids::fog_row` for 512 rows of 512 vision cells (moho's 2-unit vision grid since then, 4x the cells; the 4-unit radar / sonar / omni masks each fill two), a straight max-pass per sensor's cover masks (vectorises). Per snapshot only rows whose intel changed are rebuilt and sent |
| intel/fog view 1024 map, rows changed by one tick | 0.36 µs (0 of 512 rows; was 18 µs, 177 of 257) | the same game after 1000 units each moved 0.4: since then a disc is redrawn only once its source has moved a third of its radius, so one tick's 0.4 changes no rows (before, every cell crossing redrew). Explored marking (`intel/update` with vs without, A/B in place on a loaded machine, load ~20): 119 vs 161 µs and 0.53 vs 0.86 ms, i.e. within noise |
| intel/vision circles per snapshot (1000 of 2000 units) | 7.0 µs (2026-10-28) | the view side's vision sources at their positions (`IntelGrids::vision_sources` + entity positions), once a snapshot while fog is on: the circles the renderer draws clear |
| render/blend vision circles (1000 / 20000) | 1.3 / 32 µs (2026-10-28) | per frame: each circle blended between the last two snapshots by id (one merge walk, `snapshot::blend_vision`), before the view cull and the GPU binning. Frame cost (`FRAME_COST_VIEW=1 ... frame_cost_16k`, 1920x1080, 1010 circles): draw + GPU 3.30 -> 3.7-4.0 ms zoomed out over the whole map, 2.93-2.95 unchanged over one army; prepare unchanged (0.8 / 1.1-1.3 ms) |
| intel/update 16 armies x 1000 (a third moving) | 0.36 ms (2026-10-25; 0.43 before, was 1.83-2.08) | the 16-army stress shape on an 81 km map. Per-cell army cover masks per sensor make sightings one pass over the units for all armies (was per-army detection, 16 x 16k, which dominated); in the 16-army stress run Intel went 4.7 -> 1.2 ms/tick and Weapons 3.6 -> 2.9 (targeting's `detect` is a mask AND). 63-army stress (300 ticks): Intel 66.2 -> 4.7 ms/tick. Was 2.5-3.1 ms with per-cell stamps. 256-bit army masks (to allow 255 armies) measured 3.8-4.3 ms, so masks stay one u64 and sessions are capped at 63 armies (`MAX_ARMIES`) |
| avoidance/serial/1000 | 255 µs | 1000 movers packed in a crowd (every unit overlapping ~6 neighbours) among 1000 idle; index build ~50 µs, resolve ~180 ns per mover |
| avoidance/parallel/1000 | 237 µs | `PARALLEL_MIN` = 512; 2000: 612 -> 434 µs, 4000: 1.36 -> 0.77 ms. One coarse-grid query per mover was 12 ms (whole crowd in one 32-unit cell) |
| nav/path/cross-map 1024 | 123 µs (2026-10-07: cell hasher + binary node lookup; 141 µs) | sector graph (32-cell sectors, portals at each end of a border run): 2.0 ms with JPS, 10.7 ms plain A*. Portals every 8 cells along long runs: 544 µs for paths 2% shorter (4% -> 1.8% over optimal before smoothing), so not taken |
| nav/sector graphs 1024 (4 classes) | 28 ms (2026-10-07; ~69 ms on the previous criterion run: `components` scanned every sector's cells for its region count, twice) | building the 1x1 classes' graphs on the first movement tick, sectors in parallel (70 ms with portals every 8 cells) |
| nav/build grid 1024 | 12.2 ms | once at map load: every class's cell rule (20 classes) and the four 1x1 classes fitted, rows in parallel; was 3.5 ms with 4 fixed layers. `FlattenMapRect` recomputes only its area |
| snapshot/build 10k entities | 102 µs | windowed runs only (headless never builds snapshots); serial, under rayon's wake cost. Was 82 µs before the mesh index / scale (read from the cold half: ~8 µs; the rest the larger view, measured at load 32) |
| snapshot/build dynamic, 5k props + 1k units | 14.7 µs | windowed only: what a snapshot builds per tick on a map-like world. Props (~85% of a map's entities) go to the renderer once, then only changed ones (`EntityWorld::take_prop_changes`); the renderer merges them back in by id. In a 2-AI game the Snapshot stage went 240 -> 34 µs/tick at max speed (with skipping snapshots the channel can't take). Building views in parallel measured 3x slower in game (rayon wake + unindexed collect) |
| models/read_mesh uel0201 | 4.7 µs | renderer, on the global pool, once per mesh |
| scmap/read_terrain scmp_009 | 1.0 ms | sim, once per map |
| shadows (render, `shadow_cost` test) | +0.1-0.3 ms GPU, +0.05 ms CPU per frame | 1080p, SCMP_009 base, 5400 entities, 3 cascades (20 / 124 / 1327 casters after culling); 3.9 ms before culling |
| particles (render, Tracy) | 63-army air battle, camera on the dogfight, ticks 100-250: 2.1 ms mean / 1.5 ms median per frame on a quiet machine (11 / 6 ms on a loaded one, where develop's CPU quad builder took 137 / 160 ms); `dev/scenes/visual/effects.lua`-scale scenes are under 1 ms | `dev/scenes/stress/air.lua` (Tracy capture: docs/guides/running.md); render-thread only, not a criterion bench (bin crate). GPU-evaluated particles and trails since 2026-10-11: the CPU only emits, in parallel, on frames with a new sim tick |
| scmap/read_render scmp_009 | 1.2 ms | renderer, on the global pool, once per map: + strata, lighting, 2236 decals, blob copies |
| snapshot/poses 3000 manipulators | 73 µs | windowed only, per tick: gather by owner (sort) + attach to 10k views |
| snapshot/poses 100 ships with slaves | 146 µs (2026-10-09; 3000-manipulator row 150 µs on the same run) | windowed only, per tick: 100 units of 24 turning manipulators + 6 slaves (`CreateSlaver`, rack bones slaved to a pitch bone) each; slaves resolved per owner run, costing no more than plain rotators |
| snapshot/poses 500 posed wrecks | 48.7 µs (2026-10-29; the 3000-manipulator row 227 µs on the same loaded run) | windowed only, per tick: wrecks holding their unit's death pose (`TryCopyPose`), 20 posed bones each, as `Place` ops (`snapshot::hidden_bones`). Only bones away from rest are kept, so a wreck of a unit that died at rest costs nothing |
| movies/decode frame/main_menu | 1.48 ms (2026-10-06) | 1824x1024 MPEG-1 (pl_mpeg, C at -O3), decoded and its planes copied out, on the movie's own thread: ~4.4% of a core at 30 fps. The render thread only uploads the planes (2.8 MB per new frame) |
| movies/decode frame/Abasi | 199 µs | 192x192 talking head (29.97 fps) |
| audio/mix block, music + 32 world cues | 17.7 µs (2026-10-06) | audio thread, not the sim: one 256-frame block (5.3 ms of sound at 48 kHz) with streamed music and 26 cues left after instance limits; ~0.3% of a core |
| pose/evaluate walker | 185 ns | render thread, per animated instance per frame: 10 bones, walk clip + aimed turret (1000 walkers ~0.2 ms/frame) |
| pose/sim bone walker | 526 ns | sim, per projectile spawned / `GetPosition(bone)`: deepest bone of the walker, fixed point, walk clip + 2 rotators among 1000 manipulators (no clip data: 379 ns; no manipulators: 101 ns; the rest is CORDIC sin/cos for the rotators). Small next to the Lua side of `CreateProjectile` |
| pose/aim base walker | 594 ns (2026-10-30; was `clip aim base` 341 ns + the muzzle's `sim bone walker` 526 ns) | sim, per tick per weapon with a target (`pose::aim_base`): one walk down the muzzle's chain with the clips and the manipulators before the weapon's aim (its yaw frame, pitch tilt, muzzle), then the rest (the muzzle as finally posed) |
| pose/aim base turret on turret | 565 ns (2026-10-30) | the same on a turret under another weapon's aim (the Cybran ACU's laser on its torso), no clips |
| models/dds 512 dxt1 compressed | 2.2 µs | BC GPUs: levels copied as stored |
| models/dds 512 dxt1 decoded | 539 µs | non-BC GPUs: decoded on the global pool, never the render thread |
| checksum/10k entities | 114 µs (2026-10-28; was 78.7 on the same Mac) | XXH3 over one packed buffer (FNV-1a byte-wise was 844 µs), now in parts (entities, motion, orders, projectiles, weapons, economy, rng, mods) and run every tick of a networked game. The rise is reading each unit's motion state (speed, course, spin: one boxed `UnitData` per unit, a cache miss each); a second pass over the entities and a fresh 1.5 MB buffer per call (125 µs) were folded into one pass and a reused thread-local buffer. 0.1% of a 100 ms tick; too small to gain from rayon |
| manipulators/step/1000 animators | 2.5 µs (2026-10-25; 2.3 before speed pacing) | per-tick animator advance |
| manipulators/step/1000 speed-aligned | 33.4 µs | walk cycles (`CreateAnimator(unit, true)`): ~33 ns each over two passes (turn check before movement, advance after: an owner lookup in each, fixed sqrt of speed and climb) |
| lua_call/VDist3 x1000 | 23.8 µs | Luau shim |
| lua_call/VDist3Native x1000 | 75.9 µs | native callback (3.2x slower) |
| lua_interrupt/1000 calls (none / mlua `set_interrupt` / raw) | 6.9 / 17.9 / 9.3 µs | stall watchdog hook: worst case (2000 trivial safe points); mlua's trampoline is 2.6x, so the engine installs a raw C interrupt that reads one atomic (~1.2 ns per safe point). Real ticks: no measurable difference (0.70 ms/tick either way, 3000-tick headless release run) |
| Lua GC cycle (whole, stop-the-world; 4v4 soak, release) | 40-60 ms at 140-180 MB sim heap, ~every 25 game-s; 0.07 ms at startup sizes | measured with a temporary trace in `luaC_step` (2026-10-04). Whole cycles keep GC timing out of the sim (docs/architecture/determinism.md); average tick cost unchanged (13.8 vs 14.2 ms at tick 9000) |
| manipulators/slider step/1000 moving | 15.6 µs | ~15 ns per moving slider (fixed sqrt for the distance); finished sliders return at once (`manipulators/step` with 1000 animators: 2.0 µs) |
| manipulators/thrust step/1000 pods | 164 µs (2026-10-06) | UEF engine pods on aircraft whose motion changes every tick (~164 ns each: fixed-point normalise, shortest-arc and capped turn); per tick after movement |
| manipulators/thrust step/1000 pods hovering | 8.0 µs | the usual case: motion unchanged and the pod already there, so it returns early (~8 ns) |
| manipulators/thrust scan/100k without pods | 1 ns | the pass visits only thrust controllers (an index); walking all manipulators was 700 µs |
| spatial/props circle r8 (5182 props) | 102 ns | auto-reclaim scan per engineer (every 5 ticks), props grid separate from units |

## Typed Lua API (Apple M-series, release, 2026-10-15)

`cargo bench -- lua_api` (`benches/engine.rs`), measured from the Lua side so the
same bench ran before (`fa41173`) and after the typed-API conversion
(`feature/lua-api-types`), back to back on a quiet machine:

| bench | before | after | note |
|---|---|---|---|
| lua_api/register sim API | 176 us | 284 us | +108 us once per VM: naming each of ~1100 native functions (`lua_setcdebugname` via `exec_raw`) and recording it |
| lua_api/register user API (UserVm::new) | 1.35 ms | 1.44 ms | same, user VM |
| lua_api/unit methods x1000 | 273 us | 275 us | `self` as `Obj<moho::Unit>`: a table check, as before (+0.9%, noise) |
| lua_api/unit positions x1000 (GetPosition, GetPositionXYZ, Warp) | 517 us | 454 us | `shapes::Vector` / `Bone` / `Obj<moho::Entity>` parameters and returns: newtypes over the same `LuaTable` / `LuaValue`, no extra work (the -7% is noise: the untouched unit-methods bench moved +2% in the same run) |
| lua_api/run_script x1000 (found, 2 args) | 170 us | 171 us | typed `UnitCallbacks::*` are `run_script` underneath |
| lua_api/run_script x1000 (missing) | 63 us | 63 us | |
| lua_api/ui update_units 2000 (20 with detail) | 306 us | 307 us | `#[derive(LuaTable)]` writes: fields go straight into the table (a first version that built a `LuaValue` per field was +10%) |
| lua_api/ui update_units 2000 selected (details unchanged) | 2.64 ms | 921 us | 2026-10-23: a selected unit's detail tables (queue, economy, stats, missiles, cargo, assisters, restrictions) are only made again when that part changed since the last write (`WrittenDetails`), and the per-beat scalar fields' keys are made once per call. With 6k aircraft selected the beat's `selection refresh` was 15.5 ms |
| lua_api/ui EntityCategoryFilterDown 2000 UserUnits | 597 us | 363 us | 2026-10-23: UserUnits resolve through their unit data (`__engine_unit_data[_id].bp`, raw reads) instead of `Blueprint` / `GetUnitId` through the class metatable and a Lua call per unit. FAF's `OnSelectionChanged` filters the whole selection several times: with 6k aircraft selected the filter's samples halved |
| lua_api/ui GetUnitCommandData 2000 UserUnits (9 blueprints) | 1.77 ms | 294 us | 2026-10-23: native, each blueprint read once (its caps and parsed `BuildableCategory`), a unit only marking its blueprint used unless it has an upgrade queued or a restriction; the Luau shim walked every unit's blueprint tables and re-parsed its categories. FAF's `OnSelectionChanged` calls it twice per selection change (2 x 4.5 ms with 6k aircraft selected) |
| lua_api/OnIntelChange 40k changes | 8.95 ms | 5.14 ms | 2026-10-26: worst-case intel callbacks (a 63-army air battle with fog makes ~40k a tick): 20k units gaining then losing LOS and radar for one enemy army, a brain with FAF's jammer handler. An army's blip of a unit is now one object of FAF's `Blip` class, cached weakly until a script stores a field on it (`api::blip`), instead of a new table and userdata per call. Keeping every blip alive instead (moho's lifetime, strongly) would add ~22 µs per 1k blips to each full collection (1M blips: 22 ms, 256 MB): Lua garbage 9.7 MB a tick -> 0 (5.8 MB on the tick after a collection, rebuilding the blips nothing kept) |
| lua_api/OnIntelChange 40k changes x 100 ticks + collection | 1.024 s | 512 ms | the same at the sim's cadence: 100 ticks, then the full collection (`sim::gc::EVERY`), all timed |
| lua_api/debug.allocatedrsize nav mesh 48k leaves | 139 ms | 9.1 ms | 2026-10-30: FAF's Lua walk (`/lua/system/debug.lua`, before) against the engine's native one on the raw Luau stack (after), swapped in by an engine hook of that file. FAF sums its nav mesh with it after generating it, only for a log line: on Seton's Clutch with 8 AIs (release) the report went 588 -> 10 ms with the same totals (18.0 MB, 0.129654 MB). A faster hasher for the seen set made no measurable difference |

64k stress (`dev/scenes/stress/{land,air}.lua` on `stress_63`, 300 ticks, headless, 3
alternating rounds per binary): land 284.1 -> 281.0 ms/tick total, air 155.0 ->
155.1; every stage within noise, and the scenario's per-tick progress lines were
identical across all 12 runs.

## Interface (Apple M-series, release, 2026-10-27)

`cargo bench -- interface` (`benches/engine.rs`): FAF's skirmish lobby at 1920x1080
with its Unit Manager open (~3400 drawn controls). Before is the develop branch at the time,
after is, back to back (the machine was busy: load ~13).

| bench | before | after | note |
|---|---|---|---|
| interface/draw list, Unit Manager open | 1.47 ms | 343 us | layouts are kept between frames and read again only when they go dirty (a watcher LazyVar per control, `LAYOUT_SRC`), in one Luau call; bitmaps and texts build their items without copying the control. Reading every control's five edges each frame was ~0.3 us a control even with all of them cached. In the game the interface's share of a frame went 3.7 -> 1.7 ms |
| interface/mouse move, Unit Manager open | 832 us | 464 us | the hit test reads the same kept layouts; the rest is FAF's own enter / exit handlers |

## Stress runs (Apple M-series, release, 2026-10-03)

`dev/scenes/stress` (see `docs/guides/running.md`): 1000 land units per army on SCMP_030,
one army spawned per tick, then everyone attack-moves to the centre.
Headless, `--profile-stages`; Tracy traces from `tracy-capture`.

| Run | ms/tick | Top stages (ms/tick) | Notes |
|---|---|---|---|
| 16 x 1000, 1200 ticks | 23.3 (tick 1000, 47k entities) | Movement 9.4, Intel 3.5, Reclaim 3.2, Weapons 2.7, Construction 1.8 | 2.3x slower than real time; 3.5 GB peak RSS; 29 s wall |
| 63 x 1000, 600 ticks | 214 mean | Movement 59, Intel 52, Reclaim 51, LuaThreads 17, Weapons 10 | 131 s wall; one 10.4 s tick (Movement 9.3 s), the tick 63k units get their attack-move paths |
| 128 x 1000, 600 ticks (before the 63-army cap) | 1496 mean | Intel 625, Reclaim 372, LuaThreads 158, Projectiles 139, Movement 125 | 10 GB peak RSS; 15 min wall. Intel is per army x per unit |

### Pathing baselines (2026-10-03, before any pathing optimisation)

`dev/scenes/stress/{stress,headon,city,crossing}.lua`, release, `--profile-stages`
(which now also prints the worst tick with its stages, and path searches
with their work: `nav::Search::work`, cells the search looked at).

Orders go in `AttackFormation` (as players give them; `StressFormation =
'none'` in a script gives plain orders, all units to one point). Each army
is its faction's T1-T3 tanks plus one experimental per 100 units (Fatboy,
Galactic Colossus, Monkeylord, Ythotha; `StressPerExperimental`).

| Scenario | Searches | Work / search | Worst tick | Peak RSS |
|---|---|---|---|---|
| converge: 16 x 1010 to the centre of SCMP_030 (4096) | 17.5k | 0.9M cells | 2.8 s | 3.6 GB |
| head-on: 2 x 2020 2400 apart, SCMP_030 | 4040 | 7.2M | 5.2 s | 3.7 GB |
| city: 2 x 1010 through 6.5k generators + 7.6k walls, SCMP_030 | 2020 | 32M | 12.2 s | 3.7 GB |
| crossing: Seton's Clutch (1024), 8 x 505 to the farthest start | 4040 | 0.42M | 317 ms | 1.2 GB |

After footprint classes (units path by `SpecFootprints` class; the big
amphibious classes ignore structures), worst ticks: converge 3.3 s, head-on
5.7 s, city 7.8 s (experimentals now cross it over the buildings: work
39G, was 64G), Seton's 373 ms. Peak RSS ~4.0 GB: region labels per class in
use (4 bytes per cell each) until the hierarchy replaces them.

After the sector graph (HPA*, replacing JPS and whole-map region labels;
search buffers now window-sized):

| Scenario | Worst tick | Path work | Peak RSS |
|---|---|---|---|
| converge (16 x 1010) | 455 ms (Movement 388 ms: 17.7k searches, ~22 µs each) | 0.33G (was 17.6G) | 2.3 GB (was 4.1) |
| head-on (2 x 2020) | 265 ms | 0.12G (was 28.9G) | 1.8 GB (was 4.1) |
| city (2 x 1010) | 287 ms | 0.08G (was 38.8G) | 2.1 GB (was 4.0) |
| Seton's crossing (8 x 505) | 97 ms | 0.08G (was 1.7G) | 1.3 GB |

After formation travel (one path per formation group; members steer for
their moving slots), with sector graphs refreshed every tick (built when a
class first appears, not on the next tick with orders):

| Scenario | Worst tick | Unit + centre searches | Movement on the order tick |
|---|---|---|---|
| converge (16 x 1010) | 230 ms, LuaThreads: FAF's `formations.lua` laying out 16 groups of 1010 (~14 ms each) | 4.3k (was 17.7k) | 0.4 ms |
| head-on (2 x 2020) | 194 ms at tick 1 (session start); the order tick is cheap | 1.4k | < 1 ms |
| city (2 x 1010) | 184 ms (LuaThreads; Movement 25 ms is a placement tick's sector rebuilds) | 0.4k | - |
| Seton's crossing (8 x 505) | 48 ms (LuaThreads) | 1.2k | < 1 ms |

Members' per-tick line checks (slot clamp + line of sight) raised average
Movement from 5.9 to 9.9 ms/tick in converge (16k members). Re-measured
2026-10-03 (`sample`, converge, 600 ticks, release): by then the line walks
were only ~3% of busy CPU samples (384 of 12.6k; sidestep look-ahead 11%,
avoidance resolve 12%, weapon target queries 18%, `Commands::current` 6%);
re-checking them every 4 ticks (staggered) cut that to 110 samples, Movement
20.8 -> 20.1 ms/tick. The rest of Movement is sidestep and push-apart.

After flow fields (plain orders to a point given to >= `FLOW_MIN` = 32
units share a field per (class, goal cell)), with the scripts' plain-order
variant (`StressFormation = 'none'`):

| Scenario (plain orders) | Searches + fields, before -> after | Order tick Movement |
|---|---|---|
| converge (16 x 1010) | 17.8k -> 499 | 219 ms -> ~24 ms (fields 14.2, steering 7.4, push-apart 2.2) |
| head-on (2 x 2020) | 4.0k -> 50 | < 1 ms |
| Seton's crossing (8 x 505) | 4.0k -> 72 | 64 ms -> < 10 ms |

The formation variant's order tick is ~23 ms of Movement too (centres 3.2,
members' steering 16.5, push-apart 2.4).

After the motion model and predictive sidestepping: average Movement per
tick converge 9.9 -> 13.2 ms (16k movers each look ahead for units in their
way), head-on 3.6 -> 4.5 ms; worst ticks unchanged (FAF's formation layout
in LuaThreads).

With the path budget (`PATH_TICK_WORK` = 12M cells per tick): converge
Movement 16.5 -> 16.9 ms/tick A/B against the previous commit under the
same load (load average ~10; the 13.2 above was measured on a quieter
machine). Worst ticks unchanged.

With the real game's formation pace (members steer for their slot at
the centre's next waypoint, so their line checks are longer), A/B against
develop, release, converge 600 ticks: Movement 22.9 -> 21.1 ms/tick,
searches 4138 -> 4000, worst tick unchanged (186 ms, `formations.lua`).

With the real game's pushes (impulses between moving units on
different commands, squad-mates stop and go; each index body looks up its
command), A/B against the formation-pace build, release, converge 600
ticks: Movement 21.0 -> 23.2 ms/tick (the index's command lookups and the
impulse resolve; folding the squad check into `sidestep`'s scan saved its
own query), worst tick unchanged.

Formations don't reduce the searches yet: each member searches its own
path to its slot (formation planning itself, 1000-2000 units through
`formations.lua`, doesn't show in the worst tick).

Findings: JPS cost is its line scans, not its expansions (~140 jump points
per converge path): on open ground every diagonal step scans both straights
to the next wall or the map edge, so work grows with map size, and in the
city every forced neighbour adds scans across the open land beyond it (more
cells than the 16M-cell grid per search). Smoothing is ~1% of it. The
per-thread `Search` buffers are sized to the whole grid (13 B/cell, ~200 MB
per thread at 4096^2): 1.4 GB RSS before the first search, 3.1 GB after.

Quadratic paths the 128-army run exposed (now indexed): spawn scanned
every manipulator and weapon per new unit (128k units took > 4 min to
spawn, now ~70 s); a death scanned every entity (attachments) and every
platoon's units; `SetMesh('')` re-read the VFS and warned each call.
Still open: the per-unit command lookups (`busy_units` + `current`)
scale linearly but show up at 100k units, and a single tick's worth of
path requests for a whole army stalls Movement.
