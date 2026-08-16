//! Filesystem-backed [`AncestorStore`] tests.
//!
//! These tests use a real temp directory so reopening the store
//! demonstrates durability. They're the minimum viable coverage for
//! "does this survive a process restart?".

use diff_fusion::adapters::filesystem_ancestor::FilesystemAncestorStore;
use diff_fusion::ports::ancestor::{AncestorEntry, AncestorKey, AncestorStore};
use serde_json::json;
use std::env;

fn fresh_dir(suffix: &str) -> std::path::PathBuf {
    let base = env::temp_dir().join(format!(
        "diff-fusion-fs-ancestor-{}-{}",
        suffix,
        uniq(),
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create temp dir");
    base
}

fn uniq() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

#[test]
fn put_then_get_roundtrips() {
    let dir = fresh_dir("roundtrip");
    let store = FilesystemAncestorStore::open(&dir).unwrap();

    let key = AncestorKey::new("purchase_order", "PO-1");
    let entry = AncestorEntry::new(json!({"total": 100}), 1_700_000_000_000);

    store.put(key.clone(), entry.clone()).unwrap();

    assert_eq!(store.get(&key).unwrap(), Some(entry));
}

#[test]
fn get_returns_none_for_missing() {
    let dir = fresh_dir("missing");
    let store = FilesystemAncestorStore::open(&dir).unwrap();

    let key = AncestorKey::new("invoice", "INV-999");
    assert!(store.get(&key).unwrap().is_none());
}

#[test]
fn state_survives_reopen() {
    let dir = fresh_dir("reopen");

    let key = AncestorKey::new("purchase_order", "PO-42");
    let entry = AncestorEntry::new(json!({"price": 50}), 1_000);

    {
        let store = FilesystemAncestorStore::open(&dir).unwrap();
        store.put(key.clone(), entry.clone()).unwrap();
    }

    // Fresh store instance pointing at the same directory.
    let reopened = FilesystemAncestorStore::open(&dir).unwrap();
    assert_eq!(reopened.get(&key).unwrap(), Some(entry));
}

#[test]
fn different_entity_types_do_not_collide() {
    let dir = fresh_dir("collision");
    let store = FilesystemAncestorStore::open(&dir).unwrap();

    let k1 = AncestorKey::new("purchase_order", "X");
    let k2 = AncestorKey::new("invoice", "X");

    store
        .put(k1.clone(), AncestorEntry::new(json!({"kind": "po"}), 1))
        .unwrap();
    store
        .put(k2.clone(), AncestorEntry::new(json!({"kind": "inv"}), 2))
        .unwrap();

    assert_eq!(
        store.get(&k1).unwrap().unwrap().canonical,
        json!({"kind": "po"})
    );
    assert_eq!(
        store.get(&k2).unwrap().unwrap().canonical,
        json!({"kind": "inv"})
    );
}

#[test]
fn put_overwrites_existing_entry() {
    let dir = fresh_dir("overwrite");
    let store = FilesystemAncestorStore::open(&dir).unwrap();

    let key = AncestorKey::new("item", "SKU-1");
    store
        .put(key.clone(), AncestorEntry::new(json!({"v": 1}), 1))
        .unwrap();
    store
        .put(key.clone(), AncestorEntry::new(json!({"v": 2}), 2))
        .unwrap();

    assert_eq!(
        store.get(&key).unwrap().unwrap().canonical,
        json!({"v": 2})
    );
}

#[test]
fn ids_with_path_unsafe_chars_are_handled() {
    // canonical_id values like "customer/42" or "vendor:abc" must not
    // break the filesystem layout.
    let dir = fresh_dir("unsafe-chars");
    let store = FilesystemAncestorStore::open(&dir).unwrap();

    let key = AncestorKey::new("thing", "customer/42:abc");
    let entry = AncestorEntry::new(json!({"ok": true}), 1);
    store.put(key.clone(), entry.clone()).unwrap();

    assert_eq!(store.get(&key).unwrap(), Some(entry));
}
