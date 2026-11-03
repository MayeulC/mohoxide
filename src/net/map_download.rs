//! Maps a joining player doesn't have, fetched from FAF's map vault. The
//! FAF client downloads the host's map before the game starts; without it,
//! the lobby (`ui/lobby.lua`) holds the host's message naming a missing map,
//! [`start`]s its download, and passes the message on once it is mounted, so
//! FAF's lobby finds the map as if it had always been there.
//!
//! The vault's maps are at `content.faforever.com/maps/<folder>.zip` (no
//! login, unlike FAF's API), where `<folder>` is the map's folder name,
//! `name.vNNNN`, which is also the folder in its scenario path. The zip is
//! unpacked into the vault's `maps` folder (where FAF's init file mounts maps
//! from next time) and mounted at `/maps/<folder>` in the running VFS.
//!
//! Nothing here reaches the sim: a downloaded map is read like any other.

use std::collections::BTreeMap;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use crate::engine::vfs::{MountPoint, Vfs, normalise};

/// Where FAF's vault serves maps.
pub const CONTENT_URL: &str = "https://content.faforever.com/maps/";

/// Where maps are downloaded from: [`CONTENT_URL`], or `RTS_MAP_VAULT_URL`
/// (a mirror, or a local server in tests).
fn vault_url() -> String {
    std::env::var("RTS_MAP_VAULT_URL").ok().filter(|u| !u.is_empty()).map_or_else(|| CONTENT_URL.to_owned(), |u| format!("{}/", u.trim_end_matches('/')))
}

/// The largest download accepted (the vault's biggest maps are ~300 MB).
const MAX_DOWNLOAD: u64 = 1 << 30;
/// The most a map may unpack to.
const MAX_UNPACKED: u64 = 4 << 30;

/// How a map's download is going.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Downloading { received: u64, total: Option<u64> },
    Done,
    Failed(String),
}

/// The vault folder maps are downloaded into (`<vault>/maps`).
static MAPS_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Every download this run, by vault folder; a failed one isn't retried.
static DOWNLOADS: Mutex<BTreeMap<String, Status>> = Mutex::new(BTreeMap::new());

/// Sets the folder downloaded maps go in: the vault's `maps` folder.
pub fn set_maps_dir(dir: PathBuf) {
    *MAPS_DIR.lock().unwrap() = Some(dir);
}

/// The vault folder (`name.vNNNN`, lowercase) of a map's scenario path
/// (`/maps/<folder>/<name>_scenario.lua`); `None` for a path that isn't a
/// vault map's (the stock maps have no version, so aren't in the vault).
pub fn vault_folder(scenario: &str) -> Option<String> {
    let path = normalise(scenario);
    let folder = path.strip_prefix("/maps/")?.split('/').next()?;
    let (name, version) = folder.rsplit_once(".v")?;
    let valid_name = !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')) && !name.contains("..");
    let valid_version = !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit());
    (valid_name && valid_version).then(|| folder.to_owned())
}

/// Starts downloading the map of `scenario` (a no-op if it already has
/// been, this run) and returns how it is going; `None` when it can't be
/// downloaded (not a vault map, or no vault folder).
pub fn start(scenario: &str, vfs: Arc<RwLock<Vfs>>) -> Option<Status> {
    let folder = vault_folder(scenario)?;
    let maps_dir = MAPS_DIR.lock().unwrap().clone()?;
    let mut downloads = DOWNLOADS.lock().unwrap();
    if let Some(status) = downloads.get(&folder) {
        return Some(status.clone());
    }
    let status = Status::Downloading { received: 0, total: None };
    downloads.insert(folder.clone(), status.clone());
    drop(downloads);
    let thread_folder = folder.clone();
    let spawned = std::thread::Builder::new().name("map-download".into()).spawn(move || {
        let folder = thread_folder;
        let result = download(&folder, &maps_dir).and_then(|dest| {
            vfs.write().unwrap().mount_folder(MountPoint { source: dest, mountpoint: format!("/maps/{folder}") }).map_err(|e| format!("couldn't mount it: {e}"))
        });
        let status = match result {
            Ok(()) => {
                tracing::info!(map = %folder, "map download: mounted");
                Status::Done
            }
            Err(reason) => {
                tracing::warn!(map = %folder, "map download failed: {reason}");
                Status::Failed(reason)
            }
        };
        DOWNLOADS.lock().unwrap().insert(folder, status);
    });
    if let Err(e) = spawned {
        let status = Status::Failed(format!("couldn't start the download: {e}"));
        DOWNLOADS.lock().unwrap().insert(folder, status.clone());
        return Some(status);
    }
    Some(status)
}

/// How the download of `scenario`'s map is going; `None` if it hasn't started.
pub fn status(scenario: &str) -> Option<Status> {
    DOWNLOADS.lock().unwrap().get(&vault_folder(scenario)?).cloned()
}

fn set_progress(folder: &str, received: u64, total: Option<u64>) {
    DOWNLOADS.lock().unwrap().insert(folder.to_owned(), Status::Downloading { received, total });
}

/// Downloads and unpacks `folder` into `maps_dir`; returns where it went.
fn download(folder: &str, maps_dir: &Path) -> Result<PathBuf, String> {
    let dest = maps_dir.join(folder);
    // Already on disk (a vault folder the init file didn't mount): mount it.
    if dest.is_dir() {
        return Ok(dest);
    }
    let url = format!("{}{folder}.zip", vault_url());
    tracing::info!(%url, "map download: fetching");
    let bytes = fetch(&url, folder)?;
    unpack(&bytes, maps_dir, folder).map_err(|e| format!("couldn't unpack it into {}: {e}", maps_dir.display()))
}

/// An error with its whole `source()` chain: reqwest's own message alone
/// ("builder error") says nothing.
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut text = e.to_string();
    let mut source = e.source();
    while let Some(cause) = source {
        let cause_text = cause.to_string();
        if !text.contains(&cause_text) {
            text.push_str(": ");
            text.push_str(&cause_text);
        }
        source = cause.source();
    }
    text
}

/// The HTTP client. A failure to build it (reqwest's bare "builder error")
/// is logged with its whole cause chain.
fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| {
            let chain = error_chain(&e);
            tracing::error!(error = %chain, "map download: couldn't build the HTTP client");
            format!("couldn't build the HTTP client: {chain}")
        })
}

fn fetch(url: &str, folder: &str) -> Result<Vec<u8>, String> {
    // reqwest is built (for iroh) without a TLS crypto provider of its own:
    // use ring's, as iroh does.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let client = http_client()?;
        let mut response = client.get(url).send().await.map_err(|e| format!("couldn't reach FAF's map vault: {}", error_chain(&e)))?;
        match response.status() {
            s if s.is_success() => {}
            reqwest::StatusCode::NOT_FOUND => return Err("it isn't in FAF's map vault".to_owned()),
            s => return Err(format!("FAF's map vault answered {s}")),
        }
        let total = response.content_length();
        if total.is_some_and(|t| t > MAX_DOWNLOAD) {
            return Err(format!("it's too big ({} MB)", total.unwrap_or(0) >> 20));
        }
        let mut bytes = Vec::with_capacity(total.unwrap_or(0) as usize);
        while let Some(chunk) = response.chunk().await.map_err(|e| format!("the download broke off: {}", error_chain(&e)))? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() as u64 > MAX_DOWNLOAD {
                return Err("it's too big".to_owned());
            }
            set_progress(folder, bytes.len() as u64, total);
        }
        Ok(bytes)
    })
}

/// Unpacks a vault map's zip (its files in a `<folder>/` folder) into
/// `maps_dir/<folder>`: first into a hidden folder beside it, then renamed,
/// so a broken download never leaves a half map that FAF's init file would
/// mount next time.
pub fn unpack(zip: &[u8], maps_dir: &Path, folder: &str) -> io::Result<PathBuf> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).map_err(io::Error::other)?;
    let partial = maps_dir.join(format!(".{folder}.download"));
    if partial.exists() {
        std::fs::remove_dir_all(&partial)?;
    }
    std::fs::create_dir_all(&partial)?;
    let mut unpacked = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(io::Error::other)?;
        let Some(path) = entry.enclosed_name() else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unsafe path in the zip: {}", entry.name())));
        };
        // The map's own folder in the zip is `dest` itself.
        let mut parts = path.components();
        let inside = match parts.next() {
            Some(first) if first.as_os_str().to_string_lossy().eq_ignore_ascii_case(folder) => parts.as_path().to_path_buf(),
            _ => path.clone(),
        };
        if inside.as_os_str().is_empty() {
            continue;
        }
        let out = partial.join(&inside);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&out)?;
        let copied = io::copy(&mut (&mut entry).take(MAX_UNPACKED - unpacked + 1), &mut file)?;
        unpacked += copied;
        if unpacked > MAX_UNPACKED {
            drop(file);
            let _ = std::fs::remove_dir_all(&partial);
            return Err(io::Error::new(io::ErrorKind::InvalidData, "the map unpacks to more than 4 GB"));
        }
    }
    let dest = maps_dir.join(folder);
    std::fs::rename(&partial, &dest)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[derive(Debug)]
    struct Layer(&'static str, Option<Box<dyn std::error::Error>>);
    impl std::fmt::Display for Layer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }
    impl std::error::Error for Layer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.1.as_deref()
        }
    }

    #[test]
    fn error_chain_names_every_cause() {
        let e = Layer("builder error", Some(Box::new(Layer("middle", Some(Box::new(Layer("root cause", None)))))));
        assert_eq!(error_chain(&e), "builder error: middle: root cause");
        assert_eq!(error_chain(&Layer("alone", None)), "alone");
        // A cause the message already repeats is not said twice.
        let e = Layer("failed: refused", Some(Box::new(Layer("refused", None))));
        assert_eq!(error_chain(&e), "failed: refused");
    }

    #[test]
    fn vault_folder_is_the_versioned_map_folder() {
        assert_eq!(vault_folder("/maps/adaptive_monument_valley.v0005/adaptive_monument_valley_scenario.lua").as_deref(), Some("adaptive_monument_valley.v0005"));
        assert_eq!(vault_folder("/MAPS/Setons_Clutch.V0002/x_scenario.lua").as_deref(), Some("setons_clutch.v0002"));
        // Stock maps have no version: not in the vault.
        assert_eq!(vault_folder("/maps/SCMP_009/SCMP_009_scenario.lua"), None);
        assert_eq!(vault_folder("/maps/../x.v0001/a_scenario.lua"), None);
        assert_eq!(vault_folder("/maps/a b.v0001/a_scenario.lua"), None);
        assert_eq!(vault_folder("/mods/x.v0001/a_scenario.lua"), None);
        assert_eq!(vault_folder("/maps/x.vbeta/a_scenario.lua"), None);
    }

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in files {
            out.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            out.write_all(bytes).unwrap();
        }
        out.finish().unwrap().into_inner()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("map_download_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn unpacks_into_the_map_folder() {
        let dir = temp_dir("unpack");
        let zip = zip_of(&[("Some_Map.v0003/some_map_scenario.lua", b"version = 3"), ("Some_Map.v0003/sub/a.dds", b"dds")]);
        let dest = unpack(&zip, &dir, "some_map.v0003").unwrap();
        assert_eq!(dest, dir.join("some_map.v0003"));
        assert_eq!(std::fs::read(dest.join("some_map_scenario.lua")).unwrap(), b"version = 3");
        assert_eq!(std::fs::read(dest.join("sub/a.dds")).unwrap(), b"dds");
        assert!(!dir.join(".some_map.v0003.download").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn refuses_paths_out_of_the_folder() {
        let dir = temp_dir("unsafe");
        let zip = zip_of(&[("../evil.lua", b"x")]);
        assert!(unpack(&zip, &dir, "m.v0001").is_err());
        assert!(!dir.join("m.v0001").exists());
        assert!(!dir.parent().unwrap().join("evil.lua").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mounted_map_is_found() {
        let dir = temp_dir("mount");
        let zip = zip_of(&[("m.v0001/m_scenario.lua", b"x")]);
        let dest = unpack(&zip, &dir, "m.v0001").unwrap();
        let mut vfs = Vfs::new(Vec::new(), Vec::new()).unwrap();
        assert!(vfs.file_info("/maps/m.v0001/m_scenario.lua").is_none());
        vfs.mount_folder(MountPoint { source: dest, mountpoint: "/maps/m.v0001".into() }).unwrap();
        assert!(vfs.file_info("/maps/m.v0001/m_scenario.lua").is_some());
        assert!(vfs.file_info("/maps/m.v0001").is_some_and(|i| i.is_folder));
        assert_eq!(vfs.find_files("/maps", "*_scenario.lua").unwrap().collect::<Vec<_>>(), ["/maps/m.v0001/m_scenario.lua"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The real vault (network): `cargo test map_download -- --ignored`.
    #[test]
    #[ignore]
    fn downloads_from_the_vault() {
        let bytes = fetch(&format!("{CONTENT_URL}adaptive_monument_valley.v0005.zip"), "adaptive_monument_valley.v0005").unwrap();
        let dir = temp_dir("vault");
        let dest = unpack(&bytes, &dir, "adaptive_monument_valley.v0005").unwrap();
        assert!(dest.join("adaptive_monument_valley_scenario.lua").is_file());
        assert_eq!(fetch(&format!("{CONTENT_URL}no_such_map.v0001.zip"), "no_such_map.v0001"), Err("it isn't in FAF's map vault".to_owned()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
