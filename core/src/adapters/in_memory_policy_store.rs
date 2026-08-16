//! In-memory [`PolicyStore`] — reference impl used by tests.
//!
//! Not durable. Real deployments implement the trait against a
//! relational (`jsonb` column) or document (Mongo subdocument) backend.
//! Stores raw [`PolicyDocument`]s and calls [`PolicyDocument::build`] on
//! each [`PolicyStore::load`] — so tests against it exercise the same
//! deserialize-then-build path a real adapter uses.

use crate::application::policy::{PolicyDocument, PolicyMap};
use crate::ports::policy_store::{PolicyStore, PolicyStoreError};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct InMemoryPolicyStore {
    inner: Mutex<HashMap<String, PolicyDocument>>,
}

impl InMemoryPolicyStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a document for an entity type directly.
    pub fn set(&self, entity_type: impl Into<String>, document: PolicyDocument) {
        self.inner
            .lock()
            .expect("in-memory policy store mutex poisoned")
            .insert(entity_type.into(), document);
    }

    /// Store a document parsed from raw JSON. Surfaces a malformed
    /// document as [`PolicyStoreError::Backend`], matching how a real
    /// adapter would report a corrupt row.
    pub fn set_json(
        &self,
        entity_type: impl Into<String>,
        json: &Value,
    ) -> Result<(), PolicyStoreError> {
        let document: PolicyDocument = serde_json::from_value(json.clone())
            .map_err(|e| PolicyStoreError::Backend(e.to_string()))?;
        self.set(entity_type, document);
        Ok(())
    }
}

impl PolicyStore for InMemoryPolicyStore {
    fn load(&self, entity_type: &str) -> Result<PolicyMap, PolicyStoreError> {
        let inner = self
            .inner
            .lock()
            .map_err(|e| PolicyStoreError::Backend(e.to_string()))?;
        Ok(inner
            .get(entity_type)
            .map(PolicyDocument::build)
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::policy::{MergeContext, MergePolicyRef, resolve};
    use crate::domain::diff::three_way_diff;
    use serde_json::json;

    #[test]
    fn load_missing_entity_type_returns_default_map() {
        let store = InMemoryPolicyStore::new();
        let map = store.load("purchase_order").unwrap();

        let log = three_way_diff(&json!({"x": 1}), &json!({"x": 2}), &json!({"x": 1}));
        let r = resolve(&log, &map, &MergeContext::new("a", "b"));
        assert_eq!(r.resolved.len(), 0);
        assert_eq!(r.conflicts.len(), 1); // no document -> NoPolicy escalation
    }

    #[test]
    fn set_then_load_builds_the_declared_policies() {
        let store = InMemoryPolicyStore::new();
        store
            .set_json(
                "purchase_order",
                &json!({
                    "fields": {"price": {"kind": "owned_by", "system": "netsuite"}},
                    "default": {"kind": "additive"}
                }),
            )
            .unwrap();

        let map = store.load("purchase_order").unwrap();
        let anc = json!({"price": 10, "qty": 5});
        let a = json!({"price": 15, "qty": 6});
        let b = json!({"price": 10, "qty": 7});
        let log = three_way_diff(&anc, &a, &b);
        let r = resolve(&log, &map, &MergeContext::new("netsuite", "erp"));

        assert!(r.conflicts.is_empty());
        let resolved: HashMap<_, _> = r.resolved.into_iter().collect();
        assert_eq!(resolved["price"], json!(15)); // owned_by netsuite
        assert_eq!(resolved["qty"], json!(8.0)); // falls to default additive
    }

    #[test]
    fn set_json_surfaces_malformed_document_as_backend_error() {
        let store = InMemoryPolicyStore::new();
        let err = store
            .set_json(
                "purchase_order",
                &json!({"fields": {"price": {"kind": "not_a_real_kind"}}}),
            )
            .unwrap_err();
        assert!(matches!(err, PolicyStoreError::Backend(_)));
    }

    #[test]
    fn different_entity_types_do_not_collide() {
        let store = InMemoryPolicyStore::new();
        store.set(
            "purchase_order",
            PolicyDocument {
                fields: HashMap::new(),
                default: Some(MergePolicyRef::Additive),
            },
        );

        let log = three_way_diff(&json!({"x": 1}), &json!({"x": 2}), &json!({"x": 1}));
        let ctx = MergeContext::new("a", "b");

        let po_result = resolve(&log, &store.load("purchase_order").unwrap(), &ctx);
        assert!(po_result.conflicts.is_empty()); // has a default policy

        let inv_result = resolve(&log, &store.load("invoice").unwrap(), &ctx);
        assert_eq!(inv_result.conflicts.len(), 1); // no document for "invoice"
    }
}
