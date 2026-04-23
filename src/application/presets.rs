//! Named policy presets — ergonomic builders over [`PolicyMap`].
//!
//! The presets here are syntactic sugar: each one composes existing Tier 1
//! policies into a common shape (one-way propagation, "prefer this side",
//! "escalate everything"). They do not add new mechanics.
//!
//! The design follows Synology Drive's two-option picker and Dropbox's
//! taxonomy-of-conflict-classes ideas — expose a small set of named
//! philosophies, and drop users into the full [`crate::policy`] API only
//! when the preset doesn't fit.
//!
//! # One-way sync
//!
//! One-way mode is not a separate orchestrator path — it's the
//! [`crate::application::policy::OwnedBy`] policy applied as the map default. Every
//! field is owned by the source; target-side edits revert to the ancestor
//! value on the next cycle. This is the same behavior Synology's
//! "download only" mode exposes, expressed as a policy.
//!
//! ```rust,ignore
//! // Source = NetSuite; target = internal inventory.
//! let policies = presets::one_way_from("netsuite");
//! // Equivalent to:
//! //   PolicyMap::new().with_default(Box::new(OwnedBy::new("netsuite")));
//! ```
//!
//! # Escalate-everything
//!
//! Useful while bringing up a new adapter in shadow mode — every
//! divergence becomes a queue item for review rather than a silent auto-
//! resolve.

use crate::application::policy::{LastWriteWins, OwnedBy, PolicyMap};

/// One-way propagation: the named system owns every unspecified field.
/// Non-owner edits revert on the next cycle.
///
/// Add per-path exceptions with [`PolicyMap::with`] after this call.
pub fn one_way_from(source_system: impl Into<String>) -> PolicyMap {
    PolicyMap::new().with_default(Box::new(OwnedBy::new(source_system)))
}

/// Alias for [`one_way_from`] — "prefer this system when policies conflict."
pub fn prefer_system(system: impl Into<String>) -> PolicyMap {
    one_way_from(system)
}

/// Empty policy map — any per-field divergence escalates. Useful for
/// shadow-mode bringup where you want visibility before committing to
/// automatic resolution.
pub fn escalate_everything() -> PolicyMap {
    PolicyMap::new()
}

/// Timestamp-based default. Requires per-cycle timestamps supplied to the
/// [`LastWriteWins`] escape hatch — callers must provide the millisecond
/// epochs for each side. See that type for why this is an escape hatch
/// rather than a first-class preset.
pub fn latest_wins(
    reason: impl Into<String>,
    timestamp_a_ms: u64,
    timestamp_b_ms: u64,
) -> PolicyMap {
    PolicyMap::new().with_default(Box::new(LastWriteWins::with_reason(
        reason,
        timestamp_a_ms,
        timestamp_b_ms,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diff::three_way_diff;
    use crate::application::policy::{MergeContext, resolve};
    use serde_json::json;

    #[test]
    fn one_way_from_a_reverts_b_side_edits() {
        let policies = one_way_from("sys_a");
        let ctx = MergeContext::new("sys_a", "sys_b");

        // B tried to change, A didn't. Target must revert to ancestor.
        let anc = json!({"price": 10});
        let a = json!({"price": 10});
        let b = json!({"price": 99});
        let log = three_way_diff(&anc, &a, &b);

        let r = resolve(&log, &policies, &ctx);
        assert!(r.conflicts.is_empty());
        assert_eq!(r.resolved.len(), 1);
        assert_eq!(r.resolved[0], ("price".into(), json!(10)));
    }

    #[test]
    fn one_way_from_a_propagates_a_side_edits() {
        let policies = one_way_from("sys_a");
        let ctx = MergeContext::new("sys_a", "sys_b");

        let anc = json!({"price": 10});
        let a = json!({"price": 20});
        let b = json!({"price": 10});
        let log = three_way_diff(&anc, &a, &b);

        let r = resolve(&log, &policies, &ctx);
        assert!(r.conflicts.is_empty());
        assert_eq!(r.resolved[0], ("price".into(), json!(20)));
    }

    #[test]
    fn escalate_everything_surfaces_any_change() {
        let policies = escalate_everything();
        let ctx = MergeContext::new("sys_a", "sys_b");

        let anc = json!({"price": 10});
        let a = json!({"price": 20});
        let b = json!({"price": 10});
        let log = three_way_diff(&anc, &a, &b);

        let r = resolve(&log, &policies, &ctx);
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(r.resolved.len(), 0);
    }

    #[test]
    fn per_path_override_still_works_on_top_of_preset() {
        // Start with one-way-from-A, then override one field back to
        // bidirectional (e.g. via another policy).
        use crate::application::policy::Additive;

        let policies = one_way_from("sys_a").with("qty", Box::new(Additive));
        let ctx = MergeContext::new("sys_a", "sys_b");

        let anc = json!({"price": 10, "qty": 5});
        let a = json!({"price": 10, "qty": 7}); // A bumped qty by 2
        let b = json!({"price": 99, "qty": 8}); // B tried price; bumped qty by 3
        let log = three_way_diff(&anc, &a, &b);

        let r = resolve(&log, &policies, &ctx);
        assert!(r.conflicts.is_empty());

        let map: std::collections::HashMap<_, _> = r.resolved.into_iter().collect();
        // price reverts per OwnedBy(A); qty accumulates per Additive.
        assert_eq!(map["price"], json!(10));
        assert_eq!(map["qty"], json!(10.0));
    }
}
