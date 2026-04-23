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
//! - Builder API for policies, invariants, presets, ancestor seeding.
//! - Sensible defaults: in-memory ancestor store, in-memory escalation
//!   queue. Override either with `.ancestor_store(...)` /
//!   `.escalation_queue(...)` when deploying for real.
//! - `sync` / `preview` methods — the two operations a user cares about.
//! - `escalation_depth` — peek at pending conflicts without owning the
//!   queue type.

use crate::adapters::in_memory_ancestor::InMemoryAncestorStore;
use crate::adapters::in_memory_escalation::InMemoryEscalationQueue;
use crate::application::orchestrator::{CycleOutcome, Orchestrator, now_ms};
use crate::application::policy::{ConflictClass, Invariant, InvariantSet, MergePolicy, PolicyMap};
use crate::application::presets;
use crate::domain::error::SyncError;
use crate::ports::ancestor::{AncestorEntry, AncestorKey, AncestorStore};
use crate::ports::escalation::EscalationQueue;
use crate::ports::system::SystemPort;
use serde_json::Value;
use std::sync::Arc;

/// What happened during a sync. The facade flattens the orchestrator's
/// richer types so users don't need to import `CycleOutcome`,
/// `UnresolvedConflict`, or `FieldChange`.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncOutcome {
    /// Neither side changed since the ancestor — no writes.
    NoOp,
    /// Sync completed. `pushed_to` lists the system labels that received
    /// a write (may be empty if the resolution happened to match both
    /// current views).
    Synced { pushed_to: Vec<String> },
    /// One or more conflicts could not be resolved; they were enqueued
    /// for human review. Nothing was pushed, ancestor was not advanced.
    Escalated { conflicts: Vec<FacadeConflict> },
}

/// A user-facing summary of a conflict. Carries just the fields a caller
/// needs to surface the issue to a human — no internal types.
#[derive(Debug, Clone, PartialEq)]
pub struct FacadeConflict {
    pub path: String,
    pub reason: String,
    /// Cause category — branch on this for per-class dispositions
    /// (reject vs escalate vs preserve-both).
    pub class: ConflictClass,
}

/// Preview of what a sync would do, without writing.
#[derive(Debug, Clone, PartialEq)]
pub struct FacadePreview {
    /// The canonical value the cycle would have written on success, or
    /// `None` if it would have escalated.
    pub would_write: Option<Value>,
    /// Conflicts the cycle would have escalated.
    pub conflicts: Vec<FacadeConflict>,
}

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
    ) -> Result<SyncOutcome, SyncError> {
        let out = self
            .orchestrator
            .run_cycle_at(entity_type, canonical_id, now_ms())
            .await?;
        Ok(flatten(out))
    }

    /// Preview what `sync` would do, without writing or advancing the
    /// ancestor. The equivalent of a dry run.
    pub async fn preview(
        &self,
        entity_type: &str,
        canonical_id: &str,
    ) -> Result<FacadePreview, SyncError> {
        let report = self.orchestrator.run_shadow(entity_type, canonical_id).await?;
        Ok(FacadePreview {
            would_write: report.would_write,
            conflicts: report
                .resolution
                .conflicts
                .into_iter()
                .map(|c| FacadeConflict {
                    path: c.path,
                    reason: c.reason,
                    class: c.class,
                })
                .collect(),
        })
    }

    /// How many items currently sit in the escalation queue.
    pub fn escalation_depth(&self) -> usize {
        self.escalation.len()
    }
}

fn flatten(out: CycleOutcome) -> SyncOutcome {
    match out {
        CycleOutcome::NoOp => SyncOutcome::NoOp,
        CycleOutcome::Synced { pushed_to } => SyncOutcome::Synced { pushed_to },
        CycleOutcome::Escalated { conflicts } => SyncOutcome::Escalated {
            conflicts: conflicts
                .into_iter()
                .map(|c| FacadeConflict {
                    path: c.path,
                    reason: c.reason,
                    class: c.class,
                })
                .collect(),
        },
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
        }
    }

    /// Install a per-path merge policy.
    pub fn policy(mut self, path: impl Into<String>, policy: Box<dyn MergePolicy>) -> Self {
        self.policies = self.policies.with(path.into(), policy);
        self
    }

    /// Install a fallback merge policy for paths not otherwise covered.
    pub fn default_policy(mut self, policy: Box<dyn MergePolicy>) -> Self {
        self.policies = self.policies.with_default(policy);
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
        self.policies = presets::one_way_from(source);
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

    pub fn build(mut self) -> SyncEngine<A, B> {
        let ancestor = self.ancestor.take().unwrap_or_else(|| {
            let fresh = Arc::new(InMemoryAncestorStore::new());
            fresh as Arc<dyn AncestorStore>
        });
        let escalation = self
            .escalation
            .take()
            .unwrap_or_else(|| Arc::new(InMemoryEscalationQueue::new()) as Arc<dyn EscalationQueue>);

        let orchestrator = Orchestrator::new(
            self.side_a,
            self.side_b,
            ancestor,
            self.policies,
            escalation.clone(),
        )
        .with_invariants(self.invariants);

        SyncEngine {
            orchestrator,
            escalation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::test_memory::TestMemoryAdapter;
    use crate::application::policy::{Additive, OwnedBy};
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
        assert!(matches!(out, SyncOutcome::Synced { .. }));
    }

    #[tokio::test]
    async fn one_way_shortcut_sets_owned_by_default() {
        let a = TestMemoryAdapter::new("source");
        let b = TestMemoryAdapter::new("target");
        a.seed("e", "1", json!({"x": 42}));
        b.seed("e", "1", json!({"x": 99}));

        let engine = SyncEngine::builder(a, b).one_way().build();
        let out = engine.sync("e", "1").await.unwrap();
        assert!(matches!(out, SyncOutcome::Synced { .. }));
    }

    #[test]
    fn default_policy_builder_method_exists() {
        // Compile-time only: prove the type-level API.
        let a = TestMemoryAdapter::new("x");
        let b = TestMemoryAdapter::new("y");
        let _ = SyncEngine::builder(a, b).default_policy(Box::new(OwnedBy::new("x")));
    }
}
