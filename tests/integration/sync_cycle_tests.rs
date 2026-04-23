//! End-to-end integration tests for the full sync cycle.
//!
//! These run two in-memory adapters against one orchestrator and verify the
//! non-negotiable behaviors from `App.md` § 05 and `new/CLAUDE.md`:
//!
//! - Ancestor advances only after both pushes confirm.
//! - An idempotent re-push does not create a duplicate or bump the version.
//! - Genuine conflicts land in the escalation queue and do not silently
//!   resolve.
//! - Empty changelogs do nothing.

use diff_fusion::adapters::test_memory::TestMemoryAdapter;
use diff_fusion::adapters::in_memory_ancestor::InMemoryAncestorStore;
use diff_fusion::adapters::in_memory_escalation::InMemoryEscalationQueue;
use diff_fusion::ports::ancestor::{AncestorKey, AncestorStore};
use diff_fusion::ports::escalation::EscalationQueue;
use diff_fusion::application::orchestrator::{CycleOutcome, Orchestrator};
use diff_fusion::application::policy::{
    Additive, Invariant, InvariantOutcome, InvariantSet, OwnedBy, PolicyMap,
};
use diff_fusion::ports::system::SystemPort;
use serde_json::{Value, json};
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

#[tokio::test]
async fn identical_views_are_noop() {
    let policies = PolicyMap::new().with("qty", Box::new(Additive));
    let (orch, ancestor, escalation, a, b) = build_orchestrator(policies);

    a.seed(ENTITY, ID, json!({"qty": 5}));
    b.seed(ENTITY, ID, json!({"qty": 5}));
    // Pre-seed ancestor so three-way diff sees no change.
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            diff_fusion::ports::ancestor::AncestorEntry::new(json!({"qty": 5}), NOW - 1),
        )
        .unwrap();

    let outcome = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap();
    assert_eq!(outcome, CycleOutcome::NoOp);
    assert_eq!(escalation.len(), 0);
}

#[tokio::test]
async fn owned_field_propagates_one_way() {
    // sys_a owns price. Only sys_a moved — push to sys_b, ancestor advances.
    let policies = PolicyMap::new().with("price", Box::new(OwnedBy::new("sys_a")));
    let (orch, ancestor, escalation, a, b) = build_orchestrator(policies);

    a.seed(ENTITY, ID, json!({"price": 15}));
    b.seed(ENTITY, ID, json!({"price": 10}));
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            diff_fusion::ports::ancestor::AncestorEntry::new(json!({"price": 10}), NOW - 1),
        )
        .unwrap();

    let outcome = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap();
    match outcome {
        CycleOutcome::Synced { pushed_to } => {
            assert_eq!(pushed_to, vec!["sys_b".to_string()]);
        }
        other => panic!("expected Synced, got {other:?}"),
    }

    // Ancestor advanced to the new value.
    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(anc.canonical, json!({"price": 15}));
    assert_eq!(anc.updated_at_ms, NOW);

    // sys_b now reflects sys_a's value.
    let (b_view, _) = b
        .fetch(
            ENTITY,
            &b.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(b_view, json!({"price": 15}));

    assert_eq!(escalation.len(), 0);
}

#[tokio::test]
async fn additive_counters_accumulate_and_push_both_sides() {
    // Both sides bumped qty — additive merges both deltas.
    let policies = PolicyMap::new().with("qty", Box::new(Additive));
    let (orch, ancestor, escalation, a, b) = build_orchestrator(policies);

    a.seed(ENTITY, ID, json!({"qty": 13})); // +3 on A
    b.seed(ENTITY, ID, json!({"qty": 12})); // +2 on B
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            diff_fusion::ports::ancestor::AncestorEntry::new(json!({"qty": 10}), NOW - 1),
        )
        .unwrap();

    let outcome = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap();
    match outcome {
        CycleOutcome::Synced { pushed_to } => {
            // Both sides were stale relative to the merged 15; both pushed.
            assert_eq!(pushed_to.len(), 2);
            assert!(pushed_to.contains(&"sys_a".to_string()));
            assert!(pushed_to.contains(&"sys_b".to_string()));
        }
        other => panic!("expected Synced, got {other:?}"),
    }

    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(anc.canonical, json!({"qty": 15.0}));

    assert_eq!(escalation.len(), 0);
}

#[tokio::test]
async fn unresolvable_conflict_routes_to_escalation_and_blocks_writes() {
    // No policy for 'price' — both sides moved it → conflict → escalate.
    // The ancestor must NOT advance, and neither side should be pushed.
    let policies = PolicyMap::new();
    let (orch, ancestor, escalation, a, b) = build_orchestrator(policies);

    a.seed(ENTITY, ID, json!({"price": 15}));
    b.seed(ENTITY, ID, json!({"price": 20}));
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            diff_fusion::ports::ancestor::AncestorEntry::new(json!({"price": 10}), NOW - 1),
        )
        .unwrap();

    let outcome = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap();
    match outcome {
        CycleOutcome::Escalated { conflicts } => {
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].path, "price");
        }
        other => panic!("expected Escalated, got {other:?}"),
    }

    // Ancestor unchanged — did NOT advance.
    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(anc.canonical, json!({"price": 10}));
    assert_eq!(anc.updated_at_ms, NOW - 1);

    // Escalation received the item.
    assert_eq!(escalation.len(), 1);
    let snap = escalation.snapshot();
    assert_eq!(snap[0].entity_type, ENTITY);
    assert_eq!(snap[0].canonical_id, ID);

    // Neither side's current state changed.
    let (a_view, _) = a
        .fetch(ENTITY, &a.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap())
        .await
        .unwrap();
    let (b_view, _) = b
        .fetch(ENTITY, &b.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap())
        .await
        .unwrap();
    assert_eq!(a_view, json!({"price": 15}));
    assert_eq!(b_view, json!({"price": 20}));
}

#[tokio::test]
async fn replayed_cycle_is_idempotent() {
    // Running the same cycle twice in a row must not cause duplicate writes
    // or double-applied additive merges. Second run sees ancestor already
    // caught up and returns NoOp.
    let policies = PolicyMap::new().with("qty", Box::new(Additive));
    let (orch, _, escalation, a, b) = build_orchestrator(policies);

    a.seed(ENTITY, ID, json!({"qty": 13}));
    b.seed(ENTITY, ID, json!({"qty": 12}));

    let first = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap();
    assert!(matches!(first, CycleOutcome::Synced { .. }));

    let second = orch.run_cycle_at(ENTITY, ID, NOW + 1).await.unwrap();
    assert_eq!(second, CycleOutcome::NoOp);
    assert_eq!(escalation.len(), 0);
}

#[tokio::test]
async fn one_way_mode_propagates_source_and_reverts_target_drift() {
    // Orchestrator::one_way installs OwnedBy(source) as the default policy.
    // Source (sys_a) changes propagate to target (sys_b). Target-side drift
    // on the same field reverts to the ancestor value — matching Synology
    // Drive's "download only" semantics.
    let side_a = TestMemoryAdapter::new("sys_a");
    let side_b = TestMemoryAdapter::new("sys_b");
    let ancestor = Arc::new(InMemoryAncestorStore::new());
    let escalation = Arc::new(InMemoryEscalationQueue::new());

    side_a.seed(ENTITY, ID, json!({"price": 20}));    // source moved
    side_b.seed(ENTITY, ID, json!({"price": 99}));    // target drifted
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            diff_fusion::ports::ancestor::AncestorEntry::new(json!({"price": 10}), NOW - 1),
        )
        .unwrap();

    let orch = Orchestrator::one_way(
        side_a.clone(),
        side_b.clone(),
        ancestor.clone() as Arc<dyn AncestorStore>,
        escalation.clone() as Arc<dyn EscalationQueue>,
    );

    let outcome = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap();
    match outcome {
        CycleOutcome::Synced { pushed_to } => {
            // Both sides differ from source's new state → both receive a push.
            assert!(pushed_to.contains(&"sys_b".to_string()));
        }
        other => panic!("expected Synced, got {other:?}"),
    }

    // Both sides now reflect source's value.
    let (a_view, _) = side_a
        .fetch(
            ENTITY,
            &side_a.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap(),
        )
        .await
        .unwrap();
    let (b_view, _) = side_b
        .fetch(
            ENTITY,
            &side_b.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(a_view, json!({"price": 20}));
    assert_eq!(b_view, json!({"price": 20}));

    // Nothing escalated — target drift is expected in one-way mode, not a
    // conflict.
    assert_eq!(escalation.len(), 0);
}

// ---------------------------------------------------------------------
// Invariants — Tier 2 of the policy stack.
//
// Invariants run AFTER Tier 1 resolution produces a candidate merged
// value. They are predicates about valid entity state (e.g. "closed POs
// cannot accept more receipts"). A Reject means the Tier 1 result is
// structurally invalid — it must not be pushed and must surface to a
// human.
// ---------------------------------------------------------------------

/// Test invariant: rejects any merged canonical whose `must_fail` field
/// is truthy. Used to drive the orchestrator's Invariant Reject path.
struct RejectIfMarked;
impl Invariant for RejectIfMarked {
    fn name(&self) -> &'static str {
        "reject_if_marked"
    }
    fn check(&self, _previous: &Value, candidate: &Value) -> InvariantOutcome {
        if candidate.get("must_fail").and_then(|v| v.as_bool()) == Some(true) {
            InvariantOutcome::Reject {
                reason: "candidate is marked must_fail".into(),
            }
        } else {
            InvariantOutcome::Pass
        }
    }
}

#[tokio::test]
async fn invariant_reject_blocks_pushes_and_escalates() {
    // Tier 1 resolution succeeds (OwnedBy picks A's value), but the
    // merged candidate violates a Tier 2 invariant. The orchestrator
    // MUST NOT push to either side and MUST NOT advance the ancestor.
    let side_a = TestMemoryAdapter::new("sys_a");
    let side_b = TestMemoryAdapter::new("sys_b");
    let ancestor = Arc::new(InMemoryAncestorStore::new());
    let escalation = Arc::new(InMemoryEscalationQueue::new());

    side_a.seed(ENTITY, ID, json!({"must_fail": true, "other": "a-val"}));
    side_b.seed(ENTITY, ID, json!({"must_fail": false, "other": "b-val"}));
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            diff_fusion::ports::ancestor::AncestorEntry::new(
                json!({"must_fail": false, "other": "anc-val"}),
                NOW - 1,
            ),
        )
        .unwrap();

    // sys_a owns both fields, so Tier 1 resolves cleanly to A's values.
    let policies = PolicyMap::new().with_default(Box::new(OwnedBy::new("sys_a")));
    let invariants = InvariantSet::new().with(Box::new(RejectIfMarked));

    let orch = Orchestrator::new(
        side_a.clone(),
        side_b.clone(),
        ancestor.clone() as Arc<dyn AncestorStore>,
        policies,
        escalation.clone() as Arc<dyn EscalationQueue>,
    )
    .with_invariants(invariants);

    let outcome = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap();

    // (1) Outcome is Escalated.
    match &outcome {
        CycleOutcome::Escalated { conflicts } => {
            assert_eq!(conflicts.len(), 1, "one invariant rejection → one conflict");
            assert!(
                conflicts[0].reason.contains("reject_if_marked"),
                "escalation reason must name the invariant, got: {}",
                conflicts[0].reason
            );
        }
        other => panic!("expected Escalated, got {other:?}"),
    }

    // (2) Escalation queue got the item.
    assert_eq!(escalation.len(), 1);

    // (3) Ancestor did NOT advance.
    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(
        anc.canonical,
        json!({"must_fail": false, "other": "anc-val"}),
        "ancestor must stay frozen when an invariant rejects"
    );
    assert_eq!(anc.updated_at_ms, NOW - 1);

    // (4) Side B did NOT receive the poisoned value.
    let (b_view, _) = side_b
        .fetch(
            ENTITY,
            &side_b.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        b_view,
        json!({"must_fail": false, "other": "b-val"}),
        "side B must not have been pushed the rejected candidate"
    );
}

/// Test invariant: if `qty` > 100, clamp it to 100 via Transform.
/// Used to drive the Transform path: the pushed canonical must reflect
/// the transformed value, not the raw resolution.
struct CapQtyAt100;
impl Invariant for CapQtyAt100 {
    fn name(&self) -> &'static str {
        "cap_qty_at_100"
    }
    fn check(&self, _previous: &Value, candidate: &Value) -> InvariantOutcome {
        let Some(q) = candidate.get("qty").and_then(|v| v.as_f64()) else {
            return InvariantOutcome::Pass;
        };
        if q > 100.0 {
            let mut fixed = candidate.clone();
            fixed["qty"] = json!(100.0);
            InvariantOutcome::Transform(fixed)
        } else {
            InvariantOutcome::Pass
        }
    }
}

#[tokio::test]
async fn invariant_transform_rewrites_pushed_value() {
    // Additive policy adds deltas and produces qty=150 (a=120, b=130, anc=100 → 150).
    // The invariant clamps to 100. What gets pushed must be the transformed value.
    let side_a = TestMemoryAdapter::new("sys_a");
    let side_b = TestMemoryAdapter::new("sys_b");
    let ancestor = Arc::new(InMemoryAncestorStore::new());
    let escalation = Arc::new(InMemoryEscalationQueue::new());

    side_a.seed(ENTITY, ID, json!({"qty": 120}));
    side_b.seed(ENTITY, ID, json!({"qty": 130}));
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            diff_fusion::ports::ancestor::AncestorEntry::new(json!({"qty": 100}), NOW - 1),
        )
        .unwrap();

    let policies = PolicyMap::new().with("qty", Box::new(Additive));
    let invariants = InvariantSet::new().with(Box::new(CapQtyAt100));

    let orch = Orchestrator::new(
        side_a.clone(),
        side_b.clone(),
        ancestor.clone() as Arc<dyn AncestorStore>,
        policies,
        escalation.clone() as Arc<dyn EscalationQueue>,
    )
    .with_invariants(invariants);

    let outcome = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap();
    assert!(matches!(outcome, CycleOutcome::Synced { .. }));

    // The pushed value on both sides must be 100, not 150.
    let (a_view, _) = side_a
        .fetch(ENTITY, &side_a.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap())
        .await
        .unwrap();
    let (b_view, _) = side_b
        .fetch(ENTITY, &side_b.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap())
        .await
        .unwrap();
    assert_eq!(a_view, json!({"qty": 100.0}));
    assert_eq!(b_view, json!({"qty": 100.0}));

    // Ancestor stored the transformed value — the next cycle must see
    // the clamped qty as the new baseline, not the pre-transform 150.
    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(anc.canonical, json!({"qty": 100.0}));

    assert_eq!(escalation.len(), 0);
}

/// Test invariant: always Pass. Any cycle whose other behavior changes
/// when this is installed reveals a regression in the Pass path.
struct AlwaysPass;
impl Invariant for AlwaysPass {
    fn name(&self) -> &'static str {
        "always_pass"
    }
    fn check(&self, _previous: &Value, _candidate: &Value) -> InvariantOutcome {
        InvariantOutcome::Pass
    }
}

#[tokio::test]
async fn invariant_pass_is_a_noop() {
    // Identical setup to owned_field_propagates_one_way, but with an
    // always-Pass invariant installed. The outcome must be byte-identical
    // to the baseline: A's value wins, B gets pushed, ancestor advances,
    // no escalation. The Pass path is literally "do nothing new."
    let side_a = TestMemoryAdapter::new("sys_a");
    let side_b = TestMemoryAdapter::new("sys_b");
    let ancestor = Arc::new(InMemoryAncestorStore::new());
    let escalation = Arc::new(InMemoryEscalationQueue::new());

    side_a.seed(ENTITY, ID, json!({"price": 15}));
    side_b.seed(ENTITY, ID, json!({"price": 10}));
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            diff_fusion::ports::ancestor::AncestorEntry::new(json!({"price": 10}), NOW - 1),
        )
        .unwrap();

    let policies = PolicyMap::new().with("price", Box::new(OwnedBy::new("sys_a")));
    let invariants = InvariantSet::new().with(Box::new(AlwaysPass));

    let orch = Orchestrator::new(
        side_a.clone(),
        side_b.clone(),
        ancestor.clone() as Arc<dyn AncestorStore>,
        policies,
        escalation.clone() as Arc<dyn EscalationQueue>,
    )
    .with_invariants(invariants);

    let outcome = orch.run_cycle_at(ENTITY, ID, NOW).await.unwrap();
    match outcome {
        CycleOutcome::Synced { pushed_to } => assert_eq!(pushed_to, vec!["sys_b".to_string()]),
        other => panic!("expected Synced, got {other:?}"),
    }

    // Ancestor advanced to A's value.
    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(anc.canonical, json!({"price": 15}));
    assert_eq!(anc.updated_at_ms, NOW);

    // B got the push.
    let (b_view, _) = side_b
        .fetch(ENTITY, &side_b.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap())
        .await
        .unwrap();
    assert_eq!(b_view, json!({"price": 15}));

    // Nothing escalated.
    assert_eq!(escalation.len(), 0);
}

#[tokio::test]
async fn shadow_mode_reports_without_writing() {
    // Shadow mode must produce a report but never touch the adapters or
    // the ancestor store.
    let policies = PolicyMap::new().with("price", Box::new(OwnedBy::new("sys_a")));
    let (orch, ancestor, escalation, a, b) = build_orchestrator(policies);

    a.seed(ENTITY, ID, json!({"price": 15}));
    b.seed(ENTITY, ID, json!({"price": 10}));
    ancestor
        .put(
            AncestorKey::new(ENTITY, ID),
            diff_fusion::ports::ancestor::AncestorEntry::new(json!({"price": 10}), NOW - 1),
        )
        .unwrap();

    let report = orch.run_shadow(ENTITY, ID).await.unwrap();
    assert!(!report.changelog.is_empty());
    assert!(report.resolution.is_clean());
    assert_eq!(report.would_write, Some(json!({"price": 15})));

    // Nothing actually changed.
    let anc = ancestor.get(&AncestorKey::new(ENTITY, ID)).unwrap().unwrap();
    assert_eq!(anc.canonical, json!({"price": 10}));
    assert_eq!(anc.updated_at_ms, NOW - 1);
    assert_eq!(escalation.len(), 0);

    // Neither side was written.
    let (b_view, _) = b
        .fetch(ENTITY, &b.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap())
        .await
        .unwrap();
    assert_eq!(b_view, json!({"price": 10}));
}
