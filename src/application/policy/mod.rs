//! Per-field merge policies — Tier 1 of the policy stack.
//!
//! Each [`MergePolicy`] decides how to reconcile one [`FieldChange`] from the
//! three-way diff. Policies are declarative and pure: given a change and a
//! context, they return a [`MergeOutcome`]. They never throw, never write,
//! never observe the clock.
//!
//! # Strategies in this tier
//!
//! | Strategy | Use case | Module |
//! | -------- | -------- | ------ |
//! | [`OwnedBy`]       | One system is authoritative for the field | [`owned_by`] |
//! | [`Additive`]      | Counters / quantities where both sides contribute | [`additive`] |
//! | [`Append`]        | Lists / notes — concatenate both sides | [`append`] |
//! | [`StateMachine`]  | Enums with allowed transitions | [`state_machine`] |
//!
//! [`escape_hatch::LastWriteWins`] lives in a separate module on purpose —
//! timestamp-based resolution is a known anti-pattern at scale and must be
//! opted into explicitly with a written justification.
//!
//! # Resolving a whole changelog
//!
//! [`resolve`] dispatches each [`FieldChange`] to the [`MergePolicy`] declared
//! for its path. Unresolved paths (no policy, or policy returns
//! [`MergeOutcome::Conflict`]) accumulate in [`Resolution::conflicts`] for the
//! escalation queue.

pub mod additive;
pub mod append;
pub mod declaration;
pub mod escape_hatch;
pub mod invariants;
pub mod owned_by;
pub mod state_machine;
pub mod structural;

pub use additive::Additive;
pub use append::Append;
pub use declaration::{MergePolicyRef, TransitionRef, policy_map_from_declarations};
pub use escape_hatch::LastWriteWins;
pub use invariants::{Invariant, InvariantOutcome, InvariantSet};
pub use owned_by::OwnedBy;
pub use state_machine::{StateMachine, StateTransition};
pub use structural::{OnAdded, OnBothChanged, OnRemoved, SetByKey};

// Re-exported to the module root so callers don't have to know which
// file the type lives in.
// (ConflictClass, UnresolvedConflict, Resolution are defined below.)

use crate::domain::diff::{Changelog, FieldChange};
use serde_json::Value;
use std::collections::HashMap;

/// Labels for the two sides of a three-way diff, so per-field policies can
/// tell "A" and "B" apart by name (e.g. "netsuite" vs "our_inventory").
#[derive(Debug, Clone)]
pub struct MergeContext {
    pub system_a: String,
    pub system_b: String,
}

impl MergeContext {
    pub fn new(system_a: impl Into<String>, system_b: impl Into<String>) -> Self {
        Self {
            system_a: system_a.into(),
            system_b: system_b.into(),
        }
    }
}

/// The outcome of applying one policy to one [`FieldChange`].
#[derive(Debug, Clone, PartialEq)]
pub enum MergeOutcome {
    /// Policy produced a merged value. Use it.
    Resolved(Value),
    /// Policy cannot decide this change — escalate with the given reason.
    Conflict { reason: String },
}

/// Pure per-field merge rule. Implementors are stateless and cheap to clone
/// (usually empty structs or small config).
pub trait MergePolicy: Send + Sync {
    /// Stable name for logging, error messages, and the policy registry.
    fn name(&self) -> &'static str;

    /// Apply this policy to a single change.
    fn merge(&self, change: &FieldChange, ctx: &MergeContext) -> MergeOutcome;
}

/// Map from canonical field path to the policy that governs it.
///
/// An optional `default` policy catches paths not explicitly declared — if
/// not set, unregistered paths become [`MergeOutcome::Conflict`] and escalate.
pub struct PolicyMap {
    by_path: HashMap<String, Box<dyn MergePolicy>>,
    default: Option<Box<dyn MergePolicy>>,
}

impl PolicyMap {
    pub fn new() -> Self {
        Self {
            by_path: HashMap::new(),
            default: None,
        }
    }

    pub fn with(mut self, path: impl Into<String>, policy: Box<dyn MergePolicy>) -> Self {
        self.by_path.insert(path.into(), policy);
        self
    }

    pub fn with_default(mut self, policy: Box<dyn MergePolicy>) -> Self {
        self.default = Some(policy);
        self
    }

    fn lookup(&self, path: &str) -> Option<&dyn MergePolicy> {
        self.by_path
            .get(path)
            .map(|b| b.as_ref())
            .or_else(|| self.default.as_deref())
    }
}

impl Default for PolicyMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Cause category for an unresolved conflict. Enables per-class
/// disposition (reject / escalate / preserve-both) at the user layer,
/// per the Dropbox/Synology conflict-visibility pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictClass {
    /// No merge policy declared for the path. Caller misconfiguration —
    /// the orchestrator chose not to guess.
    NoPolicy,
    /// A declared policy ran and explicitly returned [`MergeOutcome::Conflict`]
    /// (e.g. `StateMachine` rejecting an illegal transition, `SetByKey`
    /// seeing a divergent element, `Additive` seeing non-numeric input).
    PolicyConflict,
    /// Tier-2 invariant rejected the merged candidate. Tier-1 produced
    /// a value, but the value violates a rule about valid entity state.
    /// Tagged by the orchestrator, not the resolver.
    InvariantViolation,
}

/// A conflict that survived resolution and must go to the escalation queue.
#[derive(Debug, Clone, PartialEq)]
pub struct UnresolvedConflict {
    pub path: String,
    pub reason: String,
    pub class: ConflictClass,
    pub change: FieldChange,
}

/// Per-path resolution result. `resolved` lists fields the orchestrator
/// should write; `conflicts` lists fields to escalate.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resolution {
    pub resolved: Vec<(String, Value)>,
    pub conflicts: Vec<UnresolvedConflict>,
}

impl Resolution {
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty()
    }
}

/// Apply a [`PolicyMap`] to every change in a [`Changelog`].
///
/// A change with [`ChangeSource::A`] or [`ChangeSource::B`] alone is always
/// resolvable by trivial rule — one side moved, the other did not, so the
/// mover wins *unless* an owner-based policy says otherwise. This function
/// defers the decision to the policy in all cases so overrides like
/// [`OwnedBy`] (which can veto a non-owner's change) work uniformly.
pub fn resolve(
    changelog: &Changelog,
    policies: &PolicyMap,
    ctx: &MergeContext,
) -> Resolution {
    let mut out = Resolution::default();

    for change in &changelog.changes {
        let policy = match policies.lookup(&change.path) {
            Some(p) => p,
            None => {
                out.conflicts.push(UnresolvedConflict {
                    path: change.path.clone(),
                    reason: format!("no policy declared for path '{}'", change.path),
                    class: ConflictClass::NoPolicy,
                    change: change.clone(),
                });
                continue;
            }
        };

        match policy.merge(change, ctx) {
            MergeOutcome::Resolved(v) => out.resolved.push((change.path.clone(), v)),
            MergeOutcome::Conflict { reason } => out.conflicts.push(UnresolvedConflict {
                path: change.path.clone(),
                reason: format!("{}: {}", policy.name(), reason),
                class: ConflictClass::PolicyConflict,
                change: change.clone(),
            }),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diff::three_way_diff;
    use serde_json::json;

    struct AlwaysResolveToA;
    impl MergePolicy for AlwaysResolveToA {
        fn name(&self) -> &'static str {
            "always_a"
        }
        fn merge(&self, change: &FieldChange, _ctx: &MergeContext) -> MergeOutcome {
            match &change.new_from_a {
                Some(v) => MergeOutcome::Resolved(v.clone()),
                None => MergeOutcome::Resolved(change.old_value.clone()),
            }
        }
    }

    struct AlwaysConflict;
    impl MergePolicy for AlwaysConflict {
        fn name(&self) -> &'static str {
            "always_conflict"
        }
        fn merge(&self, _c: &FieldChange, _ctx: &MergeContext) -> MergeOutcome {
            MergeOutcome::Conflict {
                reason: "by design".into(),
            }
        }
    }

    #[test]
    fn unregistered_path_becomes_conflict() {
        let anc = json!({"x": 1});
        let a = json!({"x": 2});
        let b = json!({"x": 1});

        let log = three_way_diff(&anc, &a, &b);
        let policies = PolicyMap::new();
        let ctx = MergeContext::new("sys_a", "sys_b");

        let r = resolve(&log, &policies, &ctx);
        assert_eq!(r.resolved.len(), 0);
        assert_eq!(r.conflicts.len(), 1);
        assert!(r.conflicts[0].reason.contains("no policy"));
    }

    #[test]
    fn default_policy_catches_unregistered() {
        let anc = json!({"x": 1, "y": 2});
        let a = json!({"x": 10, "y": 20});
        let b = json!({"x": 1, "y": 2});

        let log = three_way_diff(&anc, &a, &b);
        let policies = PolicyMap::new().with_default(Box::new(AlwaysResolveToA));
        let ctx = MergeContext::new("sys_a", "sys_b");

        let r = resolve(&log, &policies, &ctx);
        assert_eq!(r.resolved.len(), 2);
        assert!(r.conflicts.is_empty());
    }

    #[test]
    fn explicit_policy_overrides_default() {
        let anc = json!({"x": 1, "y": 2});
        let a = json!({"x": 10, "y": 20});
        let b = json!({"x": 1, "y": 2});

        let log = three_way_diff(&anc, &a, &b);
        let policies = PolicyMap::new()
            .with_default(Box::new(AlwaysResolveToA))
            .with("y", Box::new(AlwaysConflict));
        let ctx = MergeContext::new("sys_a", "sys_b");

        let r = resolve(&log, &policies, &ctx);
        assert_eq!(r.resolved.len(), 1);
        assert_eq!(r.resolved[0].0, "x");
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(r.conflicts[0].path, "y");
    }

    #[test]
    fn conflict_reason_includes_policy_name() {
        let anc = json!({"x": 1});
        let a = json!({"x": 2});
        let b = json!({"x": 1});

        let log = three_way_diff(&anc, &a, &b);
        let policies = PolicyMap::new().with("x", Box::new(AlwaysConflict));
        let ctx = MergeContext::new("sys_a", "sys_b");

        let r = resolve(&log, &policies, &ctx);
        assert!(r.conflicts[0].reason.starts_with("always_conflict:"));
    }
}
