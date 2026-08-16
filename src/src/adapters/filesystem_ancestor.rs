//! Filesystem-backed [`AncestorStore`].
//!
//! Stores each ancestor as a single JSON file under a root directory:
//!
//! ```text
//! <root>/
//! ├─ <entity_type>/
//! │  ├─ <hashed_canonical_id>.json
//! │  └─ ...
//! └─ ...
//! ```
//!
//! `canonical_id` is hashed (blake3) to produce the filename so arbitrary
//! user-supplied strings (including slashes, colons, unicode) can't
//! escape the root or collide with filesystem semantics. The file's
//! contents carry the original key, the canonical value, and the
//! updated-at timestamp.
//!
//! Writes go to a temp file and are renamed into place so an
//! interrupted write can't leave a half-written ancestor on disk. This
//! matches the "ancestor is load-bearing" invariant — the store must
//! never observe partial state.

use crate::ports::ancestor::{AncestorEntry, AncestorKey, AncestorStore, AncestorStoreError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct FilesystemAncestorStore {
    root: PathBuf,
}

impl FilesystemAncestorStore {
    /// Open a store rooted at `root`. Creates the directory if missing.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, AncestorStoreError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).map_err(io)?;
        Ok(Self { root })
    }

    fn path_for(&self, key: &AncestorKey) -> PathBuf {
        let entity_dir = self.root.join(sanitize(&key.entity_type));
        let file = format!("{}.json", hash_id(&key.canonical_id));
        entity_dir.join(file)
    }

    fn ensure_parent(path: &Path) -> Result<(), AncestorStoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct OnDisk {
    /// Original composite key — stored alongside the payload so callers
    /// can reconstruct it without parsing the path.
    key: AncestorKey,
    entry: AncestorEntry,
}

impl AncestorStore for FilesystemAncestorStore {
    fn get(&self, key: &AncestorKey) -> Result<Option<AncestorEntry>, AncestorStoreError> {
        let path = self.path_for(key);
        match fs::read(&path) {
            Ok(bytes) => {
                let on_disk: OnDisk = serde_json::from_slice(&bytes).map_err(serde)?;
                Ok(Some(on_disk.entry))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }

    fn put(&self, key: AncestorKey, entry: AncestorEntry) -> Result<(), AncestorStoreError> {
        let path = self.path_for(&key);
        Self::ensure_parent(&path)?;

        let payload = OnDisk { key, entry };
        let bytes = serde_json::to_vec(&payload).map_err(serde)?;

        // Atomic write: write to a temp in the same dir, then rename.
        let tmp = path.with_extension("json.tmp");
        {
            let mut f = fs::File::create(&tmp).map_err(io)?;
            f.write_all(&bytes).map_err(io)?;
            f.sync_all().map_err(io)?;
        }
        fs::rename(&tmp, &path).map_err(io)?;
        Ok(())
    }
}

/// Allow only [A-Za-z0-9_-]; replace anything else with `_`. Entity types
/// are normally short ASCII identifiers, but we don't want to trust the
/// caller to stay that way.
fn sanitize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}

/// Collapse arbitrary canonical_id strings to a stable hex filename.
/// blake3 avoids birthday-paradox collisions for any realistic id count.
fn hash_id(raw: &str) -> String {
    let digest = blake3::hash(raw.as_bytes());
    let bytes = digest.as_bytes();
    let mut out = String::with_capacity(64);
    for &b in bytes.iter().take(16) {
        // 32 hex chars — plenty for stable uniqueness on disk.
        out.push_str(&format!("{:02x}", b));
    }
    out
}

fn io(e: std::io::Error) -> AncestorStoreError {
    AncestorStoreError::Backend(format!("io: {e}"))
}

fn serde(e: serde_json::Error) -> AncestorStoreError {
    AncestorStoreError::Backend(format!("serde: {e}"))
}
