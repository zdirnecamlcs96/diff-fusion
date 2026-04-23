//! In-memory [`AncestorStore`] — reference impl used by tests and the
//! default [`crate::drivers::sync_engine::SyncEngine`] configuration.
//!
//! Not durable. Real deployments implement the trait against a filesystem
//! or database backend.

use crate::ports::ancestor::{AncestorEntry, AncestorKey, AncestorStore, AncestorStoreError};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct InMemoryAncestorStore {
    inner: Mutex<HashMap<AncestorKey, AncestorEntry>>,
}

impl InMemoryAncestorStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().expect("ancestor store poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl AncestorStore for InMemoryAncestorStore {
    fn get(&self, key: &AncestorKey) -> Result<Option<AncestorEntry>, AncestorStoreError> {
        Ok(self
            .inner
            .lock()
            .map_err(|e| AncestorStoreError::Backend(e.to_string()))?
            .get(key)
            .cloned())
    }

    fn put(&self, key: AncestorKey, entry: AncestorEntry) -> Result<(), AncestorStoreError> {
        self.inner
            .lock()
            .map_err(|e| AncestorStoreError::Backend(e.to_string()))?
            .insert(key, entry);
        Ok(())
    }

    fn delete(&self, key: &AncestorKey) -> Result<(), AncestorStoreError> {
        self.inner
            .lock()
            .map_err(|e| AncestorStoreError::Backend(e.to_string()))?
            .remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn put_then_get_roundtrips() {
        let store = InMemoryAncestorStore::new();
        let key = AncestorKey::new("purchase_order", "PO-1");
        let entry = AncestorEntry::new(json!({"total": 100}), 1_700_000_000_000);

        store.put(key.clone(), entry.clone()).unwrap();

        assert_eq!(store.get(&key).unwrap(), Some(entry));
    }

    #[test]
    fn get_returns_none_for_missing() {
        let store = InMemoryAncestorStore::new();
        let key = AncestorKey::new("invoice", "INV-999");

        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn put_overwrites() {
        let store = InMemoryAncestorStore::new();
        let key = AncestorKey::new("item", "SKU-1");

        store
            .put(key.clone(), AncestorEntry::new(json!({"v": 1}), 1))
            .unwrap();
        store
            .put(key.clone(), AncestorEntry::new(json!({"v": 2}), 2))
            .unwrap();

        assert_eq!(store.get(&key).unwrap().unwrap().canonical, json!({"v": 2}));
    }

    #[test]
    fn delete_removes() {
        let store = InMemoryAncestorStore::new();
        let key = AncestorKey::new("item", "SKU-1");

        store
            .put(key.clone(), AncestorEntry::new(json!({}), 1))
            .unwrap();
        store.delete(&key).unwrap();

        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn different_entity_types_do_not_collide() {
        let store = InMemoryAncestorStore::new();
        let k1 = AncestorKey::new("purchase_order", "ID-1");
        let k2 = AncestorKey::new("invoice", "ID-1");

        store
            .put(k1.clone(), AncestorEntry::new(json!({"kind": "po"}), 1))
            .unwrap();
        store
            .put(k2.clone(), AncestorEntry::new(json!({"kind": "inv"}), 2))
            .unwrap();

        assert_eq!(store.get(&k1).unwrap().unwrap().canonical, json!({"kind": "po"}));
        assert_eq!(store.get(&k2).unwrap().unwrap().canonical, json!({"kind": "inv"}));
    }
}
