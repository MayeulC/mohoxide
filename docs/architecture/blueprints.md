# Blueprints

A blueprint is a Lua table describing one kind of thing: a unit (`uel0101`), projectile, prop, mesh,
emitter, trail or beam. Its fields (`Physics.MaxSpeed`, `Weapon[].RateOfFire`, `Economy.BuildTime`,
...) drive both FAF's scripts and the engine. FAF's wiki explains what each field means
(`../faf-wiki/Development/Modding/Blueprints.md`).

## Loading: FAF's Lua does it, the engine registers

As in moho, loading is FAF's Lua (`/lua/system/Blueprints.lua` and the `blueprints-*.lua` files next
to it). The engine runs it unchanged during start-up ([game-start-sequence.md](game-start-sequence.md)):

1. Every `.bp` file under `/units`, `/projectiles`, `/props`, `/env`, `/meshes` and `/effects`
   runs (through the normal script path: [lua-bridge.md](lua-bridge.md#loading-scripts)), calling
   `UnitBlueprint{...}` and friends, which store the table.
2. Mods' blueprints replace or merge into them.
3. FAF post-processes everything: mesh blueprints split out of units, categories, threat values,
   LOD cutoffs, weapon defaults, SCU presets, and mods' `ModBlueprints` hooks.
4. `RegisterAllBlueprints` calls the engine's `Register<Kind>Blueprint(bp)` once per blueprint, in
   sorted order.

Everything up to step 4 is plain Lua table work. The engine's part starts at registration.

## Registration (`src/engine/api/blueprints.rs`)

`Register*Blueprint` does what moho's does, as far as FAF's scripts can see:

- **Defaults.** Fields moho fills in from its C++ structs (`UNIT_DEFAULTS`: vision radius, build
  distance, the `Transport` block, `UpgradesFrom = 'none'`, ...) are added to the table, because
  FAF's scripts read them unguarded. Each default names its source in the code; ones that only the
  interface needs are added in the interface VM only, so a guess can't change the sim.
- **Copies.** Unit, projectile and prop tables are copied, as moho's sim and user states hold copies,
  not the tables FAF built. It matters more than it looks: moho's copy walks each table with `next`,
  so keys come out in a different order from the `.bp` file, and FAF lays out some of the interface
  in `pairs` order (the enhancement tabs read right arm, back, left arm in the real game). Our copy
  does the same.
- **`__blueprints`.** Each registered table is reachable from Lua through `__blueprints`, by id and
  by registration number, which is what `GetBlueprint` and FAF's code read.
- **The registry.** Rust keeps a handle to each table in a `BTreeMap` per kind (`BlueprintRegistry`),
  so iteration order is the same on every machine.

## How the engine reads blueprints

The engine doesn't parse every field into Rust structs up front. Each system reads the fields it
uses from the registered table, once, into its own typed data, in fixed point for anything the sim
uses:

- unit motion limits at spawn (`api/unit.rs` `motion_of`), weapons when a unit is made
  (`sim/weapon.rs`), projectile physics on first use (`sim/projectile.rs` `ProjectileTemplate`),
  threat values (`sim/threat.rs`), footprints for pathfinding (`sim/nav.rs`);
- the renderer gets what it draws (meshes, icons, LODs, strategic icon names) through the first
  snapshot, never by reading Lua.

So adding support for a field means reading it where it's used, not extending a central struct.
Before implementing one, check how the real engine reads it (its default, its units): the
blueprint-field audit lists the fields moho reads that we don't ([audits.md](../guides/audits.md#blueprint-fields)),
and [verifying-against-the-binary.md](../research/verifying-against-the-binary.md) has the method.

## Ids

| Kind | Id | Example |
|---|---|---|
| Unit | the file name, lower case | `uel0101` |
| Projectile, prop, emitter, trail, beam | the full path, lower case | `/projectiles/tdfgauss01/tdfgauss01_proj.bp` |
| Mesh | the full path without `.bp` | `/units/uel0101/uel0101_mesh` |

Units use short ids so FAF's code can name them directly (`CreateUnitHPR('uel0101', ...)`).

## Interface VM

The interface's Lua state loads blueprints too, as moho's user state does, so FAF's interface can
read them (`GetBlueprint`, tooltips). It's a separate copy: nothing the interface does to a blueprint
reaches the sim.
