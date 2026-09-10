//! Pluggable storage backend for crash-safe persistence.
//!
//! `SaveStore` and friends are parametric over [`Storage`] so the same atomic
//! `temp+rename` + `.bak` + quarantine logic works on native `fs`, in-memory
//! (tests/WASM), or future `EncryptedStorage`/`SteamCloudStorage` without
//! duplicating the crash-safety machinery. Ron stays the default codec.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use web_time::{SystemTime, UNIX_EPOCH};

/// Trait for the underlying file system. Keep it `Clone` so `SaveStore` can stay `Clone`.
pub trait Storage: Clone + Send + Sync + 'static {
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>>;
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn is_file(&self, path: &Path) -> bool;
    fn metadata_len(&self, path: &Path) -> Option<u64>;
    fn mtime_secs(&self, path: &Path) -> Option<u64>;
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn sync_file(&self, path: &Path);
    fn sync_dir(&self, path: &Path);
}

#[derive(Debug, Clone, Default)]
pub struct FsStorage;

#[cfg(not(target_arch = "wasm32"))]
impl Storage for FsStorage {
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        match std::fs::read(path) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, data)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        std::fs::copy(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir_all(path)
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
    fn metadata_len(&self, path: &Path) -> Option<u64> {
        std::fs::metadata(path).ok().map(|m| m.len())
    }
    fn mtime_secs(&self, path: &Path) -> Option<u64> {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(path)? {
            out.push(e?.path());
        }
        Ok(out)
    }
    fn sync_file(&self, path: &Path) {
        if let Ok(f) = std::fs::File::open(path) {
            let _ = f.sync_all();
        }
    }
    fn sync_dir(&self, path: &Path) {
        if let Ok(f) = std::fs::File::open(path) {
            let _ = f.sync_all();
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Storage for FsStorage {
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        Self::ropfs().create_dir_all(path)
    }
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        Self::ropfs().read(path)
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        Self::ropfs().write(path, data)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        Self::ropfs().rename(from, to)
    }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        Self::ropfs().copy(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        Self::ropfs().remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        Self::ropfs().remove_dir_all(path)
    }
    fn exists(&self, path: &Path) -> bool {
        Self::ropfs().exists(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        Self::ropfs().is_dir(path)
    }
    fn is_file(&self, path: &Path) -> bool {
        Self::ropfs().is_file(path)
    }
    fn metadata_len(&self, path: &Path) -> Option<u64> {
        Self::ropfs().metadata_len(path)
    }
    fn mtime_secs(&self, path: &Path) -> Option<u64> {
        Self::ropfs().mtime_secs(path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        Self::ropfs().read_dir(path)
    }
    fn sync_file(&self, _path: &Path) {}
    fn sync_dir(&self, _path: &Path) {}
}

#[cfg(target_arch = "wasm32")]
impl FsStorage {
    fn ropfs() -> &'static RopfsStorage {
        use std::sync::OnceLock;
        static ROPFS: OnceLock<RopfsStorage> = OnceLock::new();
        ROPFS.get_or_init(|| {
            migrate_opfs_to_ropfs();
            RopfsStorage::new()
        })
    }
}

/// One-time, non-destructive migration of wasm `localStorage` keys written by
/// the old `opfs` crate (`opfs:` prefix, raw UTF-8 values) to the `ropfs`
/// crate layout (`ropfs:` prefix, base64 values).
///
/// Copies each `opfs:<rest>` entry to `ropfs:<rest>` only when the destination
/// does not already exist, so it never overwrites newer data. Old values are
/// copied verbatim: `ropfs` base64-decodes with a raw-bytes fallback, so stale
/// raw entries stay readable until their next write re-encodes them. The old
/// keys are left behind; safe to run repeatedly.
///
/// Runs automatically on first [`FsStorage`] use on wasm. Call it explicitly
/// if you construct [`RopfsStorage`] directly (bypassing `FsStorage`), before
/// the first read.
///
/// To delete after a few minor bumps
#[cfg(target_arch = "wasm32")]
pub fn migrate_opfs_to_ropfs() {
    const OLD_PREFIX: &str = "opfs:";
    const NEW_PREFIX: &str = "ropfs:";

    let Some(ls) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    else {
        return;
    };
    let len = ls.length().ok().unwrap_or(0);
    // Snapshot keys first: `set_item` during iteration would shift indices.
    let mut old_keys = Vec::new();
    for i in 0..len {
        if let Ok(Some(k)) = ls.key(i) {
            if k.starts_with(OLD_PREFIX) {
                old_keys.push(k);
            }
        }
    }
    for old_key in old_keys {
        let rest = &old_key[OLD_PREFIX.len()..];
        let new_key = format!("{NEW_PREFIX}{rest}");
        // Never overwrite newer `ropfs:` data.
        if ls.get_item(&new_key).ok().flatten().is_some() {
            continue;
        }
        if let Ok(Some(v)) = ls.get_item(&old_key) {
            let _ = ls.set_item(&new_key, &v);
        }
    }
}

/// On wasm, `ropfs::sync::Fs` is a `localStorage`-backed shim (not OPFS:
/// ~5 MB quota, string-only storage with base64-encoded values); on native
/// it delegates to `std::fs`. Prefer the async `ropfs` OPFS backend on the
/// web for large or strictly durable data.
#[cfg(target_arch = "wasm32")]
pub use ropfs::sync::Fs as RopfsStorage;

/// Kept for compatibility after the `opfs` crate was renamed to `ropfs`.
/// New code should use [`RopfsStorage`].
#[cfg(target_arch = "wasm32")]
#[deprecated(
    since = "0.2.0",
    note = "The `opfs` crate was renamed to `ropfs`; use `RopfsStorage` instead."
)]
pub use ropfs::sync::Fs as OpfsStorage;

#[cfg(target_arch = "wasm32")]
impl Storage for RopfsStorage {
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        RopfsStorage::create_dir_all(self, path)
    }
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        RopfsStorage::read(self, path)
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        RopfsStorage::write(self, path, data)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        RopfsStorage::rename(self, from, to)
    }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        RopfsStorage::copy(self, from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        RopfsStorage::remove_file(self, path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        RopfsStorage::remove_dir_all(self, path)
    }
    fn exists(&self, path: &Path) -> bool {
        RopfsStorage::exists(self, path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        RopfsStorage::is_dir(self, path)
    }
    fn is_file(&self, path: &Path) -> bool {
        RopfsStorage::is_file(self, path)
    }
    fn metadata_len(&self, path: &Path) -> Option<u64> {
        RopfsStorage::metadata_len(self, path)
    }
    fn mtime_secs(&self, path: &Path) -> Option<u64> {
        RopfsStorage::mtime_secs(self, path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        RopfsStorage::read_dir(self, path)
    }
    fn sync_file(&self, path: &Path) {
        RopfsStorage::sync_file(self, path)
    }
    fn sync_dir(&self, path: &Path) {
        RopfsStorage::sync_dir(self, path)
    }
}
/// In-memory `Storage` for hermetic tests. Shares an `Arc<RwLock<HashMap>>` so
/// clones see the same files (mirrors `FsStorage` sharing the real FS).
#[derive(Debug, Clone, Default)]
pub struct MemoryStorage {
    inner: Arc<RwLock<MemoryInner>>,
}

#[derive(Debug, Default)]
struct MemoryInner {
    files: HashMap<PathBuf, Vec<u8>>,
    dirs: std::collections::HashSet<PathBuf>,
    mtimes: HashMap<PathBuf, u64>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }
    fn normalize(p: &Path) -> PathBuf {
        p.to_path_buf()
    }
}

impl Storage for MemoryStorage {
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        let mut g = self.inner.write().unwrap();
        let mut cur = PathBuf::new();
        for comp in path.components() {
            cur.push(comp);
            g.dirs.insert(cur.clone());
        }
        g.dirs.insert(path.to_path_buf());
        Ok(())
    }
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        let g = self.inner.read().unwrap();
        Ok(g.files.get(&Self::normalize(path)).cloned())
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        let mut g = self.inner.write().unwrap();
        if let Some(parent) = path.parent() {
            drop(g);
            self.create_dir_all(parent)?;
            g = self.inner.write().unwrap();
        }
        g.files.insert(Self::normalize(path), data.to_vec());
        g.mtimes.insert(Self::normalize(path), now_secs());
        if let Some(parent) = path.parent() {
            g.dirs.insert(parent.to_path_buf());
        }
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut g = self.inner.write().unwrap();
        let from_n = Self::normalize(from);
        let to_n = Self::normalize(to);
        if let Some(data) = g.files.remove(&from_n) {
            let mtime = g.mtimes.remove(&from_n).unwrap_or(now_secs());
            let mut to_move = Vec::new();
            for k in g.files.keys().cloned().collect::<Vec<_>>() {
                if k.starts_with(&from_n) && k != from_n {
                    to_move.push(k);
                }
            }
            for k in to_move {
                if let Some(v) = g.files.remove(&k) {
                    let rel = k.strip_prefix(&from_n).unwrap();
                    let new_k = to_n.join(rel);
                    g.files.insert(new_k.clone(), v);
                    if let Some(mt) = g.mtimes.remove(&k) {
                        g.mtimes.insert(new_k, mt);
                    }
                }
            }
            let mut dirs_to_move = Vec::new();
            for d in g.dirs.iter().cloned().collect::<Vec<_>>() {
                if d.starts_with(&from_n) && d != from_n {
                    dirs_to_move.push(d);
                }
            }
            for d in dirs_to_move {
                g.dirs.remove(&d);
                let rel = d.strip_prefix(&from_n).unwrap();
                g.dirs.insert(to_n.join(rel));
            }
            g.dirs.remove(&from_n);
            g.files.insert(to_n.clone(), data);
            g.mtimes.insert(to_n.clone(), mtime);
            g.dirs.remove(&to_n);
            if let Some(parent) = to_n.parent() {
                g.dirs.insert(parent.to_path_buf());
            }
            return Ok(());
        }
        if g.dirs.contains(&from_n) || g.files.keys().any(|k| k.starts_with(&from_n)) {
            let mut to_move = Vec::new();
            for k in g.files.keys().cloned().collect::<Vec<_>>() {
                if k.starts_with(&from_n) {
                    to_move.push(k);
                }
            }
            for k in to_move {
                if let Some(v) = g.files.remove(&k) {
                    let rel = k.strip_prefix(&from_n).unwrap();
                    let new_k = if rel.as_os_str().is_empty() {
                        to_n.clone()
                    } else {
                        to_n.join(rel)
                    };
                    let mt = g.mtimes.remove(&k).unwrap_or(now_secs());
                    g.files.insert(new_k.clone(), v);
                    g.mtimes.insert(new_k, mt);
                }
            }
            let mut dirs_to_move = Vec::new();
            for d in g.dirs.iter().cloned().collect::<Vec<_>>() {
                if d.starts_with(&from_n) {
                    dirs_to_move.push(d);
                }
            }
            for d in dirs_to_move {
                let rel = d.strip_prefix(&from_n).unwrap();
                let new_d = if rel.as_os_str().is_empty() {
                    to_n.clone()
                } else {
                    to_n.join(rel)
                };
                let is_moved = g.dirs.remove(&d);
                g.dirs.insert(new_d.clone());
                if let Some(mt) = g.mtimes.remove(&d) {
                    g.mtimes.insert(new_d, mt);
                } else if is_moved {
                    g.mtimes.insert(new_d, now_secs());
                }
            }
            g.dirs.remove(&from_n);
            g.dirs.insert(to_n.clone());
            if let Some(parent) = to_n.parent() {
                g.dirs.insert(parent.to_path_buf());
            }
            return Ok(());
        }
        Err(io::Error::new(io::ErrorKind::NotFound, "not found"))
    }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        let g = self.inner.read().unwrap();
        let data = g
            .files
            .get(&Self::normalize(from))
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "copy source not found"))?;
        drop(g);
        let len = data.len() as u64;
        self.write(to, &data)?;
        Ok(len)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        let mut g = self.inner.write().unwrap();
        if g.files.remove(&Self::normalize(path)).is_none() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "not found"));
        }
        g.mtimes.remove(&Self::normalize(path));
        Ok(())
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        let mut g = self.inner.write().unwrap();
        let p = Self::normalize(path);
        g.files.retain(|k, _| !k.starts_with(&p));
        g.mtimes.retain(|k, _| !k.starts_with(&p));
        g.dirs.retain(|d| !d.starts_with(&p) && d != &p);
        Ok(())
    }
    fn exists(&self, path: &Path) -> bool {
        let g = self.inner.read().unwrap();
        let p = Self::normalize(path);
        g.files.contains_key(&p) || g.dirs.contains(&p)
    }
    fn is_dir(&self, path: &Path) -> bool {
        let g = self.inner.read().unwrap();
        g.dirs.contains(&Self::normalize(path))
    }
    fn is_file(&self, path: &Path) -> bool {
        let g = self.inner.read().unwrap();
        g.files.contains_key(&Self::normalize(path))
    }
    fn metadata_len(&self, path: &Path) -> Option<u64> {
        let g = self.inner.read().unwrap();
        g.files.get(&Self::normalize(path)).map(|v| v.len() as u64)
    }
    fn mtime_secs(&self, path: &Path) -> Option<u64> {
        let g = self.inner.read().unwrap();
        g.mtimes.get(&Self::normalize(path)).copied()
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let g = self.inner.read().unwrap();
        let p = Self::normalize(path);
        if !g.dirs.contains(&p) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "dir not found"));
        }
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for k in g.files.keys().chain(g.dirs.iter()) {
            if let Ok(rel) = k.strip_prefix(&p) {
                if let Some(first) = rel.components().next() {
                    let child = p.join(first);
                    if seen.insert(child.clone()) {
                        out.push(child);
                    }
                }
            }
        }
        Ok(out)
    }
    fn sync_file(&self, _path: &Path) {}
    fn sync_dir(&self, _path: &Path) {}
}

/// Simple XOR decorator over any `Storage`. Keeps Ron codec: `write` encrypts Ron bytes,
/// `read` decrypts.
#[derive(Debug, Clone)]
pub struct EncryptedStorage<S: Storage> {
    inner: S,
    key: Vec<u8>,
}

impl<S: Storage> EncryptedStorage<S> {
    pub fn new(inner: S, key: impl Into<Vec<u8>>) -> Self {
        Self {
            inner,
            key: key.into(),
        }
    }
    pub fn inner(&self) -> &S {
        &self.inner
    }
    fn xor(&self, data: &[u8]) -> Vec<u8> {
        if self.key.is_empty() {
            return data.to_vec();
        }
        data.iter()
            .enumerate()
            .map(|(i, b)| b ^ self.key[i % self.key.len()])
            .collect()
    }
}

impl<S: Storage> Storage for EncryptedStorage<S> {
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        self.inner.create_dir_all(path)
    }
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        match self.inner.read(path)? {
            Some(v) => Ok(Some(self.xor(&v))),
            None => Ok(None),
        }
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        self.inner.write(path, &self.xor(data))
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to)
    }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        self.inner.copy(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_dir_all(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        self.inner.is_dir(path)
    }
    fn is_file(&self, path: &Path) -> bool {
        self.inner.is_file(path)
    }
    fn metadata_len(&self, path: &Path) -> Option<u64> {
        self.inner.metadata_len(path)
    }
    fn mtime_secs(&self, path: &Path) -> Option<u64> {
        self.inner.mtime_secs(path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        self.inner.read_dir(path)
    }
    fn sync_file(&self, path: &Path) {
        self.inner.sync_file(path)
    }
    fn sync_dir(&self, path: &Path) {
        self.inner.sync_dir(path)
    }
}

#[cfg(target_os = "android")]
static ANDROID_DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Record the runtime internal data dir (from
/// `AndroidApp::internal_data_path()`) for [`android_data_dir`]. Call once
/// from `android_main`; later calls are ignored.
#[cfg(target_os = "android")]
pub fn set_android_data_dir(path: PathBuf) {
    let _ = ANDROID_DATA_DIR.set(path);
}

/// The stored runtime dir, if [`set_android_data_dir`] was called.
/// `SaveManager` prefers this on Android without needing the package id.
#[cfg(target_os = "android")]
pub fn android_runtime_dir() -> Option<PathBuf> {
    ANDROID_DATA_DIR.get().cloned()
}

pub fn android_fallback_dir(package: &str) -> PathBuf {
    PathBuf::from(format!("/data/data/{package}/files"))
}

/// App-private data dir on Android: stored runtime path, then
/// [`android_fallback_dir`], then the temp fallback. Never panics.
#[cfg(target_os = "android")]
pub fn android_data_dir(package: &str) -> PathBuf {
    if let Some(dir) = ANDROID_DATA_DIR.get() {
        return dir.clone();
    }
    let dir = android_fallback_dir(package);
    if FsStorage.create_dir_all(&dir).is_ok() {
        return dir;
    }
    let dir = std::env::temp_dir().join(package.replace('.', "-"));
    let _ = FsStorage.create_dir_all(&dir);
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<S: Storage>(s: S) {
        let dir = PathBuf::from("/tmp/memtest");
        s.create_dir_all(&dir).unwrap();
        let p = dir.join("a.ron");
        s.write(&p, b"hello").unwrap();
        assert_eq!(s.read(&p).unwrap(), Some(b"hello".to_vec()));
        assert!(s.exists(&p));
        assert_eq!(s.metadata_len(&p), Some(5));
        s.remove_file(&p).unwrap();
        assert!(!s.exists(&p));
    }

    #[test]
    fn fs_storage_smoke() {
        roundtrip(FsStorage);
    }

    #[test]
    fn memory_storage_smoke() {
        roundtrip(MemoryStorage::new());
    }

    #[test]
    fn memory_rename_dir() {
        let s = MemoryStorage::new();
        let base = PathBuf::from("/tmp/base");
        s.create_dir_all(&base.join("a")).unwrap();
        s.write(&base.join("a/f.txt"), b"x").unwrap();
        s.rename(&base.join("a"), &base.join("b")).unwrap();
        assert!(!s.exists(&base.join("a/f.txt")));
        assert_eq!(s.read(&base.join("b/f.txt")).unwrap(), Some(b"x".to_vec()));
    }

    #[test]
    fn android_fallback_dir_matches_godot_user_dir() {
        assert_eq!(
            android_fallback_dir("org.rozvp.app"),
            PathBuf::from("/data/data/org.rozvp.app/files")
        );
    }
}
