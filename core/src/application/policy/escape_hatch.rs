//! Escape hatches — policies that exist but should almost never be your
//! first choice.
//!
//! [`LastWriteWins`] is here instead of alongside `OwnedBy`/`Additive`/etc.
//! on purpose. Timestamp-based conflict resolution fails at scale: clock
//! skew between systems, batch-ingest windows, and queue backpressure all
//! make "most recent write" an unreliable signal. Using it implicitly
//! produces the well-known shopping-cart-bug class of regression that
//! motivated CRDT research in the first place (App.md § 06).
//!
//! A `LastWriteWins` is not constructable without a written justification.
//! The `reason` field is displayed in logs and conflict reports so reviewers
//! can see why the escape hatch was taken for a given field.

use super::{MergeContext, MergeOutcome, MergePolicy};
use crate::domain::diff::{ChangeSource, FieldChange};

/// Pick the side with the most recent write.
///
/// Requires per-field timestamp metadata — not always available from external
/// systems, so the caller supplies it via `timestamp_a`/`timestamp_b`. These
/// are millisecond epochs; the larger value wins. Ties escalate.
///
/// The `reason` argument is mandatory and surfaces in logs/conflict reports.
#[derive(Debug, Clone)]
pub struct LastWriteWins {
    pub reason: String,
    pub timestamp_a: u64,
    pub timestamp_b: u64,
}

impl LastWriteWins {
    /// Construct with an explicit justification. There is no default
    /// constructor on purpose — every use site must spell out why.
    pub fn with_reason(
        reason: impl Into<String>,
        timestamp_a_ms: u64,
        timestamp_b_ms: u64,
    ) -> Self {
        Self {
            reason: reason.into(),
            timestamp_a: timestamp_a_ms,
            timestamp_b: timestamp_b_ms,
        }
    }
}

impl MergePolicy for LastWriteWins {
    fn name(&self) -> &'static str {
        "last_write_wins"
    }

    fn merge(&self, change: &FieldChange, _ctx: &MergeContext) -> MergeOutcome {
        use std::cmp::Ordering;
        let pick_a = || {
            MergeOutcome::Resolved(change.new_from_a.clone().expect("expected new_from_a"))
        };
        let pick_b = || {
            MergeOutcome::Resolved(change.new_from_b.clone().expect("expected new_from_b"))
        };
        match change.source {
            ChangeSource::A => pick_a(),
            ChangeSource::B => pick_b(),
            ChangeSource::Both => match self.timestamp_a.cmp(&self.timestamp_b) {
                Ordering::Greater => pick_a(),
                Ordering::Less => pick_b(),
                Ordering::Equal => MergeOutcome::Conflict {
                    reason: "last_write_wins: timestamps tie".into(),
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diff::three_way_diff;
    use serde_json::json;

    fn ctx() -> MergeContext {
        MergeContext::new("a", "b")
    }

    #[test]
    fn reason_is_visible() {
        let lww = LastWriteWins::with_reason("legacy support", 10, 20);
        assert_eq!(lww.reason, "legacy support");
    }

    #[test]
    fn both_moved_newer_timestamp_wins() {
        let anc = json!({"x": 1});
        let a = json!({"x": 2});
        let b = json!({"x": 3});
        let log = three_way_diff(&anc, &a, &b);

        let lww = LastWriteWins::with_reason("no natural owner", 100, 200);
        let out = lww.merge(&log.changes[0], &ctx());
        assert_eq!(out, MergeOutcome::Resolved(json!(3)));
    }

    #[test]
    fn tied_timestamps_escalate() {
        let anc = json!({"x": 1});
        let a = json!({"x": 2});
        let b = json!({"x": 3});
        let log = three_way_diff(&anc, &a, &b);

        let lww = LastWriteWins::with_reason("reason", 100, 100);
        let out = lww.merge(&log.changes[0], &ctx());
        assert!(matches!(out, MergeOutcome::Conflict { .. }));
    }

    #[test]
    fn single_side_move_is_trivial() {
        let anc = json!({"x": 1});
        let a = json!({"x": 7});
        let b = json!({"x": 1});
        let log = three_way_diff(&anc, &a, &b);

        let lww = LastWriteWins::with_reason("r", 0, 0);
        assert_eq!(
            lww.merge(&log.changes[0], &ctx()),
            MergeOutcome::Resolved(json!(7))
        );
    }
}
