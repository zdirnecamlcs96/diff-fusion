//! Facade-level tests — what the PUBLIC user-facing API looks like.
//!
//! These tests use **only** `SyncEngine` and the minimum types a user
//! must know about (policies, invariants, their own adapter). They do
//! NOT import `Orchestrator`, `InMemoryAncestorStore`, `PolicyMap`,
//! `Arc`, or `dyn AncestorStore` casts. If a test here needs one of
//! those, the facade is leaking implementation detail and needs another
//! method.

use diff_fusion::adapters::test_memory::TestMemoryAdapter;
use diff_fusion::application::policy::{
    Additive, ConflictClass, Invariant, InvariantOutcome, OwnedBy, SetByKey,
};
use diff_fusion::ports::system::SystemPort;
use diff_fusion::drivers::sync_engine::{SyncEngine, SyncOutcome};
use serde_json::{Value, json};

const ENTITY: &str = "purchase_order";
const ID: &str = "PO-42";

fn seed_pair() -> (TestMemoryAdapter, TestMemoryAdapter) {
    let a = TestMemoryAdapter::new("erp");
    let b = TestMemoryAdapter::new("inv");
    a.seed(ENTITY, ID, json!({"price": 20, "qty_recv": 6}));
    b.seed(ENTITY, ID, json!({"price": 10, "qty_recv": 7}));
    (a, b)
}

#[tokio::test]
async fn happy_path_uses_only_facade_and_defaults() {
    // A minimal facade user: build engine from two adapters, declare
    // policies per field, call sync(). No Arc, no dyn, no imports from
    // the internal modules.
    let (erp, inv) = seed_pair();

    let engine = SyncEngine::builder(erp, inv)
        .policy("price", Box::new(OwnedBy::new("erp")))
        .policy("qty_recv", Box::new(Additive))
        .build();

    let outcome = engine.sync(ENTITY, ID).await.unwrap();
    match outcome {
        SyncOutcome::Synced { pushed_to } => {
            assert!(pushed_to.contains(&"erp".to_string()));
            assert!(pushed_to.contains(&"inv".to_string()));
        }
        other => panic!("expected Synced, got {other:?}"),
    }

    // Queue is part of the facade — no need to own the Arc or the type.
    assert_eq!(engine.escalation_depth(), 0);
}

#[tokio::test]
async fn preview_reports_without_writing() {
    let (erp, inv) = seed_pair();

    let engine = SyncEngine::builder(erp, inv)
        .policy("price", Box::new(OwnedBy::new("erp")))
        .policy("qty_recv", Box::new(Additive))
        .build();

    let preview = engine.preview(ENTITY, ID).await.unwrap();
    assert!(preview.would_write.is_some());
    assert!(preview.conflicts.is_empty());

    // Running sync afterward still works — preview didn't advance
    // ancestor or mutate anything.
    let outcome = engine.sync(ENTITY, ID).await.unwrap();
    assert!(matches!(outcome, SyncOutcome::Synced { .. }));
}

struct NeverNegativeQty;
impl Invariant for NeverNegativeQty {
    fn name(&self) -> &'static str {
        "never_negative_qty"
    }
    fn check(&self, _prev: &Value, candidate: &Value) -> InvariantOutcome {
        let q = candidate.get("qty_recv").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if q < 0.0 {
            InvariantOutcome::Reject {
                reason: "qty_recv must not be negative".into(),
            }
        } else {
            InvariantOutcome::Pass
        }
    }
}

#[tokio::test]
async fn invariant_rejection_escalates_through_facade() {
    // Invariants are exposed via the builder; rejections show up via
    // SyncOutcome::Escalated and engine.escalation_depth() without the
    // user touching any inner queue type.
    let erp = TestMemoryAdapter::new("erp");
    let inv = TestMemoryAdapter::new("inv");
    // Seed state that will resolve to a negative qty_recv (additive of
    // negative deltas).
    erp.seed(ENTITY, ID, json!({"qty_recv": 3}));
    inv.seed(ENTITY, ID, json!({"qty_recv": 2}));
    // Without ancestor, the facade bootstraps from side_a; the three-way
    // diff vs. that bootstrap will show Both-changed. Additive math: anc=3,
    // a=3 (no change), b=2 (-1), result = 2. Hmm, not negative — make
    // ancestor large so both deltas are negative.

    let engine = SyncEngine::builder(erp, inv)
        .policy("qty_recv", Box::new(Additive))
        .invariant(Box::new(NeverNegativeQty))
        // Force a stored ancestor so the math is explicit.
        .seed_ancestor(ENTITY, ID, json!({"qty_recv": 10}))
        .build();

    // a delta = 3 - 10 = -7, b delta = 2 - 10 = -8, merged = 10 - 7 - 8 = -5
    let outcome = engine.sync(ENTITY, ID).await.unwrap();
    match outcome {
        SyncOutcome::Escalated { conflicts } => {
            assert_eq!(conflicts.len(), 1);
            assert!(conflicts[0].reason.contains("never_negative_qty"));
        }
        other => panic!("expected Escalated, got {other:?}"),
    }

    assert_eq!(engine.escalation_depth(), 1);
}

#[tokio::test]
async fn one_way_preset_is_a_single_call() {
    // The user shouldn't need to build a PolicyMap or know about
    // OwnedBy to declare one-way sync. The facade exposes presets
    // directly.
    let erp = TestMemoryAdapter::new("erp");
    let inv = TestMemoryAdapter::new("inv");
    erp.seed(ENTITY, ID, json!({"price": 20}));
    inv.seed(ENTITY, ID, json!({"price": 99})); // target drift

    let engine = SyncEngine::builder(erp, inv).one_way().build();

    let outcome = engine.sync(ENTITY, ID).await.unwrap();
    assert!(matches!(outcome, SyncOutcome::Synced { .. }));
    assert_eq!(engine.escalation_depth(), 0);
}

// ---------------------------------------------------------------------
// Tier 3 — SetByKey collection merges through the full cycle.
// ---------------------------------------------------------------------

#[tokio::test]
async fn set_by_key_merges_additions_from_both_sides() {
    // Ancestor has one line item. Each side adds a distinct new item.
    // After sync, both sides must see all three items.
    let erp = TestMemoryAdapter::new("erp");
    let inv = TestMemoryAdapter::new("inv");

    let ancestor_state = json!({"items": [{"sku": "X", "q": 1}]});
    erp.seed(
        ENTITY,
        ID,
        json!({"items": [{"sku": "X", "q": 1}, {"sku": "Y", "q": 2}]}),
    );
    inv.seed(
        ENTITY,
        ID,
        json!({"items": [{"sku": "X", "q": 1}, {"sku": "Z", "q": 3}]}),
    );

    let engine = SyncEngine::builder(erp.clone(), inv.clone())
        .policy("items", Box::new(SetByKey::new("sku")))
        .seed_ancestor(ENTITY, ID, ancestor_state)
        .build();

    let outcome = engine.sync(ENTITY, ID).await.unwrap();
    assert!(matches!(outcome, SyncOutcome::Synced { .. }));

    // Both sides converge to the same array containing all three items.
    for (port, label) in [(&erp, "erp"), (&inv, "inv")] {
        let ext = port.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap();
        let (val, _) = port.fetch(ENTITY, &ext).await.unwrap();
        let items = val["items"]
            .as_array()
            .unwrap_or_else(|| panic!("{label} must have items array"));
        let skus: Vec<&str> = items.iter().map(|v| v["sku"].as_str().unwrap()).collect();
        assert!(skus.contains(&"X"), "{label}: missing X");
        assert!(skus.contains(&"Y"), "{label}: missing Y");
        assert!(skus.contains(&"Z"), "{label}: missing Z");
        assert_eq!(skus.len(), 3, "{label}: unexpected length {skus:?}");
    }

    assert_eq!(engine.escalation_depth(), 0);
}

#[tokio::test]
async fn set_by_key_escalates_when_same_element_diverges() {
    // Both sides modified line 'X' to different values. Cycle must
    // escalate and NOT push or advance ancestor.
    let erp = TestMemoryAdapter::new("erp");
    let inv = TestMemoryAdapter::new("inv");

    let ancestor_state = json!({"items": [{"sku": "X", "q": 1}]});
    erp.seed(ENTITY, ID, json!({"items": [{"sku": "X", "q": 10}]}));
    inv.seed(ENTITY, ID, json!({"items": [{"sku": "X", "q": 20}]}));

    let engine = SyncEngine::builder(erp.clone(), inv.clone())
        .policy("items", Box::new(SetByKey::new("sku")))
        .seed_ancestor(ENTITY, ID, ancestor_state.clone())
        .build();

    let outcome = engine.sync(ENTITY, ID).await.unwrap();
    match outcome {
        SyncOutcome::Escalated { conflicts } => {
            assert_eq!(conflicts.len(), 1);
            assert!(conflicts[0].reason.contains("set_by_key"));
        }
        other => panic!("expected Escalated, got {other:?}"),
    }

    // Neither side was pushed — sides keep their divergent state.
    let (a_view, _) = erp
        .fetch(
            ENTITY,
            &erp.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap(),
        )
        .await
        .unwrap();
    let (b_view, _) = inv
        .fetch(
            ENTITY,
            &inv.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(a_view, json!({"items": [{"sku": "X", "q": 10}]}));
    assert_eq!(b_view, json!({"items": [{"sku": "X", "q": 20}]}));

    assert_eq!(engine.escalation_depth(), 1);
}

#[tokio::test]
async fn set_by_key_honors_unilateral_deletion() {
    // A removed item Y. B did not touch Y. Merge should drop Y from
    // both sides without escalating — the removal was unopposed.
    let erp = TestMemoryAdapter::new("erp");
    let inv = TestMemoryAdapter::new("inv");

    let ancestor_state = json!({"items": [{"sku": "X"}, {"sku": "Y"}]});
    erp.seed(ENTITY, ID, json!({"items": [{"sku": "X"}]}));
    inv.seed(ENTITY, ID, json!({"items": [{"sku": "X"}, {"sku": "Y"}]}));

    let engine = SyncEngine::builder(erp.clone(), inv.clone())
        .policy("items", Box::new(SetByKey::new("sku")))
        .seed_ancestor(ENTITY, ID, ancestor_state)
        .build();

    let outcome = engine.sync(ENTITY, ID).await.unwrap();
    assert!(matches!(outcome, SyncOutcome::Synced { .. }));

    for (port, label) in [(&erp, "erp"), (&inv, "inv")] {
        let ext = port.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap();
        let (val, _) = port.fetch(ENTITY, &ext).await.unwrap();
        let items = val["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "{label}: Y should be gone");
        assert_eq!(items[0]["sku"], json!("X"));
    }

    assert_eq!(engine.escalation_depth(), 0);
}

// ---------------------------------------------------------------------
// ConflictClass taxonomy — tag each FacadeConflict with its cause so
// users can branch disposition per class (Dropbox/Synology-style
// per-class visibility).
// ---------------------------------------------------------------------

struct CapAt100;
impl Invariant for CapAt100 {
    fn name(&self) -> &'static str {
        "cap_at_100"
    }
    fn check(&self, _prev: &Value, cand: &Value) -> InvariantOutcome {
        let Some(q) = cand.get("qty").and_then(|v| v.as_f64()) else {
            return InvariantOutcome::Pass;
        };
        if q > 100.0 {
            let mut fixed = cand.clone();
            fixed["qty"] = json!(100.0);
            InvariantOutcome::Transform(fixed)
        } else {
            InvariantOutcome::Pass
        }
    }
}

#[tokio::test]
async fn invariant_transform_propagates_through_facade_to_adapter_state() {
    // Additive resolves qty to 150 (anc=100, a=120 (+20), b=130 (+30)).
    // Invariant clamps to 100. What adapters actually store must be the
    // TRANSFORMED value, not the raw resolved one — verified through the
    // facade's sync().
    let erp = TestMemoryAdapter::new("erp");
    let inv = TestMemoryAdapter::new("inv");
    erp.seed(ENTITY, ID, json!({"qty": 120}));
    inv.seed(ENTITY, ID, json!({"qty": 130}));

    let engine = SyncEngine::builder(erp.clone(), inv.clone())
        .policy("qty", Box::new(Additive))
        .invariant(Box::new(CapAt100))
        .seed_ancestor(ENTITY, ID, json!({"qty": 100}))
        .build();

    let outcome = engine.sync(ENTITY, ID).await.unwrap();
    assert!(matches!(outcome, SyncOutcome::Synced { .. }));

    for (port, label) in [(&erp, "erp"), (&inv, "inv")] {
        let ext = port.find_by_canonical_id(ENTITY, ID).await.unwrap().unwrap();
        let (val, _) = port.fetch(ENTITY, &ext).await.unwrap();
        assert_eq!(
            val, json!({"qty": 100.0}),
            "{label} must reflect the transformed value, not 150"
        );
    }
}

#[tokio::test]
async fn preview_surfaces_conflict_class() {
    // Preview should return the same class information as sync would,
    // without writing anything — useful for dry-run audits.
    let (erp, inv) = seed_pair();
    let engine = SyncEngine::builder(erp, inv).build();

    let preview = engine.preview(ENTITY, ID).await.unwrap();
    assert!(preview.would_write.is_none(), "no policies → no merged value");
    assert!(!preview.conflicts.is_empty());
    assert!(
        preview
            .conflicts
            .iter()
            .all(|c| c.class == ConflictClass::NoPolicy)
    );
}

#[tokio::test]
async fn unregistered_path_surfaces_no_policy_class() {
    let (erp, inv) = seed_pair();
    let engine = SyncEngine::builder(erp, inv).build();

    match engine.sync(ENTITY, ID).await.unwrap() {
        SyncOutcome::Escalated { conflicts } => {
            assert!(
                conflicts.iter().all(|c| c.class == ConflictClass::NoPolicy),
                "all conflicts must carry NoPolicy class, got {:?}",
                conflicts.iter().map(|c| c.class).collect::<Vec<_>>()
            );
        }
        other => panic!("expected Escalated, got {other:?}"),
    }
}

#[tokio::test]
async fn policy_rejection_surfaces_policy_conflict_class() {
    // Two sides modify 'price' without an OwnedBy/etc. to resolve it.
    // We install a policy that returns Conflict (StateMachine with no
    // matching transition); the class should be PolicyConflict.
    let erp = TestMemoryAdapter::new("erp");
    let inv = TestMemoryAdapter::new("inv");
    erp.seed(ENTITY, ID, json!({"status": "closed"}));
    inv.seed(ENTITY, ID, json!({"status": "draft"}));

    let engine = SyncEngine::builder(erp, inv)
        .policy(
            "status",
            Box::new(diff_fusion::application::policy::StateMachine::new([
                // Only open→closed allowed; ancestor (bootstrapped from A)
                // is "closed", B diverged to "draft", no transition exists.
                diff_fusion::application::policy::StateTransition::new("open", "closed"),
            ])),
        )
        .build();

    match engine.sync(ENTITY, ID).await.unwrap() {
        SyncOutcome::Escalated { conflicts } => {
            assert!(
                conflicts
                    .iter()
                    .any(|c| c.class == ConflictClass::PolicyConflict),
                "expected PolicyConflict class, got {:?}",
                conflicts.iter().map(|c| c.class).collect::<Vec<_>>()
            );
        }
        other => panic!("expected Escalated, got {other:?}"),
    }
}

struct RejectAll;
impl Invariant for RejectAll {
    fn name(&self) -> &'static str {
        "reject_all"
    }
    fn check(&self, _prev: &Value, _cand: &Value) -> InvariantOutcome {
        InvariantOutcome::Reject {
            reason: "by design".into(),
        }
    }
}

#[tokio::test]
async fn invariant_rejection_surfaces_invariant_violation_class() {
    let erp = TestMemoryAdapter::new("erp");
    let inv = TestMemoryAdapter::new("inv");
    erp.seed(ENTITY, ID, json!({"price": 15}));
    inv.seed(ENTITY, ID, json!({"price": 10}));

    let engine = SyncEngine::builder(erp, inv)
        .policy("price", Box::new(OwnedBy::new("erp")))
        .invariant(Box::new(RejectAll))
        .seed_ancestor(ENTITY, ID, json!({"price": 10}))
        .build();

    match engine.sync(ENTITY, ID).await.unwrap() {
        SyncOutcome::Escalated { conflicts } => {
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].class, ConflictClass::InvariantViolation);
        }
        other => panic!("expected Escalated, got {other:?}"),
    }
}

#[tokio::test]
async fn unregistered_conflict_is_visible_without_knowing_inner_types() {
    // No policy declared — the change must escalate and the facade
    // must expose enough of the conflict for the caller to act on it
    // (path + reason) without reaching for UnresolvedConflict's inner
    // fields or Changelog.
    let (erp, inv) = seed_pair();

    let engine = SyncEngine::builder(erp, inv).build();

    let outcome = engine.sync(ENTITY, ID).await.unwrap();
    match outcome {
        SyncOutcome::Escalated { conflicts } => {
            assert!(
                conflicts.iter().any(|c| c.path == "price" || c.path == "qty_recv"),
                "facade conflict must expose a readable path"
            );
        }
        other => panic!("expected Escalated, got {other:?}"),
    }
}
