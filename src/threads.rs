//! How many cores the engine sizes its thread pools for.

/// The cores to size thread pools for: the machine's, or `RTS_CORES` when
/// set (to try a small machine's pool sizes on a big one; pair it with
/// load on the other cores to get its contention too, docs/guides/running.md).
pub fn cores() -> usize {
    static CORES: std::sync::LazyLock<usize> = std::sync::LazyLock::new(|| {
        let machine = std::thread::available_parallelism().map_or(1, |n| n.get());
        std::env::var("RTS_CORES").ok().and_then(|v| v.parse::<usize>().ok()).filter(|&n| n > 0).unwrap_or(machine)
    });
    *CORES
}

/// Stack size of the global pool's workers. Besides the sim's stages they
/// translate and compile scripts at startup, which recurses through
/// full_moon's AST (deepest in debug builds); rayon's default is 2 MiB.
/// Only the pages a thread touches are committed.
const GLOBAL_STACK_BYTES: usize = 32 * 1024 * 1024;

/// Builds rayon's global pool: one worker per core ([`cores`]), named
/// ("worker N") in the OS and in Tracy so the parallel stages show on their
/// own tracks. The sim's stages and startup's background work (script
/// compiles, VFS reads) share it. Call once, early (after the Tracy layer
/// is installed).
pub fn init_global_pool() {
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(cores())
        .stack_size(GLOBAL_STACK_BYTES)
        .thread_name(|i| format!("worker {i}"))
        .start_handler(|i| crate::engine::tracy::name_thread(&format!("worker {i}")))
        .build_global();
}
