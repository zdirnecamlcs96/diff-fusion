//! `OwnedBy` — one system is authoritative for the field.
//!
//! The owner's changes propagate; the non-owner's changes to this field are
//! ignored (the owner's last known value wins). This is the single most
//! effective strategy — App.md reports it eliminates ~80% of conflicts
//! before they arise.

use super::{MergeContext, MergeOutcome, MergePolicy};
use crate::domain::diff::{ChangeSource, FieldChange};

/// Owner is declared by system label — must match `ctx.system_a` or
/// `ctx.system_b` at merge time.
#[derive(Debug, Clone)]
pub struct OwnedBy {
    pub system: String,
}

impl OwnedBy {
    pub fn new(system: impl Into<String>) -> Self {
        Self {
            system: system.into(),
        }
    }
}

impl MergePolicy for OwnedBy {
    fn name(&self) -> &'static str {
        "owned_by"
    }

    fn merge(&self, change: &FieldChange, ctx: &MergeContext) -> MergeOutcome {
        let owner_is_a = self.system == ctx.system_a;
        let owner_is_b = self.system == ctx.system_b;

        if !owner_is_a && !owner_is_b {
            return MergeOutcome::Conflict {
                reason: format!(
                    "owner '{}' does not match either side ('{}', '{}')",
                    self.system, ctx.system_a, ctx.system_b
                ),
            };
        }

        // The owner's latest known value wins. If the owner did not move,
        // the ancestor value wins — reverting the non-owner's attempt.
        let owner_new = if owner_is_a {
            change.new_from_a.as_ref()
        } else {
            change.new_from_b.as_ref()
        };

        match owner_new {
            Some(v) => MergeOutcome::Resolved(v.clone()),
            None => {
                // Owner did not move; non-owner did. Revert to ancestor.
                debug_assert!(matches!(
                    change.source,
                    ChangeSource::A | ChangeSource::B
                ));
                MergeOutcome::Resolved(change.old_value.clone())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diff::three_way_diff;
    use serde_json::json;

    fn ctx() -> MergeContext {
        MergeContext::new("sys_a", "sys_b")
    }

    #[test]
    fn owner_a_wins_when_a_moves() {
        let anc = json!({"x": 1});
        let a = json!({"x": 5});
        let b = json!({"x": 1});
        let log = three_way_diff(&anc, &a, &b);
        let policy = OwnedBy::new("sys_a");

        let out = policy.merge(&log.changes[0], &ctx());
        assert_eq!(out, MergeOutcome::Resolved(json!(5)));
    }

    #[test]
    fn non_owner_change_reverts_to_ancestor() {
        // B attempts to change, but A owns the field. Revert to ancestor.
        let anc = json!({"x": 1});
        let a = json!({"x": 1});
        let b = json!({"x": 99});
        let log = three_way_diff(&anc, &a, &b);
        let policy = OwnedBy::new("sys_a");

        let out = policy.merge(&log.changes[0], &ctx());
        assert_eq!(out, MergeOutcome::Resolved(json!(1)));
    }

    #[test]
    fn owner_wins_when_both_move() {
        // Both sides changed; owner's value is authoritative.
        let anc = json!({"x": 1});
        let a = json!({"x": 5});
        let b = json!({"x": 99});
        let log = three_way_diff(&anc, &a, &b);
        let policy = OwnedBy::new("sys_b");

        let out = policy.merge(&log.changes[0], &ctx());
        assert_eq!(out, MergeOutcome::Resolved(json!(99)));
    }

    #[test]
    fn unknown_owner_is_a_conflict() {
        let anc = json!({"x": 1});
        let a = json!({"x": 2});
        let b = json!({"x": 1});
        let log = three_way_diff(&anc, &a, &b);
        let policy = OwnedBy::new("unknown_system");

        let out = policy.merge(&log.changes[0], &ctx());
        match out {
            MergeOutcome::Conflict { reason } => assert!(reason.contains("does not match")),
            _ => panic!("expected conflict"),
        }
    }
}
