//! Integration tests for `PolicyStore` — the host-backed alternative to a
//! hand-coded `PolicyMap`, wired through `Orchestrator::with_policy_store`.
//!
//! Mirrors `sync_cycle_tests.rs`'s structure and helpers (`build_orchestrator`,
//! `TestMemoryAdapter`, `InMemoryAncestorStore`, `InMemoryEscalationQueue`).

use diff_fusion::adapters::in_memory_ancestor::InMemoryAncestorStore;
use diff_fusion::adapters::in_memory_escalation::InMemoryEscalationQueue;
use diff_fusion::adapters::in_memory_policy_store::InMemoryPolicyStore;
use diff_fusion::adapters::test_memory::TestMemoryAdapter;
use diff_fusion::application::orchestrator::{CycleOutcome, Orchestrator};
use diff_fusion::application::policy::{ConflictClass, OwnedBy, PolicyMap};
use diff_fusion::domain::error::SyncError;
use diff_fusion::ports::ancestor::{AncestorEntry, AncestorKey, AncestorStore};
use diff_fusion::ports::escalation::EscalationQueue;
use diff_fusion::ports::policy_store::{PolicyStore, PolicyStoreError};
use serde_json::json;
use std::sync::Arc;

const ENTITY: &str = "purchase_order";
const ID: &str = "PO-1";
const NOW: u64 = 1_700_000_000_000;

fn build_orchestrator(
    policies: PolicyMap,
) -> (
    Orchestrator<TestMemoryAdapter, TestMemoryAdapter>,
    Arc<InMemoryAncestorStore>,
    Arc<InMemoryEscalationQueue>,
    TestMemoryAdapter,
    TestMemoryAdapter,
) {
    let side_a = TestMemoryAdapter::new("sys_a");
    let side_b = TestMemoryAdapter::new("sys_b");
    let ancestor = Arc::new(InMemoryAncestorStore::new());
    let escalation = Arc::new(InMemoryEscalationQueue::new());

    let orch = Orchestrator::new(
        side_a.clone(),
        side_b.clone(),
        ancestor.clone() as Arc<dyn AncestorStore>,
        policies,
        escalation.clone() as Arc<dyn EscalationQueue>,
    );
    (orch, ancestor, escalation, side_a, side_b)
}

/// A store whose `load` always fails, standing in for a malformed document
/// a real adapter would surface as `Backend` (e.g. corrupt JSON in a
/// `jsonb` column). `InMemoryPolicyStore::set_json` validates at write
/// time, so it can never hold a malformed document — this stub is what
/// exercises `run_cycle_at`'s error path instead.
struct FailingPolicyStore;
impl PolicyStore for FailingPolicyStore {
    fn load(&self, _entity_type: &str) -> Result<PolicyMap, PolicyStoreError> {
        Err(PolicyStoreError::Backend(
            "malformed json: unexpected end of input".into(),
        ))
    }
}

#[tokio::test]
async fn store_loaded_map_resolves_a_cycle_cleanly() {
    let store = InMemoryPolicyStore::new();
    store
        .set_json(
            ENTITY,
            &json!({
                "fields": {"price": {"kind": "owned_by", "system": "sys_a"}}
            }),
        )
        .unwrap();

    let (orch, ancestor, escalation, a, b) = build_orchestrator(PolicyMap::new());
    let orch = orch.with_policy_store(Arc::new(store));

    a.seed(ENTITY, ID, json!({"price": 15}));
    b.seed(ENTITY, ID, json!({"price": 10}));
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            AncestorEntry::new(json!({"price": 10}), NOW - 1),
        )
        .unwrap();

    let outcome = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap();
    match outcome {
        CycleOutcome::Synced { pushed_to } => assert_eq!(pushed_to, vec!["sys_b".to_string()]),
        other => panic!("expected Synced, got {other:?}"),
    }

    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(anc.canonical, json!({"price": 15}));
    assert_eq!(escalation.len(), 0);
}

#[tokio::test]
async fn changed_field_with_no_matching_policy_escalates_and_leaves_ancestor_unchanged() {
    // The store's document covers "qty" but not "price" — a changed
    // "price" has no policy to resolve it under.
    let store = InMemoryPolicyStore::new();
    store
        .set_json(ENTITY, &json!({"fields": {"qty": {"kind": "additive"}}}))
        .unwrap();

    let (orch, ancestor, escalation, a, b) = build_orchestrator(PolicyMap::new());
    let orch = orch.with_policy_store(Arc::new(store));

    a.seed(ENTITY, ID, json!({"price": 15}));
    b.seed(ENTITY, ID, json!({"price": 20}));
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            AncestorEntry::new(json!({"price": 10}), NOW - 1),
        )
        .unwrap();

    let outcome = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap();
    match outcome {
        CycleOutcome::Escalated { conflicts } => {
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].path, "price");
            assert_eq!(conflicts[0].class, ConflictClass::NoPolicy);
        }
        other => panic!("expected Escalated, got {other:?}"),
    }

    // Ancestor did NOT advance.
    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(anc.canonical, json!({"price": 10}));
    assert_eq!(anc.updated_at_ms, NOW - 1);
    assert_eq!(escalation.len(), 1);
}

#[tokio::test]
async fn malformed_store_document_returns_transient_error_and_touches_nothing() {
    let (orch, ancestor, escalation, a, b) = build_orchestrator(PolicyMap::new());
    let orch = orch.with_policy_store(Arc::new(FailingPolicyStore));

    a.seed(ENTITY, ID, json!({"price": 15}));
    b.seed(ENTITY, ID, json!({"price": 20}));
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            AncestorEntry::new(json!({"price": 10}), NOW - 1),
        )
        .unwrap();

    let err = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap_err();
    assert!(matches!(err, SyncError::Transient(_)));

    // Neither the ancestor nor the escalation queue was touched.
    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(anc.canonical, json!({"price": 10}));
    assert_eq!(anc.updated_at_ms, NOW - 1);
    assert_eq!(escalation.len(), 0);
}

#[tokio::test]
async fn policy_swapped_between_cycles_takes_effect_on_the_next_cycle_only() {
    let store = Arc::new(InMemoryPolicyStore::new());
    store
        .set_json(ENTITY, &json!({"default": {"kind": "owned_by", "system": "sys_a"}}))
        .unwrap();

    let (orch, ancestor, _escalation, a, b) = build_orchestrator(PolicyMap::new());
    let orch = orch.with_policy_store(store.clone());

    // Cycle 1: sys_a owns by policy — sys_a's value wins.
    a.seed(ENTITY, ID, json!({"price": 15}));
    b.seed(ENTITY, ID, json!({"price": 10}));
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            AncestorEntry::new(json!({"price": 10}), NOW - 1),
        )
        .unwrap();

    let first = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap();
    assert!(matches!(first, CycleOutcome::Synced { .. }));
    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(anc.canonical, json!({"price": 15}));

    // Swap the policy row: ownership now belongs to sys_b.
    store
        .set_json(ENTITY, &json!({"default": {"kind": "owned_by", "system": "sys_b"}}))
        .unwrap();

    // Cycle 2: both sides diverge from the new ancestor (15).
    a.seed(ENTITY, ID, json!({"price": 99}));
    b.seed(ENTITY, ID, json!({"price": 50}));

    let second = orch.run_cycle_at(ENTITY, ID, NOW + 1).await.unwrap();
    assert!(matches!(second, CycleOutcome::Synced { .. }));

    // sys_b's value won this time — proves the reload picked up the swap.
    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(anc.canonical, json!({"price": 50}));
}

#[tokio::test]
async fn store_replaces_static_policies_entirely_rather_than_merging() {
    // Static policies would resolve "price" cleanly via OwnedBy(sys_a).
    // The configured store has no document for this entity type at all
    // (Ok(PolicyMap::default())) — if the two were merged, the static
    // OwnedBy would still apply and this would resolve cleanly. It must
    // not: the store's empty map wins outright.
    let static_policies = PolicyMap::new().with("price", Box::new(OwnedBy::new("sys_a")));
    let (orch, ancestor, escalation, a, b) = build_orchestrator(static_policies);
    let orch = orch.with_policy_store(Arc::new(InMemoryPolicyStore::new()));

    a.seed(ENTITY, ID, json!({"price": 15}));
    b.seed(ENTITY, ID, json!({"price": 20}));
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            AncestorEntry::new(json!({"price": 10}), NOW - 1),
        )
        .unwrap();

    let outcome = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap();
    match outcome {
        CycleOutcome::Escalated { conflicts } => {
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].path, "price");
            assert_eq!(conflicts[0].class, ConflictClass::NoPolicy);
        }
        other => panic!("expected Escalated (store replaces, not merges), got {other:?}"),
    }

    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(anc.canonical, json!({"price": 10}));
    assert_eq!(escalation.len(), 1);
}
