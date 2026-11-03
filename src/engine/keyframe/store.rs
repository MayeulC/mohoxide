//! Keyframes on disk: one file per keyframe in a folder per replay.
//!
//! A *full* keyframe is the keyframe compressed with zstd on its own; a
//! *diff* is compressed against the keyframe before it (zstd's patch-from:
//! the previous keyframe's bytes as a prefix, long-distance matching on),
//! which a minute later in a game is mostly the same bytes: a 16-army game's
//! keyframe is ~13 MB full and well under 1 MB as a diff. Reading a diff
//! needs the chain back to the last full keyframe, so a full one is written
//! every [`Store::full_every`] ticks to bound that chain.
//!
//! Files are `<tick>-<base>.kf` (`base` = the tick it is a diff from, its own
//! tick when full), 10 digits each, so a listing sorts by tick. Each is
//! written to a temporary name and renamed: a reader (the viewer, while a
//! run-ahead process writes) never sees half a file, and two writers of the
//! same keyframe leave one whole file.

use std::path::{Path, PathBuf};

use super::Keyframe;

const MAGIC: &[u8; 4] = b"RKF1";

/// zstd level: level 9 saves ~8% at 6x the time (`docs/architecture/replays.md`).
const LEVEL: i32 = 3;

/// A keyframe file: its tick, and the tick it is a diff from (its own when
/// full).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entry {
    pub tick: u64,
    pub base: u64,
}

impl Entry {
    pub fn is_full(self) -> bool {
        self.base == self.tick
    }

    fn file_name(self) -> String {
        format!("{:010}-{:010}.kf", self.tick, self.base)
    }

    fn parse(name: &str) -> Option<Self> {
        let stem = name.strip_suffix(".kf")?;
        let (tick, base) = stem.split_once('-')?;
        Some(Self { tick: tick.parse().ok()?, base: base.parse().ok()? })
    }
}

/// A replay's keyframe folder.
pub struct Store {
    dir: PathBuf,
    /// Ticks between full keyframes.
    pub full_every: u64,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>, full_every: u64) -> Self {
        Self { dir: dir.into(), full_every }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The keyframes on disk, by tick.
    pub fn entries(&self) -> Vec<Entry> {
        let mut out: Vec<Entry> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| Entry::parse(&e.file_name().to_string_lossy()))
            .collect();
        out.sort();
        out
    }

    /// The keyframe at or before `tick` whose chain is whole, if any.
    pub fn latest_at_or_before(&self, tick: u64) -> Option<Entry> {
        let entries = self.entries();
        let have: std::collections::BTreeSet<u64> = entries.iter().map(|e| e.tick).collect();
        entries.iter().rev().filter(|e| e.tick <= tick).find(|e| self.chain(&entries, **e, &have).is_some()).copied()
    }

    /// The entries to read, oldest first, to rebuild `entry`: back to a
    /// full keyframe.
    fn chain(&self, entries: &[Entry], entry: Entry, have: &std::collections::BTreeSet<u64>) -> Option<Vec<Entry>> {
        let mut chain = vec![entry];
        let mut at = entry;
        while !at.is_full() {
            if !have.contains(&at.base) {
                return None;
            }
            at = *entries.iter().find(|e| e.tick == at.base)?;
            chain.push(at);
        }
        chain.reverse();
        Some(chain)
    }

    /// The tick of the full keyframe `entry`'s chain starts from.
    pub fn full_of(&self, entry: Entry) -> Option<u64> {
        let entries = self.entries();
        let have = entries.iter().map(|e| e.tick).collect();
        self.chain(&entries, entry, &have).and_then(|c| c.first().map(|e| e.tick))
    }

    /// Whether a keyframe for `tick` should be full (none before it to diff
    /// from, or the last full one is `full_every` ticks back).
    pub fn wants_full(&self, tick: u64, previous: Option<(u64, u64)>) -> bool {
        match previous {
            None => true,
            Some((_, last_full)) => tick >= last_full + self.full_every,
        }
    }

    /// Writes `keyframe`, as a diff from `previous` (its tick and bytes,
    /// [`encode`]d) unless it should be full. Returns its bytes for the next
    /// write's `previous`, and its entry.
    pub fn write(&self, keyframe: &Keyframe, previous: Option<(&Entry, &[u8])>, last_full: Option<u64>) -> std::io::Result<(Vec<u8>, Entry)> {
        let raw = encode(keyframe)?;
        let full = previous.is_none() || self.wants_full(keyframe.tick, previous.map(|(e, _)| (e.tick, last_full.unwrap_or(0))));
        let (entry, body) = match previous.filter(|_| !full) {
            Some((prev, prev_raw)) => (Entry { tick: keyframe.tick, base: prev.tick }, compress_from(&raw, prev_raw)?),
            None => (Entry { tick: keyframe.tick, base: keyframe.tick }, compress(&raw)?),
        };
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(entry.file_name());
        let tmp = self.dir.join(format!(".{}.{}.tmp", entry.file_name(), std::process::id()));
        let mut file = Vec::with_capacity(body.len() + MAGIC.len());
        file.extend_from_slice(MAGIC);
        file.extend_from_slice(&body);
        std::fs::write(&tmp, &file)?;
        std::fs::rename(&tmp, &path)?;
        Ok((raw, entry))
    }

    /// Reads the keyframe `entry` (following its chain back to a full one).
    pub fn read(&self, entry: Entry) -> std::io::Result<Keyframe> {
        let entries = self.entries();
        let have = entries.iter().map(|e| e.tick).collect();
        let chain = self.chain(&entries, entry, &have).ok_or_else(|| std::io::Error::other(format!("keyframe {}: its chain is broken", entry.tick)))?;
        let mut raw: Vec<u8> = Vec::new();
        for e in chain {
            let file = std::fs::read(self.dir.join(e.file_name()))?;
            let body = file.strip_prefix(MAGIC.as_slice()).ok_or_else(|| std::io::Error::other("not a keyframe file"))?;
            raw = if e.is_full() { decompress(body)? } else { decompress_from(body, &raw)? };
        }
        decode(&raw)
    }

    /// The folder's size in bytes.
    pub fn size(&self) -> u64 {
        std::fs::read_dir(&self.dir).into_iter().flatten().flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()
    }
}

/// A keyframe as bytes.
pub fn encode(keyframe: &Keyframe) -> std::io::Result<Vec<u8>> {
    postcard::to_allocvec(keyframe).map_err(std::io::Error::other)
}

pub fn decode(raw: &[u8]) -> std::io::Result<Keyframe> {
    postcard::from_bytes(raw).map_err(std::io::Error::other)
}

fn workers() -> u32 {
    std::thread::available_parallelism().map_or(1, |n| n.get().min(8) as u32)
}

fn compress(raw: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut c = zstd::bulk::Compressor::new(LEVEL)?;
    c.set_parameter(zstd::zstd_safe::CParameter::NbWorkers(workers()))?;
    c.compress(raw)
}

fn decompress(body: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut d = zstd::stream::Decoder::new(body)?;
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut d, &mut out)?;
    Ok(out)
}

/// zstd's window must span the prefix and the data.
fn window_log(prefix: usize, data: usize) -> u32 {
    let need = (prefix + data).max(1 << 20) as u64;
    (64 - (need - 1).leading_zeros()).clamp(20, 31)
}

fn compress_from(raw: &[u8], prefix: &[u8]) -> std::io::Result<Vec<u8>> {
    use zstd::zstd_safe::CParameter;
    let mut c = zstd::bulk::Compressor::new(LEVEL)?;
    c.set_parameter(CParameter::NbWorkers(workers()))?;
    c.set_parameter(CParameter::WindowLog(window_log(prefix.len(), raw.len())))?;
    c.set_parameter(CParameter::EnableLongDistanceMatching(true))?;
    c.context_mut().ref_prefix(prefix).map_err(|e| std::io::Error::other(zstd::zstd_safe::get_error_name(e)))?;
    c.compress(raw)
}

fn decompress_from(body: &[u8], prefix: &[u8]) -> std::io::Result<Vec<u8>> {
    use zstd::zstd_safe::{DCtx, DParameter, InBuffer, OutBuffer};
    let err = |e| std::io::Error::other(zstd::zstd_safe::get_error_name(e));
    let mut d = DCtx::create();
    d.set_parameter(DParameter::WindowLogMax(31)).map_err(err)?;
    d.ref_prefix(prefix).map_err(err)?;
    let mut out: Vec<u8> = Vec::with_capacity(prefix.len() + prefix.len() / 4);
    let mut chunk = vec![0u8; 1 << 20];
    let mut input = InBuffer::around(body);
    loop {
        let mut output = OutBuffer::around(&mut chunk[..]);
        let left = d.decompress_stream(&mut output, &mut input).map_err(err)?;
        let written = output.pos();
        out.extend_from_slice(&chunk[..written]);
        if left == 0 && input.pos() == body.len() {
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyframe(tick: u64, fill: u8) -> Keyframe {
        // Bytes that don't compress on their own (a real heap's don't much).
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        let mut heap: Vec<u8> = (0..3_000_000)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect();
        heap[1000] = fill;
        Keyframe { tick, base: Some((1, 2, 3)), heap, userdata: vec![fill; 10], state: [("rng".to_string(), vec![fill, 1, 2])].into() }
    }

    #[test]
    fn diffs_chain_back_to_a_full_keyframe() {
        let dir = std::env::temp_dir().join(format!("kf-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(&dir, 300);
        let mut previous: Option<(Entry, Vec<u8>)> = None;
        let mut last_full = None;
        for (i, tick) in [99u64, 199, 299, 399, 499].into_iter().enumerate() {
            let k = keyframe(tick, i as u8);
            let (raw, entry) = store.write(&k, previous.as_ref().map(|(e, r)| (e, r.as_slice())), last_full).unwrap();
            if entry.is_full() {
                last_full = Some(tick);
            }
            previous = Some((entry, raw));
        }
        let entries = store.entries();
        assert_eq!(entries.iter().filter(|e| e.is_full()).map(|e| e.tick).collect::<Vec<_>>(), vec![99, 399], "a full one every 300 ticks");
        for (i, e) in entries.iter().enumerate() {
            assert_eq!(store.read(*e).unwrap(), keyframe(e.tick, i as u8), "keyframe {}", e.tick);
        }
        let diff = std::fs::metadata(dir.join(entries[1].file_name())).unwrap().len();
        let full = std::fs::metadata(dir.join(entries[0].file_name())).unwrap().len();
        assert!(diff * 10 < full, "a diff ({diff} B) is far smaller than a full keyframe ({full} B)");
        assert_eq!(store.latest_at_or_before(350), Some(entries[2]));
        std::fs::remove_file(dir.join(entries[0].file_name())).unwrap();
        assert_eq!(store.latest_at_or_before(350), None, "its chain lost its full keyframe");
        assert_eq!(store.latest_at_or_before(450), Some(entries[3]));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
