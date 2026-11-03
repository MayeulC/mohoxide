//! Memory figures for the status bar: the process's resident memory, and
//! the GPU memory the game holds (Metal: the device's allocated size; other
//! backends: wgpu's buffer and texture byte counters).

use std::time::{Duration, Instant};

/// How often the figures are refreshed.
const PERIOD: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct MemorySample {
    /// Resident memory of the process (bytes), if the OS reports it.
    pub ram: Option<u64>,
    /// GPU memory in use by the game (bytes), if the backend reports it.
    pub vram: Option<u64>,
    taken: Option<Instant>,
}

impl MemorySample {
    /// Refreshes the figures if they're older than [`PERIOD`].
    pub fn refresh(&mut self, device: &wgpu::Device) {
        if self.taken.is_some_and(|t| t.elapsed() < PERIOD) {
            return;
        }
        self.taken = Some(Instant::now());
        self.ram = memory_stats::memory_stats().map(|m| m.physical_mem as u64);
        self.vram = vram(device);
    }
}

#[cfg(target_os = "macos")]
fn vram(device: &wgpu::Device) -> Option<u64> {
    use objc2_metal::MTLDevice;
    // SAFETY: only reads the device's allocated size; the hal device isn't
    // kept past this call.
    let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }?;
    Some(hal.raw_device().currentAllocatedSize() as u64)
}

#[cfg(not(target_os = "macos"))]
fn vram(device: &wgpu::Device) -> Option<u64> {
    let c = device.get_internal_counters().hal;
    let bytes = c.buffer_memory.read() + c.texture_memory.read();
    (bytes > 0).then_some(bytes as u64)
}

/// `bytes` as MB under a GB, else GB with one decimal.
pub fn format_bytes(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb < 1024.0 { format!("{mb:.0} MB") } else { format!("{:.1} GB", mb / 1024.0) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn bytes_read_as_mb_then_gb() {
        assert_eq!(super::format_bytes(512 * 1024 * 1024), "512 MB");
        assert_eq!(super::format_bytes(3 * 1024 * 1024 * 1024 / 2), "1.5 GB");
    }
}
