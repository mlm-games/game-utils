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
use std::time::{SystemTime, UNIX_EPOCH};

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
        Self::opfs().create_dir_all(path)
    }
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        Self::opfs().read(path)
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        Self::opfs().write(path, data)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        Self::opfs().rename(from, to)
    }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        Self::opfs().copy(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        Self::opfs().remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        Self::opfs().remove_dir_all(path)
    }
    fn exists(&self, path: &Path) -> bool {
        Self::opfs().exists(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        Self::opfs().is_dir(path)
    }
    fn is_file(&self, path: &Path) -> bool {
        Self::opfs().is_file(path)
    }
    fn metadata_len(&self, path: &Path) -> Option<u64> {
        Self::opfs().metadata_len(path)
    }
    fn mtime_secs(&self, path: &Path) -> Option<u64> {
        Self::opfs().mtime_secs(path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        Self::opfs().read_dir(path)
    }
    fn sync_file(&self, _path: &Path) {}
    fn sync_dir(&self, _path: &Path) {}
}

#[cfg(target_arch = "wasm32")]
impl FsStorage {
    fn opfs() -> &'static OpfsStorage {
        use std::sync::OnceLock;
        static OPFS: OnceLock<OpfsStorage> = OnceLock::new();
        OPFS.get_or_init(OpfsStorage::new)
    }
}

/// On wasm, `opfs::sync::Fs` is origin-private `localStorage` sync, on native it is `std::fs`.
#[cfg(target_arch = "wasm32")]
pub use opfs::sync::Fs as OpfsStorage;

#[cfg(target_arch = "wasm32")]
impl Storage for OpfsStorage {
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        OpfsStorage::create_dir_all(self, path)
    }
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        OpfsStorage::read(self, path)
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        OpfsStorage::write(self, path, data)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        OpfsStorage::rename(self, from, to)
    }
    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        OpfsStorage::copy(self, from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        OpfsStorage::remove_file(self, path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        OpfsStorage::remove_dir_all(self, path)
    }
    fn exists(&self, path: &Path) -> bool {
        OpfsStorage::exists(self, path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        OpfsStorage::is_dir(self, path)
    }
    fn is_file(&self, path: &Path) -> bool {
        OpfsStorage::is_file(self, path)
    }
    fn metadata_len(&self, path: &Path) -> Option<u64> {
        OpfsStorage::metadata_len(self, path)
    }
    fn mtime_secs(&self, path: &Path) -> Option<u64> {
        OpfsStorage::mtime_secs(self, path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        OpfsStorage::read_dir(self, path)
    }
    fn sync_file(&self, path: &Path) {
        OpfsStorage::sync_file(self, path)
    }
    fn sync_dir(&self, path: &Path) {
        OpfsStorage::sync_dir(self, path)
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
            g.dirs.insert(to_n.clone());
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
}
