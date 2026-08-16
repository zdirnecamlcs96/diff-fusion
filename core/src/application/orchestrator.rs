//! The cycle — one pass of pull → three-way diff → resolve → push → commit.
//!
//! Implements the seven steps from `App.md` § 05 in their canonical order.
//! The ancestor update is always **last**: if any push fails, the ancestor
//! stays put and the next cycle retries cleanly. Advancing the ancestor
//! before pushes confirm is how silent drift creeps in over time.
//!
//! # Guarantees
//!
//! - An empty changelog (no side moved) never writes anything.
//! - Unresolved conflicts block both the pushes *and* the ancestor update.
//! - Every push carries a deterministic idempotency key so retries collapse.
//! - Every push asserts the version the orchestrator last observed; mismatch
//!   raises [`SyncError::StaleWrite`] and the cycle aborts.
//!
//! # Shadow mode
//!
//! [`Orchestrator::run_shadow`] performs the diff and resolution but skips
//! the push and ancestor update. Use it to let a new adapter run alongside
//! production for a week and review the changelog it *would* have applied.

use crate::ports::ancestor::{AncestorEntry, AncestorKey, AncestorStore};
use crate::domain::diff::{Changelog, three_way_diff};
use crate::domain::error::SyncError;
use crate::ports::escalation::{EscalationItem, EscalationQueue};
use crate::ports::policy_store::PolicyStore;
use crate::domain::idempotency::idempotency_key;
use crate::application::policy::{
    InvariantOutcome, InvariantSet, MergeContext, OwnedBy, PolicyMap, Resolution,
    UnresolvedConflict, resolve,
};
use crate::ports::system::SystemPort;
use serde_json::Value;
use std::sync::Arc;

/// Terminal result of one cycle.
#[derive(Debug, Clone, PartialEq)]
pub enum CycleOutcome {
    /// Neither side moved since the ancestor — skipped without any writes.
    NoOp,
    /// Cycle completed cleanly. `pushed_to` lists the system types that
    /// received a write (may be empty if resolution happened to match both
    /// current views).
    Synced { pushed_to: Vec<String> },
    /// Resolution surfaced conflicts. The ancestor was not advanced and
    /// nothing was pushed. The orchestrator already enqueued the item in
    /// the escalation queue.
    Escalated { conflicts: Vec<UnresolvedConflict> },
}

/// Output of shadow mode — what the cycle *would* have done.
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowReport {
    pub changelog: Changelog,
    pub resolution: Resolution,
    /// Canonical value the cycle would have written, or `None` if it would
    /// have escalated.
    pub would_write: Option<Value>,
}

/// Holds the two sides, the ancestor store, the policy map, and the
/// escalation queue. A single [`Orchestrator`] runs many cycles over time.
pub struct Orchestrator<A: SystemPort, B: SystemPort> {
    pub side_a: A,
    pub side_b: B,
    pub ancestor: Arc<dyn AncestorStore>,
    pub policies: PolicyMap,
    pub escalation: Arc<dyn EscalationQueue>,
    /// Tier-2 post-merge invariants. Defaults to an empty set; add one
    /// via [`Orchestrator::with_invariants`].
    pub invariants: InvariantSet,
    /// Host-backed policy source. When set, loaded fresh once per cycle
    /// and used in place of `policies` — see [`Orchestrator::with_policy_store`].
    pub policy_store: Option<Arc<dyn PolicyStore>>,
}

impl<A: SystemPort, B: SystemPort> Orchestrator<A, B> {
    pub fn new(
        side_a: A,
        side_b: B,
        ancestor: Arc<dyn AncestorStore>,
        policies: PolicyMap,
        escalation: Arc<dyn EscalationQueue>,
    ) -> Self {
        Self {
            side_a,
            side_b,
            ancestor,
            policies,
            escalation,
            invariants: InvariantSet::default(),
            policy_store: None,
        }
    }

    /// Attach a set of Tier-2 invariants. They run after Tier-1 resolution
    /// produces a candidate merged value: a `Reject` blocks the cycle and
    /// escalates; a `Transform` rewrites the value before push.
    pub fn with_invariants(mut self, invariants: InvariantSet) -> Self {
        self.invariants = invariants;
        self
    }

    /// Attach a host-backed policy store. When set, every cycle loads a
    /// fresh [`PolicyMap`] from it and uses that in place of `policies` —
    /// the store fully replaces the static map, it does not merge with it.
    pub fn with_policy_store(mut self, store: Arc<dyn PolicyStore>) -> Self {
        self.policy_store = Some(store);
        self
    }

    /// Construct a one-way orchestrator: `side_a` is the source of truth,
    /// `side_b` mirrors it. Target-side edits revert on the next cycle
    /// (Synology's "download only" semantics, expressed as an
    /// [`crate::application::policy::OwnedBy`] default).
    ///
    /// Call [`PolicyMap::with`] on the returned orchestrator's `policies`
    /// field before construction if you want per-path exceptions.
    pub fn one_way(
        side_a: A,
        side_b: B,
        ancestor: Arc<dyn AncestorStore>,
        escalation: Arc<dyn EscalationQueue>,
    ) -> Self {
        let source_name = side_a.system_type().to_string();
        let policies = PolicyMap::new().with_default(Box::new(OwnedBy::new(source_name)));
        Self::new(side_a, side_b, ancestor, policies, escalation)
    }

    /// Execute one full cycle. `now_ms` is the wall-clock timestamp
    /// stamped onto the ancestor on success; tests pass a fixed value.
    pub async fn run_cycle_at(
        &self,
        entity_type: &str,
        canonical_id: &str,
        now_ms: u64,
    ) -> Result<CycleOutcome, SyncError> {
        let prepared = self.prepare(entity_type, canonical_id).await?;

        if prepared.changelog.is_empty() {
            return Ok(CycleOutcome::NoOp);
        }

        let ctx = MergeContext::new(self.side_a.system_type(), self.side_b.system_type());
        let loaded = self.load_policy_override(entity_type)?;
        let resolution = resolve(
            &prepared.changelog,
            loaded.as_ref().unwrap_or(&self.policies),
            &ctx,
        );

        if !resolution.is_clean() {
            self.enqueue_escalation(
                entity_type,
                canonical_id,
                resolution.conflicts.clone(),
                now_ms,
            )?;
            return Ok(CycleOutcome::Escalated {
                conflicts: resolution.conflicts,
            });
        }

        let candidate = apply_resolution(&prepared.ancestor_view, &resolution);

        // Tier-2: post-merge invariants. A Transform rewrites the value;
        // a Reject blocks the cycle — the Tier-1 result was structurally
        // valid but violated a rule about entity state.
        let merged = match self.invariants.apply(&prepared.ancestor_view, &candidate) {
            InvariantOutcome::Pass => candidate,
            InvariantOutcome::Transform(v) => v,
            InvariantOutcome::Reject { reason } => {
                let conflicts =
                    vec![invariant_conflict(reason, &prepared.ancestor_view, &candidate)];
                self.enqueue_escalation(entity_type, canonical_id, conflicts.clone(), now_ms)?;
                return Ok(CycleOutcome::Escalated { conflicts });
            }
        };

        // Push stale sides. Deterministic A-then-B order keeps logs sane;
        // a mid-sequence failure leaves the ancestor untouched so the next
        // cycle re-derives everything.
        let mut pushed_to = Vec::new();
        if merged != prepared.view_a {
            self.push_to(
                &self.side_a,
                entity_type,
                canonical_id,
                &merged,
                &prepared.fresh_ref_a,
            )
            .await?;
            pushed_to.push(self.side_a.system_type().to_string());
        }
        if merged != prepared.view_b {
            self.push_to(
                &self.side_b,
                entity_type,
                canonical_id,
                &merged,
                &prepared.fresh_ref_b,
            )
            .await?;
            pushed_to.push(self.side_b.system_type().to_string());
        }

        // Commit the new ancestor LAST. Every earlier step retries
        // idempotently; this one must not happen until both sides confirmed.
        self.ancestor
            .put(
                AncestorKey::new(entity_type, canonical_id),
                AncestorEntry::new(merged, now_ms),
            )
            .map_err(|e| SyncError::transient(e.to_string()))?;

        Ok(CycleOutcome::Synced { pushed_to })
    }

    /// Fresh policy map from `policy_store` for this cycle, or `None` if
    /// no store is configured — callers fall back to `self.policies`. The
    /// store fully replaces the static map; there is no merge between the
    /// two, and nothing is cached across cycles.
    fn load_policy_override(&self, entity_type: &str) -> Result<Option<PolicyMap>, SyncError> {
        match &self.policy_store {
            Some(store) => Ok(Some(
                store
                    .load(entity_type)
                    .map_err(|e| SyncError::transient(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    fn enqueue_escalation(
        &self,
        entity_type: &str,
        canonical_id: &str,
        conflicts: Vec<UnresolvedConflict>,
        now_ms: u64,
    ) -> Result<(), SyncError> {
        self.escalation
            .push(EscalationItem {
                entity_type: entity_type.into(),
                canonical_id: canonical_id.into(),
                conflicts,
                created_at_ms: now_ms,
            })
            .map_err(|e| SyncError::transient(e.to_string()))
    }

    async fn push_to<P: SystemPort>(
        &self,
        side: &P,
        entity_type: &str,
        canonical_id: &str,
        merged: &Value,
        fresh_ref: &crate::ports::system::ExternalRef,
    ) -> Result<(), SyncError> {
        let ik = idempotency_key(canonical_id, "upsert", merged);
        side.upsert(
            entity_type,
            canonical_id,
            merged,
            fresh_ref.version.as_deref(),
            &ik,
        )
        .await?;
        Ok(())
    }

    /// Shadow mode — reports what a cycle would do without writing anywhere.
    pub async fn run_shadow(
        &self,
        entity_type: &str,
        canonical_id: &str,
    ) -> Result<ShadowReport, SyncError> {
        let prepared = self.prepare(entity_type, canonical_id).await?;

        if prepared.changelog.is_empty() {
            return Ok(ShadowReport {
                changelog: prepared.changelog,
                resolution: Resolution::default(),
                would_write: None,
            });
        }

        let ctx = MergeContext::new(self.side_a.system_type(), self.side_b.system_type());
        let loaded = self.load_policy_override(entity_type)?;
        let resolution = resolve(
            &prepared.changelog,
            loaded.as_ref().unwrap_or(&self.policies),
            &ctx,
        );

        let would_write = if resolution.is_clean() {
            Some(apply_resolution(&prepared.ancestor_view, &resolution))
        } else {
            None
        };

        Ok(ShadowReport {
            changelog: prepared.changelog,
            resolution,
            would_write,
        })
    }

    async fn prepare(
        &self,
        entity_type: &str,
        canonical_id: &str,
    ) -> Result<PreparedCycle, SyncError> {
        let ref_a = self
            .side_a
            .find_by_canonical_id(entity_type, canonical_id)
            .await?
            .ok_or_else(|| {
                SyncError::transient(format!(
                    "entity {canonical_id} not found on {}",
                    self.side_a.system_type()
                ))
            })?;
        let ref_b = self
            .side_b
            .find_by_canonical_id(entity_type, canonical_id)
            .await?
            .ok_or_else(|| {
                SyncError::transient(format!(
                    "entity {canonical_id} not found on {}",
                    self.side_b.system_type()
                ))
            })?;

        let (view_a, fresh_ref_a) = self.side_a.fetch(entity_type, &ref_a).await?;
        let (view_b, fresh_ref_b) = self.side_b.fetch(entity_type, &ref_b).await?;

        // Bootstrap (App.md § 05 row iii): missing ancestor treats A's
        // current view as the baseline so the first cycle propagates A→B.
        let ancestor_view = self
            .ancestor
            .get(&AncestorKey::new(entity_type, canonical_id))
            .map_err(|e| SyncError::transient(e.to_string()))?
            .map(|e| e.canonical)
            .unwrap_or_else(|| view_a.clone());

        let changelog = three_way_diff(&ancestor_view, &view_a, &view_b);

        Ok(PreparedCycle {
            view_a,
            view_b,
            fresh_ref_a,
            fresh_ref_b,
            ancestor_view,
            changelog,
        })
    }
}

/// Wall-clock helper for tests that don't care about determinism. Real
/// callers should supply their own timestamps.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

struct PreparedCycle {
    view_a: Value,
    view_b: Value,
    fresh_ref_a: crate::ports::system::ExternalRef,
    fresh_ref_b: crate::ports::system::ExternalRef,
    ancestor_view: Value,
    changelog: Changelog,
}

/// Start from the ancestor and overlay every `(path, value)` resolved by the
/// policies. Paths are dotted object keys (matching the three-way diff's
/// output). Intermediate objects are created when missing so resolutions
/// onto previously-unset nested fields work.
fn apply_resolution(base: &Value, resolution: &Resolution) -> Value {
    let mut out = base.clone();
    for (path, value) in &resolution.resolved {
        crate::domain::json_path::set_at_path(&mut out, path, value.clone());
    }
    out
}

/// Build the synthetic conflict that represents a Tier-2 invariant rejection.
fn invariant_conflict(
    reason: String,
    ancestor_view: &Value,
    candidate: &Value,
) -> UnresolvedConflict {
    UnresolvedConflict {
        path: String::new(),
        reason,
        class: crate::application::policy::ConflictClass::InvariantViolation,
        change: crate::domain::diff::FieldChange {
            path: String::new(),
            old_value: ancestor_view.clone(),
            new_from_a: Some(candidate.clone()),
            new_from_b: None,
            source: crate::domain::diff::ChangeSource::Both,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // set_at_path primitives are tested in crate::domain::json_path; this
    // module tests only the orchestrator-specific overlay logic.

    #[test]
    fn apply_resolution_overlays_multiple_paths() {
        let base = json!({"price": 10, "qty": 5});
        let res = Resolution {
            resolved: vec![
                ("price".into(), json!(20)),
                ("qty".into(), json!(8.0)),
            ],
            conflicts: vec![],
        };
        assert_eq!(
            apply_resolution(&base, &res),
            json!({"price": 20, "qty": 8.0})
        );
    }

    #[test]
    fn apply_resolution_is_a_noop_on_empty_resolution() {
        let base = json!({"price": 10});
        let res = Resolution::default();
        assert_eq!(apply_resolution(&base, &res), base);
    }

    #[test]
    fn apply_resolution_creates_nested_paths() {
        let base = json!({});
        let res = Resolution {
            resolved: vec![("pricing.amount".into(), json!(42))],
            conflicts: vec![],
        };
        assert_eq!(
            apply_resolution(&base, &res),
            json!({"pricing": {"amount": 42}})
        );
    }

    // -- policy_store seam ---------------------------------------------
    // Minimal stubs (never invoked) so an Orchestrator can be constructed
    // without depending on the adapters layer.

    use crate::ports::ancestor::{AncestorEntry, AncestorKey, AncestorStoreError};
    use crate::ports::escalation::{EscalationError, EscalationItem};
    use crate::ports::policy_store::PolicyStoreError;
    use crate::ports::system::ExternalRef;

    struct StubSystem;
    #[async_trait::async_trait]
    impl SystemPort for StubSystem {
        fn system_type(&self) -> &str {
            "stub"
        }
        async fn fetch(
            &self,
            _entity_type: &str,
            _ext: &ExternalRef,
        ) -> Result<(Value, ExternalRef), SyncError> {
            unimplemented!("not exercised by load_policy_override tests")
        }
        async fn find_by_canonical_id(
            &self,
            _entity_type: &str,
            _canonical_id: &str,
        ) -> Result<Option<ExternalRef>, SyncError> {
            unimplemented!("not exercised by load_policy_override tests")
        }
        async fn upsert(
            &self,
            _entity_type: &str,
            _canonical_id: &str,
            _canonical: &Value,
            _expect_version: Option<&str>,
            _idempotency_key: &[u8; 32],
        ) -> Result<ExternalRef, SyncError> {
            unimplemented!("not exercised by load_policy_override tests")
        }
    }

    struct StubAncestor;
    impl AncestorStore for StubAncestor {
        fn get(&self, _key: &AncestorKey) -> Result<Option<AncestorEntry>, AncestorStoreError> {
            unimplemented!()
        }
        fn put(&self, _key: AncestorKey, _entry: AncestorEntry) -> Result<(), AncestorStoreError> {
            unimplemented!()
        }
    }

    struct StubEscalation;
    impl EscalationQueue for StubEscalation {
        fn push(&self, _item: EscalationItem) -> Result<(), EscalationError> {
            unimplemented!()
        }
        fn len(&self) -> usize {
            0
        }
    }

    struct StubPolicyStore;
    impl PolicyStore for StubPolicyStore {
        fn load(&self, entity_type: &str) -> Result<PolicyMap, PolicyStoreError> {
            Ok(PolicyMap::new().with_default(Box::new(OwnedBy::new(entity_type))))
        }
    }

    fn build_orchestrator() -> Orchestrator<StubSystem, StubSystem> {
        Orchestrator::new(
            StubSystem,
            StubSystem,
            Arc::new(StubAncestor),
            PolicyMap::new(),
            Arc::new(StubEscalation),
        )
    }

    #[test]
    fn load_policy_override_is_none_without_a_configured_store() {
        let orch = build_orchestrator();
        assert!(
            orch.load_policy_override("purchase_order")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn load_policy_override_loads_from_the_store_when_configured() {
        let orch = build_orchestrator().with_policy_store(Arc::new(StubPolicyStore));
        assert!(
            orch.load_policy_override("purchase_order")
                .unwrap()
                .is_some()
        );
    }
}
