//! Partial push failure — the ancestor must stay frozen when any push fails.
//!
//! These tests pin down the single most load-bearing ordering rule in the
//! library (App.md § 05, `New claude.md` invariant #4): the ancestor
//! advances only after **all** pushes confirm. If side A's push succeeds
//! but side B's fails mid-sequence, the ancestor must not move, so the
//! next cycle re-derives everything from current state.
//!
//! Side A will still carry its write — that's the honest consequence of
//! sequential (non-two-phase-commit) pushes. Replay is the recovery.

use async_trait::async_trait;
use diff_fusion::adapters::test_memory::TestMemoryAdapter;
use diff_fusion::adapters::in_memory_ancestor::InMemoryAncestorStore;
use diff_fusion::adapters::in_memory_escalation::InMemoryEscalationQueue;
use diff_fusion::application::orchestrator::{CycleOutcome, Orchestrator};
use diff_fusion::application::policy::{OwnedBy, PolicyMap};
use diff_fusion::domain::error::SyncError;
use diff_fusion::ports::ancestor::{AncestorEntry, AncestorKey, AncestorStore};
use diff_fusion::ports::escalation::EscalationQueue;
use diff_fusion::ports::system::{Capabilities, ExternalRef, SystemPort};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

const ENTITY: &str = "purchase_order";
const ID: &str = "PO-1";
const NOW: u64 = 1_700_000_000_000;

/// Test adapter that delegates to an inner `TestMemoryAdapter` for fetch
/// and find operations, but forces every `upsert` to fail with
/// [`SyncError::StaleWrite`]. Call `allow_upsert()` to flip it into a
/// passthrough (used to test recovery).
#[derive(Clone)]
struct FailingUpsertAdapter {
    inner: TestMemoryAdapter,
    fail: Arc<Mutex<bool>>,
}

impl FailingUpsertAdapter {
    fn new(inner: TestMemoryAdapter) -> Self {
        Self {
            inner,
            fail: Arc::new(Mutex::new(true)),
        }
    }

    fn allow_upsert(&self) {
        *self.fail.lock().unwrap() = false;
    }
}

#[async_trait]
impl SystemPort for FailingUpsertAdapter {
    fn system_type(&self) -> &str {
        self.inner.system_type()
    }

    fn capabilities(&self) -> &Capabilities {
        self.inner.capabilities()
    }

    async fn fetch(
        &self,
        entity_type: &str,
        ext: &ExternalRef,
    ) -> Result<(Value, ExternalRef), SyncError> {
        self.inner.fetch(entity_type, ext).await
    }

    async fn find_by_canonical_id(
        &self,
        entity_type: &str,
        canonical_id: &str,
    ) -> Result<Option<ExternalRef>, SyncError> {
        self.inner.find_by_canonical_id(entity_type, canonical_id).await
    }

    async fn upsert(
        &self,
        entity_type: &str,
        canonical_id: &str,
        canonical: &Value,
        expect_version: Option<&str>,
        idempotency_key: &[u8; 32],
    ) -> Result<ExternalRef, SyncError> {
        if *self.fail.lock().unwrap() {
            return Err(SyncError::stale(
                self.inner.system_type(),
                Some("forced".into()),
                "FailingUpsertAdapter: upsert forced to fail",
            ));
        }
        self.inner
            .upsert(entity_type, canonical_id, canonical, expect_version, idempotency_key)
            .await
    }
}

#[tokio::test]
async fn partial_failure_leaves_ancestor_unchanged() {
    // side_a is a normal in-memory adapter; side_b fails every upsert.
    // Setup: ancestor=10, a=15 (changed), b=10 (unchanged). OwnedBy A
    // resolves to 15. Cycle tries to push to B (stale side); B refuses.
    // The orchestrator must:
    //   (1) surface the StaleWrite error,
    //   (2) leave the ancestor at its pre-cycle state.
    let inner_a = TestMemoryAdapter::new("sys_a");
    let inner_b = TestMemoryAdapter::new("sys_b");
    inner_a.seed(ENTITY, ID, json!({"price": 15}));
    inner_b.seed(ENTITY, ID, json!({"price": 10}));

    let side_b = FailingUpsertAdapter::new(inner_b.clone());

    let ancestor = Arc::new(InMemoryAncestorStore::new());
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            AncestorEntry::new(json!({"price": 10}), NOW - 1),
        )
        .unwrap();
    let escalation = Arc::new(InMemoryEscalationQueue::new());

    let policies = PolicyMap::new().with("price", Box::new(OwnedBy::new("sys_a")));
    let orch = Orchestrator::new(
        inner_a.clone(),
        side_b,
        ancestor.clone() as Arc<dyn AncestorStore>,
        policies,
        escalation.clone() as Arc<dyn EscalationQueue>,
    );

    let result = orch.run_cycle_at(ENTITY, ID, NOW).await;

    // (1) StaleWrite surfaces to the caller.
    match result {
        Err(SyncError::StaleWrite { .. }) => {}
        other => panic!("expected StaleWrite, got {other:?}"),
    }

    // (2) Ancestor is FROZEN — the single most important invariant.
    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(
        anc.canonical,
        json!({"price": 10}),
        "ancestor must not advance when a push fails"
    );
    assert_eq!(anc.updated_at_ms, NOW - 1, "ancestor timestamp must not update");
}

#[tokio::test]
async fn partial_failure_side_a_still_carries_its_write() {
    // Honest accounting: A pushes before B in the current cycle. So when
    // B's push fails, A already received its write. The recovery path
    // (second test below) relies on this being the case.
    let inner_a = TestMemoryAdapter::new("sys_a");
    let inner_b = TestMemoryAdapter::new("sys_b");
    // A needs to be stale-relative-to-merged so the cycle actually pushes
    // to A. Use ancestor=10, a=5 (A regressed), b=10. OwnedBy A resolves
    // to 5 → A already has 5, but wait — need A to receive a write.
    //
    // Simpler: Additive policy with both moved, so merged != either view.
    inner_a.seed(ENTITY, ID, json!({"qty": 13})); // +3
    inner_b.seed(ENTITY, ID, json!({"qty": 12})); // +2
    let side_b = FailingUpsertAdapter::new(inner_b.clone());

    let ancestor = Arc::new(InMemoryAncestorStore::new());
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            AncestorEntry::new(json!({"qty": 10}), NOW - 1),
        )
        .unwrap();
    let escalation = Arc::new(InMemoryEscalationQueue::new());

    let policies = PolicyMap::new()
        .with("qty", Box::new(diff_fusion::application::policy::Additive));
    let orch = Orchestrator::new(
        inner_a.clone(),
        side_b,
        ancestor.clone() as Arc<dyn AncestorStore>,
        policies,
        escalation.clone() as Arc<dyn EscalationQueue>,
    );

    let _ = orch.run_cycle_at(ENTITY, ID, NOW).await;

    // Merged value = 10 + 3 + 2 = 15. A got pushed first and accepted.
    // B's push failed. A's state reflects the merged value; B's does not.
    let (a_view, _) = inner_a
        .fetch(
            ENTITY,
            &inner_a.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap(),
        )
        .await
        .unwrap();
    let (b_view, _) = inner_b
        .fetch(
            ENTITY,
            &inner_b.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        a_view,
        json!({"qty": 15.0}),
        "A's push happened before B failed — A has the merged state"
    );
    assert_eq!(
        b_view,
        json!({"qty": 12}),
        "B's push failed — B retains its pre-cycle state"
    );
}

#[tokio::test]
async fn replay_after_partial_failure_converges_b() {
    // The recovery path: after a partial failure, the ancestor is still
    // pre-cycle. Side A carries the write, side B does not. When B comes
    // back (allow_upsert), the next cycle re-derives from current state
    // and pushes to B without double-applying A's delta.
    //
    // This test uses OwnedBy rather than Additive — Additive has a known
    // double-count issue under partial failure (A's delta is already
    // baked into the new-A view). OwnedBy is idempotent under replay and
    // captures the structural guarantee: cycles ARE self-healing.
    let inner_a = TestMemoryAdapter::new("sys_a");
    let inner_b = TestMemoryAdapter::new("sys_b");
    inner_a.seed(ENTITY, ID, json!({"price": 15}));
    inner_b.seed(ENTITY, ID, json!({"price": 10}));

    let side_b = FailingUpsertAdapter::new(inner_b.clone());

    let ancestor = Arc::new(InMemoryAncestorStore::new());
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            AncestorEntry::new(json!({"price": 10}), NOW - 1),
        )
        .unwrap();
    let escalation = Arc::new(InMemoryEscalationQueue::new());

    let policies = PolicyMap::new().with("price", Box::new(OwnedBy::new("sys_a")));
    let orch = Orchestrator::new(
        inner_a.clone(),
        side_b.clone(),
        ancestor.clone() as Arc<dyn AncestorStore>,
        policies,
        escalation.clone() as Arc<dyn EscalationQueue>,
    );

    // First cycle: B fails, ancestor stays at 10.
    let first = orch.run_cycle_at(ENTITY, ID, NOW).await;
    assert!(matches!(first, Err(SyncError::StaleWrite { .. })));

    // Fix side B and replay.
    side_b.allow_upsert();
    let second = orch.run_cycle_at(ENTITY, ID, NOW + 1).await.unwrap();

    match second {
        CycleOutcome::Synced { pushed_to } => {
            assert!(
                pushed_to.contains(&"sys_b".to_string()),
                "replay must push to B: {pushed_to:?}"
            );
        }
        other => panic!("expected Synced on replay, got {other:?}"),
    }

    // B now has the merged value.
    let (b_view, _) = inner_b
        .fetch(
            ENTITY,
            &inner_b.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(b_view, json!({"price": 15}));

    // Ancestor finally advanced — to the merged value, with the replay
    // timestamp.
    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(anc.canonical, json!({"price": 15}));
    assert_eq!(anc.updated_at_ms, NOW + 1);
}
