//! `SyncEngine` — the facade for the reconciliation pipeline.
//!
//! This is the only type most users should need to touch. It hides the
//! moving parts of a full sync (ancestor store, escalation queue,
//! orchestrator wiring, `Arc` plumbing, trait-object casts) behind a
//! builder API. Advanced users can still reach for
//! [`crate::application::orchestrator::Orchestrator`] directly.
//!
//! # Minimal example
//!
//! ```no_run
//! use diff_fusion::drivers::sync_engine::SyncEngine;
//! use diff_fusion::application::policy::{Additive, OwnedBy};
//! use diff_fusion::adapters::test_memory::TestMemoryAdapter;
//!
//! # async fn run() {
//! let engine = SyncEngine::builder(
//!         TestMemoryAdapter::new("erp"),
//!         TestMemoryAdapter::new("inv"),
//!     )
//!     .policy("price",    Box::new(OwnedBy::new("erp")))
//!     .policy("qty_recv", Box::new(Additive))
//!     .build();
//!
//! let outcome = engine.sync("purchase_order", "PO-42").await.unwrap();
//! # }
//! ```
//!
//! # What the facade provides
//!
//! - Builder API for policies, invariants, ancestor seeding.
//! - Sensible defaults: in-memory ancestor store, in-memory escalation
//!   queue. Override either with `.ancestor_store(...)` /
//!   `.escalation_queue(...)` when deploying for real.
//! - `sync` / `preview` methods — the two operations a user cares about.
//! - `escalation_depth` — peek at pending conflicts without owning the
//!   queue type.

use crate::adapters::in_memory_ancestor::InMemoryAncestorStore;
use crate::adapters::in_memory_escalation::InMemoryEscalationQueue;
use crate::application::orchestrator::{CycleOutcome, Orchestrator, ShadowReport, now_ms};
use crate::application::policy::{Invariant, InvariantSet, MergePolicy, OwnedBy, PolicyMap};
use crate::domain::error::SyncError;
use crate::ports::ancestor::{AncestorEntry, AncestorKey, AncestorStore};
use crate::ports::escalation::EscalationQueue;
use crate::ports::policy_store::PolicyStore;
use crate::ports::system::SystemPort;
use serde_json::Value;
use std::sync::Arc;

/// Reconciliation facade. Build with [`SyncEngine::builder`].
pub struct SyncEngine<A: SystemPort, B: SystemPort> {
    orchestrator: Orchestrator<A, B>,
    escalation: Arc<dyn EscalationQueue>,
}

impl<A: SystemPort, B: SystemPort> SyncEngine<A, B> {
    /// Start building an engine from two adapters.
    pub fn builder(side_a: A, side_b: B) -> SyncEngineBuilder<A, B> {
        SyncEngineBuilder::new(side_a, side_b)
    }

    /// Run one reconciliation cycle for the given entity. Uses the
    /// system clock for the ancestor timestamp.
    pub async fn sync(
        &self,
        entity_type: &str,
        canonical_id: &str,
    ) -> Result<CycleOutcome, SyncError> {
        self.orchestrator
            .run_cycle_at(entity_type, canonical_id, now_ms())
            .await
    }

    /// Preview what `sync` would do, without writing or advancing the
    /// ancestor. The equivalent of a dry run.
    pub async fn preview(
        &self,
        entity_type: &str,
        canonical_id: &str,
    ) -> Result<ShadowReport, SyncError> {
        self.orchestrator.run_shadow(entity_type, canonical_id).await
    }

    /// How many items currently sit in the escalation queue.
    pub fn escalation_depth(&self) -> usize {
        self.escalation.len()
    }
}

/// Builder for [`SyncEngine`]. Use [`SyncEngine::builder`] to obtain one.
pub struct SyncEngineBuilder<A: SystemPort, B: SystemPort> {
    side_a: A,
    side_b: B,
    policies: PolicyMap,
    invariants: InvariantSet,
    ancestor: Option<Arc<dyn AncestorStore>>,
    /// Concrete handle for the default ancestor store, used by
    /// `seed_ancestor`. None when the caller supplied a custom store.
    default_ancestor: Option<Arc<InMemoryAncestorStore>>,
    escalation: Option<Arc<dyn EscalationQueue>>,
    policy_store: Option<Arc<dyn PolicyStore>>,
}

impl<A: SystemPort, B: SystemPort> SyncEngineBuilder<A, B> {
    fn new(side_a: A, side_b: B) -> Self {
        Self {
            side_a,
            side_b,
            policies: PolicyMap::new(),
            invariants: InvariantSet::new(),
            ancestor: None,
            default_ancestor: None,
            escalation: None,
            policy_store: None,
        }
    }

    /// Install a per-path merge policy.
    pub fn policy(mut self, path: impl Into<String>, policy: Box<dyn MergePolicy>) -> Self {
        self.policies = self.policies.with(path.into(), policy);
        self
    }

    /// Add a Tier-2 post-merge invariant.
    pub fn invariant(mut self, invariant: Box<dyn Invariant>) -> Self {
        self.invariants = self.invariants.with(invariant);
        self
    }

    /// One-way sync preset: `side_a` becomes the source of truth; any
    /// field not overridden by a subsequent `policy(...)` call is owned
    /// by `side_a`. Target-side edits revert on the next cycle
    /// (Synology-style "download only" semantics).
    pub fn one_way(mut self) -> Self {
        let source = self.side_a.system_type().to_string();
        self.policies = PolicyMap::new().with_default(Box::new(OwnedBy::new(source)));
        self
    }

    /// Supply a custom ancestor store. Without this call the engine uses
    /// an in-memory default (suitable for tests; not durable).
    pub fn ancestor_store(mut self, store: Arc<dyn AncestorStore>) -> Self {
        self.ancestor = Some(store);
        self.default_ancestor = None;
        self
    }

    /// Supply a custom escalation queue. Without this call the engine
    /// uses an in-memory default.
    pub fn escalation_queue(mut self, queue: Arc<dyn EscalationQueue>) -> Self {
        self.escalation = Some(queue);
        self
    }

    /// Supply a host-backed policy store. When set, every cycle loads a
    /// fresh policy map from it in place of the policies installed via
    /// `.policy(...)` — no effect unless configured.
    pub fn policy_store(mut self, store: Arc<dyn PolicyStore>) -> Self {
        self.policy_store = Some(store);
        self
    }

    /// Pre-populate the (default, in-memory) ancestor store with a
    /// known baseline. Useful in tests and for explicit initial-sync
    /// seeding. No effect when a custom ancestor store was supplied.
    pub fn seed_ancestor(
        mut self,
        entity_type: impl Into<String>,
        canonical_id: impl Into<String>,
        canonical: Value,
    ) -> Self {
        let store = self.ensure_default_ancestor();
        store
            .put(
                AncestorKey::new(entity_type, canonical_id),
                AncestorEntry::new(canonical, 0),
            )
            .expect("in-memory store does not fail");
        self
    }

    fn ensure_default_ancestor(&mut self) -> Arc<InMemoryAncestorStore> {
        if let Some(existing) = &self.default_ancestor {
            return existing.clone();
        }
        let fresh = Arc::new(InMemoryAncestorStore::new());
        self.default_ancestor = Some(fresh.clone());
        self.ancestor = Some(fresh.clone() as Arc<dyn AncestorStore>);
        fresh
    }

    /// Validate the installed policies against a CIF schema before the
    /// first cycle runs. For each registered policy, the field at
    /// `schema.cif_schema.<path>` is inspected — `SetByKey` verifies that
    /// its `a_anchor` / `b_anchor` fields exist with the matching
    /// `anchor` roles, its `identity` fields exist, and any `nested`
    /// policies line up with declared sub-arrays.
    ///
    /// Returns `Err(errors)` if any misalignment is found. Each message
    /// is prefixed with the policy path. Call before `build()` to
    /// fail fast on misconfigured policies rather than at first cycle.
    pub fn validate_against_schema(self, schema: &Value) -> Result<Self, Vec<String>> {
        let errors = self.policies.validate_against_schema(schema);
        if errors.is_empty() {
            Ok(self)
        } else {
            Err(errors)
        }
    }

    pub fn build(mut self) -> SyncEngine<A, B> {
        let ancestor = self.ancestor.take().unwrap_or_else(|| {
            let fresh = Arc::new(InMemoryAncestorStore::new());
            fresh as Arc<dyn AncestorStore>
        });
        let escalation = self
            .escalation
            .take()
            .unwrap_or_else(|| Arc::new(InMemoryEscalationQueue::new()) as Arc<dyn EscalationQueue>);

        let mut orchestrator = Orchestrator::new(
            self.side_a,
            self.side_b,
            ancestor,
            self.policies,
            escalation.clone(),
        )
        .with_invariants(self.invariants);
        if let Some(store) = self.policy_store.take() {
            orchestrator = orchestrator.with_policy_store(store);
        }

        SyncEngine {
            orchestrator,
            escalation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::in_memory_policy_store::InMemoryPolicyStore;
    use crate::adapters::test_memory::TestMemoryAdapter;
    use crate::application::policy::Additive;
    use serde_json::json;

    #[tokio::test]
    async fn smoke_builds_and_runs() {
        let a = TestMemoryAdapter::new("erp");
        let b = TestMemoryAdapter::new("inv");
        a.seed("e", "1", json!({"q": 11}));
        b.seed("e", "1", json!({"q": 12}));

        let engine = SyncEngine::builder(a, b)
            .policy("q", Box::new(Additive))
            .seed_ancestor("e", "1", json!({"q": 10}))
            .build();

        let out = engine.sync("e", "1").await.unwrap();
        assert!(matches!(out, CycleOutcome::Synced { .. }));
    }

    #[tokio::test]
    async fn one_way_shortcut_sets_owned_by_default() {
        let a = TestMemoryAdapter::new("source");
        let b = TestMemoryAdapter::new("target");
        a.seed("e", "1", json!({"x": 42}));
        b.seed("e", "1", json!({"x": 99}));

        let engine = SyncEngine::builder(a, b).one_way().build();
        let out = engine.sync("e", "1").await.unwrap();
        assert!(matches!(out, CycleOutcome::Synced { .. }));
    }

    #[tokio::test]
    async fn policy_store_supplies_policies_the_builder_never_declared() {
        let a = TestMemoryAdapter::new("erp");
        let b = TestMemoryAdapter::new("inv");
        a.seed("e", "1", json!({"q": 11}));
        b.seed("e", "1", json!({"q": 12}));

        let store = Arc::new(InMemoryPolicyStore::new());
        store
            .set_json("e", &json!({"fields": {"q": {"kind": "additive"}}}))
            .unwrap();

        // No `.policy("q", ...)` call — the store is the only source of
        // a policy for "q", so a clean Synced outcome proves it was used.
        let engine = SyncEngine::builder(a, b)
            .policy_store(store)
            .seed_ancestor("e", "1", json!({"q": 10}))
            .build();

        let out = engine.sync("e", "1").await.unwrap();
        assert!(matches!(out, CycleOutcome::Synced { .. }));
    }
}
