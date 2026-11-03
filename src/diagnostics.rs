//! What goes in the log for a player's bug report: the machine it ran on
//! (at startup) and any crash (a panic, with where it happened and a
//! backtrace). The GPU and its driver are logged by the renderer when it
//! picks one (`render::state`).

/// Logs the engine build, OS, CPU, cores and memory.
pub fn log_system() {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
    let cpu = sys.cpus().first().map_or_else(|| "unknown".to_string(), |c| c.brand().trim().to_string());
    let cores = std::thread::available_parallelism().map_or(0, |n| n.get());
    let pools_for = crate::threads::cores();
    let gib = |bytes: u64| format!("{:.1} GiB", bytes as f64 / (1u64 << 30) as f64);
    tracing::info!(
        build = concat!(env!("CARGO_PKG_VERSION"), "+", env!("RTS_BUILD_ID")),
        os = %System::long_os_version().unwrap_or_else(|| std::env::consts::OS.to_string()),
        arch = std::env::consts::ARCH,
        cpu = %cpu,
        threads = cores,
        pools_for,
        ram = %gib(sys.total_memory()),
        ram_free = %gib(sys.available_memory()),
        "system"
    );
}

/// Panics go to the log (message, where, backtrace) as well as the
/// console: a player who double-clicked the program never sees the
/// console, and the log is what they send.
pub fn log_panics() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let backtrace = std::backtrace::Backtrace::force_capture();
        tracing::error!(target: "panic", thread = thread.name().unwrap_or("?"), "crashed: {info}\n{backtrace}");
        previous(info);
    }));
}

/// Counts slow frames or ticks and logs them for bug reports without
/// flooding the log: a summary at most every 10 s (how many, the worst and
/// when), and at once any that froze for a second or more.
pub struct Hitches {
    what: &'static str,
    since: std::time::Instant,
    count: u32,
    worst: (f64, u64),
}

impl Hitches {
    pub fn new(what: &'static str) -> Self {
        Hitches { what, since: std::time::Instant::now(), count: 0, worst: (0.0, 0) }
    }

    /// One took `ms` when it had `budget_ms`; `at` says when (a tick).
    pub fn note(&mut self, ms: f64, budget_ms: f64, at: u64) {
        if ms > budget_ms {
            self.count += 1;
            if ms > self.worst.0 {
                self.worst = (ms, at);
            }
            if ms >= 1000.0 {
                tracing::warn!(what = self.what, ms = format!("{ms:.0}"), budget_ms = format!("{budget_ms:.0}"), at, "hitch: froze");
            }
        }
        if self.since.elapsed().as_secs() >= 10 {
            if self.count > 0 {
                tracing::warn!(what = self.what, count = self.count, worst_ms = format!("{:.0}", self.worst.0), worst_at = self.worst.1, budget_ms = format!("{budget_ms:.0}"), "hitch: slow in the last 10 s");
            }
            *self = Hitches::new(self.what);
        }
    }
}
