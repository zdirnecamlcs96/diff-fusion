//! In-memory reference adapter.
//!
//! Used by the shared contract test suite and any integration test that
//! needs a [`SystemPort`] without hitting a network. Two instances with
//! different `system_type` labels simulate a two-system sync end to end.
//!
//! Behavior:
//! - Storage is a `Mutex<HashMap>` keyed by `(entity_type, external_id)`.
//! - Reverse lookup uses a secondary `(entity_type, canonical_id)` index.
//! - Version strings are monotonic decimal counters (`"1"`, `"2"`, ...).
//! - OCC is enforced: an upsert with a stale `expect_version` returns
//!   [`SyncError::StaleWrite`].
//! - Idempotency keys are tracked per entity: a repeat upsert with the same
//!   key is a no-op, returning the existing ref unchanged.
//! - `upsert` shallow-merges the pushed CIF's top-level fields onto any
//!   existing stored record — it never replaces the record wholesale, so
//!   fields the CIF mapping doesn't carry (native-only state) survive a
//!   push. See [`crate::ports::conformance`].
//!
//! The adapter is deliberately small — everything in this file is a
//! reference, not a framework.

use crate::domain::error::SyncError;
use crate::ports::system::{ExternalRef, SystemPort};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
struct StoredEntity {
    external_id: String,
    canonical: Value,
    version: u64,
    last_idempotency_key: Option<[u8; 32]>,
}

#[derive(Debug, Default)]
struct Inner {
    entities: HashMap<(String, String), StoredEntity>,
    canonical_index: HashMap<(String, String), String>,
    next_external_id: u64,
    next_version: u64,
}

/// In-memory adapter. Clone is cheap — the inner store is `Arc<Mutex<_>>`.
#[derive(Debug, Clone)]
pub struct TestMemoryAdapter {
    system_type: String,
    inner: Arc<Mutex<Inner>>,
}

impl TestMemoryAdapter {
    pub fn new(system_type: impl Into<String>) -> Self {
        Self {
            system_type: system_type.into(),
            inner: Arc::new(Mutex::new(Inner::default())),
        }
    }

    /// Seed an entity directly — bypasses the normal upsert path, useful
    /// for tests that want to start the adapter in a specific state.
    pub fn seed(&self, entity_type: &str, canonical_id: &str, canonical: Value) -> ExternalRef {
        let mut inner = self.inner.lock().expect("poisoned");
        inner.next_external_id += 1;
        inner.next_version += 1;
        let external_id = format!("EXT-{}", inner.next_external_id);
        let version = inner.next_version;
        let stored = StoredEntity {
            external_id: external_id.clone(),
            canonical,
            version,
            last_idempotency_key: None,
        };
        inner
            .entities
            .insert((entity_type.into(), external_id.clone()), stored);
        inner
            .canonical_index
            .insert((entity_type.into(), canonical_id.into()), external_id.clone());
        ExternalRef::new(&self.system_type, external_id, Some(version.to_string()))
    }

    fn make_ref(&self, external_id: &str, version: u64) -> ExternalRef {
        ExternalRef::new(&self.system_type, external_id, Some(version.to_string()))
    }
}

#[async_trait]
impl SystemPort for TestMemoryAdapter {
    fn system_type(&self) -> &str {
        &self.system_type
    }

    async fn fetch(
        &self,
        entity_type: &str,
        ext: &ExternalRef,
    ) -> Result<(Value, ExternalRef), SyncError> {
        let inner = self.inner.lock().expect("poisoned");
        let key = (entity_type.to_string(), ext.external_id.clone());
        match inner.entities.get(&key) {
            Some(e) => Ok((e.canonical.clone(), self.make_ref(&e.external_id, e.version))),
            None => Err(SyncError::transient(format!(
                "entity not found: {}/{}",
                entity_type, ext.external_id
            ))),
        }
    }

    async fn find_by_canonical_id(
        &self,
        entity_type: &str,
        canonical_id: &str,
    ) -> Result<Option<ExternalRef>, SyncError> {
        let inner = self.inner.lock().expect("poisoned");
        let idx_key = (entity_type.to_string(), canonical_id.to_string());
        let Some(external_id) = inner.canonical_index.get(&idx_key) else {
            return Ok(None);
        };
        let ent_key = (entity_type.to_string(), external_id.clone());
        let Some(e) = inner.entities.get(&ent_key) else {
            return Ok(None);
        };
        Ok(Some(self.make_ref(&e.external_id, e.version)))
    }

    async fn upsert(
        &self,
        entity_type: &str,
        canonical_id: &str,
        canonical: &Value,
        expect_version: Option<&str>,
        idempotency_key: &[u8; 32],
    ) -> Result<ExternalRef, SyncError> {
        let mut inner = self.inner.lock().expect("poisoned");

        let idx_key = (entity_type.to_string(), canonical_id.to_string());
        let existing_ext_id = inner.canonical_index.get(&idx_key).cloned();
        let mut existing_canonical: Option<Value> = None;

        if let Some(ext_id) = existing_ext_id.clone() {
            let ent_key = (entity_type.to_string(), ext_id.clone());
            let existing = inner
                .entities
                .get(&ent_key)
                .expect("index points to live entity");

            // Idempotency: repeat upsert with the same key is a no-op.
            if existing.last_idempotency_key.as_ref() == Some(idempotency_key) {
                return Ok(self.make_ref(&existing.external_id, existing.version));
            }

            // OCC: if caller supplied an expected version, it must match.
            if let Some(expect) = expect_version
                && expect != existing.version.to_string()
            {
                return Err(SyncError::stale(
                    &self.system_type,
                    Some(existing.version.to_string()),
                    format!(
                        "version mismatch: caller expected {expect}, current {}",
                        existing.version
                    ),
                ));
            }

            existing_canonical = Some(existing.canonical.clone());
        } else if expect_version.is_some() {
            // Caller expected an existing record but none exists.
            return Err(SyncError::stale(
                &self.system_type,
                None,
                "caller supplied expect_version but no existing record",
            ));
        }

        inner.next_version += 1;
        let version = inner.next_version;

        let external_id = if let Some(ext_id) = existing_ext_id {
            ext_id
        } else {
            inner.next_external_id += 1;
            let new_id = format!("EXT-{}", inner.next_external_id);
            inner
                .canonical_index
                .insert(idx_key, new_id.clone());
            new_id
        };

        // Read-modify-write: shallow-merge the pushed CIF onto whatever's
        // already stored, so fields outside the CIF mapping (native-only
        // local state) survive the push instead of being wiped by replace.
        // ponytail: top-level shallow merge; deep path-merge if nested local fields ever needed
        let merged_canonical = match &existing_canonical {
            Some(base) => shallow_merge(base, canonical),
            None => canonical.clone(),
        };

        let stored = StoredEntity {
            external_id: external_id.clone(),
            canonical: merged_canonical,
            version,
            last_idempotency_key: Some(*idempotency_key),
        };
        inner
            .entities
            .insert((entity_type.to_string(), external_id.clone()), stored);

        Ok(self.make_ref(&external_id, version))
    }
}

/// Shallow-merge `patch`'s top-level keys onto `base`. Non-object values
/// fall back to a straight replace (there's no sensible field to merge).
fn shallow_merge(base: &Value, patch: &Value) -> Value {
    let (Some(base_obj), Some(patch_obj)) = (base.as_object(), patch.as_object()) else {
        return patch.clone();
    };
    let mut out = base_obj.clone();
    for (k, v) in patch_obj {
        out.insert(k.clone(), v.clone());
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::idempotency::idempotency_key;
    use serde_json::json;

    fn key(payload: &Value) -> [u8; 32] {
        idempotency_key("PO-1", "upsert", payload)
    }

    #[tokio::test]
    async fn find_by_canonical_id_returns_none_when_missing() {
        let a = TestMemoryAdapter::new("sys_a");
        let r = a.find_by_canonical_id("purchase_order", "PO-1").await.unwrap();
        assert!(r.is_none());
    }

    #[tokio::test]
    async fn upsert_creates_and_find_returns_ref() {
        let a = TestMemoryAdapter::new("sys_a");
        let payload = json!({"total": 100});
        let r = a
            .upsert("purchase_order", "PO-1", &payload, None, &key(&payload))
            .await
            .unwrap();
        assert_eq!(r.system, "sys_a");
        assert!(r.version.is_some());

        let found = a.find_by_canonical_id("purchase_order", "PO-1").await.unwrap();
        assert_eq!(found, Some(r.clone()));
    }

    #[tokio::test]
    async fn repeat_upsert_with_same_idempotency_key_is_noop() {
        let a = TestMemoryAdapter::new("sys_a");
        let payload = json!({"total": 100});
        let k = key(&payload);
        let r1 = a
            .upsert("purchase_order", "PO-1", &payload, None, &k)
            .await
            .unwrap();
        let r2 = a
            .upsert("purchase_order", "PO-1", &payload, None, &k)
            .await
            .unwrap();
        assert_eq!(r1, r2, "second upsert should return the same ref");
    }

    #[tokio::test]
    async fn new_idempotency_key_advances_version() {
        let a = TestMemoryAdapter::new("sys_a");
        let k1 = idempotency_key("PO-1", "upsert", &json!({"total": 100}));
        let k2 = idempotency_key("PO-1", "upsert", &json!({"total": 200}));
        let r1 = a
            .upsert("purchase_order", "PO-1", &json!({"total": 100}), None, &k1)
            .await
            .unwrap();
        let r2 = a
            .upsert(
                "purchase_order",
                "PO-1",
                &json!({"total": 200}),
                r1.version.as_deref(),
                &k2,
            )
            .await
            .unwrap();
        assert_ne!(r1.version, r2.version);
    }

    #[tokio::test]
    async fn stale_version_produces_stale_write_error() {
        let a = TestMemoryAdapter::new("sys_a");
        let k1 = idempotency_key("PO-1", "upsert", &json!({"total": 100}));
        let k2 = idempotency_key("PO-1", "upsert", &json!({"total": 200}));
        let _ = a
            .upsert("purchase_order", "PO-1", &json!({"total": 100}), None, &k1)
            .await
            .unwrap();
        // Caller still thinks version is something that moved on.
        let err = a
            .upsert(
                "purchase_order",
                "PO-1",
                &json!({"total": 200}),
                Some("999"),
                &k2,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, SyncError::StaleWrite { .. }));
    }

    #[tokio::test]
    async fn expect_version_on_nonexistent_record_is_stale() {
        let a = TestMemoryAdapter::new("sys_a");
        let k = idempotency_key("PO-1", "upsert", &json!({}));
        let err = a
            .upsert("purchase_order", "PO-1", &json!({}), Some("1"), &k)
            .await
            .unwrap_err();
        assert!(matches!(err, SyncError::StaleWrite { .. }));
    }

    #[tokio::test]
    async fn fetch_roundtrips() {
        let a = TestMemoryAdapter::new("sys_a");
        let k = idempotency_key("PO-1", "upsert", &json!({"total": 100}));
        let r = a
            .upsert("purchase_order", "PO-1", &json!({"total": 100}), None, &k)
            .await
            .unwrap();
        let (val, fetched_ref) = a.fetch("purchase_order", &r).await.unwrap();
        assert_eq!(val, json!({"total": 100}));
        assert_eq!(fetched_ref.external_id, r.external_id);
    }

    #[tokio::test]
    async fn seeded_entities_are_findable() {
        let a = TestMemoryAdapter::new("sys_a");
        let r = a.seed("purchase_order", "PO-7", json!({"total": 77}));
        let found = a.find_by_canonical_id("purchase_order", "PO-7").await.unwrap();
        assert_eq!(found, Some(r));
    }

    // -- SystemPort conformance harness ---------------------------------
    // `seed()` already bypasses upsert; `fetch()` + `find_by_canonical_id()`
    // already return the raw stored value verbatim (this adapter doesn't
    // do CIF↔native translation), so both sides of `RawAccess` fall
    // straight through to existing SystemPort methods.

    use crate::ports::conformance::{RawAccess, assert_system_port_contract};

    struct RawView<'a>(&'a TestMemoryAdapter);

    #[async_trait::async_trait]
    impl<'a> RawAccess for RawView<'a> {
        async fn seed_raw(&self, entity_type: &str, canonical_id: &str, native: Value) -> ExternalRef {
            self.0.seed(entity_type, canonical_id, native)
        }

        async fn read_raw(&self, entity_type: &str, canonical_id: &str) -> Value {
            let r = self
                .0
                .find_by_canonical_id(entity_type, canonical_id)
                .await
                .unwrap()
                .expect("seed_raw must have created a findable record");
            let (val, _) = self.0.fetch(entity_type, &r).await.unwrap();
            val
        }
    }

    #[tokio::test]
    async fn passes_the_system_port_conformance_harness() {
        let a = TestMemoryAdapter::new("sys_a");
        assert_system_port_contract(&a, &RawView(&a)).await;
    }
}
