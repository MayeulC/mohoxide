//! A disk cache of textures whose missing mip levels were generated
//! ([`super::mips`]), so a texture pays for its levels once, not on every
//! load.
//!
//! Each entry is the whole texture as a `.dds` (stored plus generated
//! levels, [`super::dds::write`]), so a hit streams like any texture that
//! shipped with mips. It's named by a hash of [`key`]: the source's
//! fingerprint (`Vfs::fingerprint`: an archive entry's CRC, a loose file's
//! size and time, a map blob's bytes), its path, how it's sampled and
//! [`GENERATOR_VERSION`], so a changed source or generator just misses.
//!
//! Writes go to a temporary file renamed into place, so a crash or two
//! loaders racing never leave a partial entry under a real name. A hit
//! refreshes the entry's modified time (at most daily: the last-used time,
//! as access times are unreliable), and [`MipCache::prune`] (once a map's
//! textures have loaded and the player's [`Settings`] are known) deletes
//! entries unused for their age limit, then the oldest beyond their disk
//! budget. Render-only: none of this reaches the sim.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// Bump when generated levels change (filter, encoder), so old entries
/// stop matching.
pub const GENERATOR_VERSION: u64 = 1;

/// The player's texture cache options (the Options screen's Video tab:
/// `eng_TextureCacheMB`, `eng_TextureCacheDays`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Disk kept, MB: beyond it the least recently used entries go.
    pub budget_mb: u64,
    /// Entries unused this many days go.
    pub max_age_days: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self { budget_mb: 2048, max_age_days: 90 }
    }
}

impl Settings {
    pub fn max_bytes(self) -> u64 {
        self.budget_mb << 20
    }

    pub fn max_age(self) -> Duration {
        Duration::from_secs(self.max_age_days * 24 * 60 * 60)
    }
}

/// A hit refreshes an entry's time only if it's older than this.
const TOUCH_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// The cache key of a texture: `source` its fingerprint, `path` where it
/// came from, `srgb` / `bc` how it's sampled.
pub fn key(source: u64, path: &str, srgb: bool, bc: bool) -> u64 {
    let mut h = xxhash_rust::xxh3::Xxh3::new();
    h.update(&GENERATOR_VERSION.to_le_bytes());
    h.update(&source.to_le_bytes());
    h.update(&[srgb as u8, bc as u8]);
    h.update(path.as_bytes());
    h.digest()
}

pub struct MipCache {
    dir: PathBuf,
}

impl MipCache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn file(&self, key: u64) -> PathBuf {
        self.dir.join(format!("{key:016x}.dds"))
    }

    /// The entry for `key`, if there is one (its last-used time refreshed).
    pub fn get(&self, key: u64) -> Option<PathBuf> {
        let path = self.file(key);
        let modified = fs::metadata(&path).ok()?.modified().ok();
        if modified.and_then(|t| t.elapsed().ok()).is_none_or(|age| age > TOUCH_AFTER) {
            let _ = fs::File::options().write(true).open(&path).and_then(|f| f.set_modified(SystemTime::now()));
        }
        Some(path)
    }

    /// Drops a bad entry.
    pub fn remove(&self, key: u64) {
        let _ = fs::remove_file(self.file(key));
    }

    /// Stores `dds` (a whole `.dds` file) as `key`'s entry.
    pub fn put(&self, key: u64, dds: &[u8]) -> io::Result<()> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join(format!(".{key:016x}.{}.{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::write(&tmp, dds)?;
        fs::rename(&tmp, self.file(key)).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }

    /// Deletes entries last used before `now - max_age`, then the least
    /// recently used until the rest fit in `max_bytes`, and temporary files
    /// over a day old (a crashed write). Returns (entries removed, bytes
    /// kept).
    pub fn prune(&self, now: SystemTime, max_age: Duration, max_bytes: u64) -> io::Result<(usize, u64)> {
        let mut entries = Vec::new();
        let mut removed = 0;
        for e in fs::read_dir(&self.dir)? {
            let e = e?;
            let name = e.file_name();
            let name = name.to_string_lossy();
            let meta = e.metadata()?;
            let age = now.duration_since(meta.modified()?).unwrap_or_default();
            if name.ends_with(".tmp") {
                if age > TOUCH_AFTER && fs::remove_file(e.path()).is_ok() {
                    removed += 1;
                }
            } else if name.ends_with(".dds") {
                if age > max_age {
                    removed += fs::remove_file(e.path()).is_ok() as usize;
                } else {
                    entries.push((age, meta.len(), e.path()));
                }
            }
        }
        // Newest first: keep while they fit.
        entries.sort_by_key(|&(age, _, _)| age);
        let mut kept = 0;
        for (_, len, path) in entries {
            if kept + len <= max_bytes {
                kept += len;
            } else {
                removed += fs::remove_file(path).is_ok() as usize;
            }
        }
        Ok((removed, kept))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cache(name: &str) -> MipCache {
        let dir = std::env::temp_dir().join(format!("rts-mip-cache-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        MipCache::new(dir)
    }

    fn age(cache: &MipCache, key: u64, by: Duration) {
        let f = fs::File::options().write(true).open(cache.file(key)).unwrap();
        f.set_modified(SystemTime::now() - by).unwrap();
    }

    #[test]
    fn miss_then_hit() {
        let c = temp_cache("hit");
        assert!(c.get(1).is_none());
        c.put(1, b"DDS data").unwrap();
        assert_eq!(fs::read(c.get(1).unwrap()).unwrap(), b"DDS data");
        c.remove(1);
        assert!(c.get(1).is_none());
        let _ = fs::remove_dir_all(&c.dir);
    }

    #[test]
    fn key_changes_with_source_sampling_and_path() {
        let k = key(7, "/a.dds", false, true);
        assert_eq!(k, key(7, "/a.dds", false, true));
        for other in [key(8, "/a.dds", false, true), key(7, "/b.dds", false, true), key(7, "/a.dds", true, true), key(7, "/a.dds", false, false)] {
            assert_ne!(k, other);
        }
    }

    #[test]
    fn a_hit_refreshes_an_old_entry() {
        let c = temp_cache("touch");
        c.put(1, b"x").unwrap();
        age(&c, 1, Duration::from_secs(10 * 24 * 3600));
        c.get(1).unwrap();
        let since = fs::metadata(c.file(1)).unwrap().modified().unwrap().elapsed().unwrap();
        assert!(since < TOUCH_AFTER);
        let _ = fs::remove_dir_all(&c.dir);
    }

    #[test]
    fn prune_drops_old_entries_then_the_least_recently_used() {
        let c = temp_cache("prune");
        for (k, days) in [(1, 100), (2, 10), (3, 5), (4, 1)] {
            c.put(k, &[0; 10]).unwrap();
            age(&c, k, Duration::from_secs(days * 24 * 3600));
        }
        fs::write(c.dir.join(".5.tmp"), b"partial").unwrap();
        let f = fs::File::options().write(true).open(c.dir.join(".5.tmp")).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(2 * 24 * 3600)).unwrap();
        // 1 is past the age limit; of the rest only two fit in 20 bytes.
        assert_eq!(c.prune(SystemTime::now(), Settings::default().max_age(), 20).unwrap(), (3, 20));
        assert!(c.get(1).is_none() && c.get(2).is_none());
        assert!(c.get(3).is_some() && c.get(4).is_some());
        assert!(!c.dir.join(".5.tmp").exists());
        let _ = fs::remove_dir_all(&c.dir);
    }
}
