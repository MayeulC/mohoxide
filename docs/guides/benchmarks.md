# Benchmarks

## Criterion benches

`benches/engine.rs` holds the [criterion](https://github.com/bheisler/criterion.rs) benches for the hot paths: per-tick and per-unit sim code, and the startup pipeline.

```sh
cargo bench                          # every bench (release build; takes a while)
cargo bench -- scheduler             # only benches whose name contains "scheduler"
cargo bench -- 'sim/tick'            # a group, by its name prefix
FAF_REPO=/path/to/fa cargo bench     # FAF's Lua somewhere other than ../fa
```

The script and blueprint benches read FAF's Lua from `../fa` (or `FAF_REPO`), and a few read the game data beside the repo. Without them, those benches fail, naming the missing path.

Every bench function is listed in `criterion_group!` at the end of the file. Bench names start with their area (`scripts/compile Unit.lua`, `movement/...`, `sim/tick/1000 threads + sync`), and that name is what the filter matches.

### Comparing a change

Criterion compares each run with the previous one on the same machine (history in `target/criterion`). To compare a change properly, save a baseline from `develop` first:

```sh
git switch develop && cargo bench -- movement --save-baseline develop
git switch my-branch && cargo bench -- movement --baseline develop
```

Criterion then reports each bench's change and whether it's significant. Numbers only mean something against a baseline from the same machine. Keep the machine quiet while it runs (check `uptime`): a busy machine moves results by 10% or more, in both directions.

### Adding a bench

Add or extend a bench whenever you add or change per-tick, per-unit or startup code (see [CONTRIBUTING.md](../../CONTRIBUTING.md)), and add its function to `criterion_group!` at the end of `benches/engine.rs`. Don't bench one-off setters. `cargo check --benches` (part of the pre-merge checks) makes sure the benches still compile.

## Whole-game runs

Criterion times one function at a time. For the cost of a whole game, run a headless game in release. Its log has a `stage ms` line every 1000 ticks with each sim stage's ms/tick, the worst tick and its stages, and the path searches:

```sh
cargo run --release -- --headless --no-audio --ticks 3000
grep "stage ms" game.log
```

- **Stress maps** (`dev/scenes/stress/`: thousands of units per army, head-on battles, a city of structures): commands in [running.md](running.md#stress-maps-manual).
- **Long AI games**: [running.md](running.md#long-ai-games-manual).
- **Where the time goes inside a tick or a frame**: Tracy ([running.md](running.md#profiling-with-tracy)).

Compare runs the same way as the benches: the same command on the same machine, before and after, ideally alternating the two builds.

Results recorded during development are in [docs/archive/benchmark-results.md](../archive/benchmark-results.md).
