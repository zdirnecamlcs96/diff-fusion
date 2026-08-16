//! `Append` — concatenate both sides' additions.
//!
//! For array fields like `notes` or `tags`, the merged value is the union of
//! A's and B's additions to the ancestor, preserving order (A's additions
//! before B's). Duplicates introduced by independent additions are kept by
//! default; deduplication is a separate concern (callers can post-process).
//!
//! Non-array fields return [`MergeOutcome::Conflict`]. String concatenation is
//! intentionally *not* supported — App.md § 03 warns that git-style merges on
//! free-text business prose produce garbage.

use super::{MergeContext, MergeOutcome, MergePolicy};
use crate::domain::diff::{ChangeSource, FieldChange};
use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct Append;

impl Append {
    pub fn new() -> Self {
        Self
    }
}

impl MergePolicy for Append {
    fn name(&self) -> &'static str {
        "append"
    }

    fn merge(&self, change: &FieldChange, _ctx: &MergeContext) -> MergeOutcome {
        let Some(anc) = change.old_value.as_array() else {
            return non_array();
        };
        let a = match side_array(change.new_from_a.as_ref(), anc) {
            Ok(v) => v,
            Err(o) => return o,
        };
        let b = match side_array(change.new_from_b.as_ref(), anc) {
            Ok(v) => v,
            Err(o) => return o,
        };

        let merged = match change.source {
            ChangeSource::A => a,
            ChangeSource::B => b,
            ChangeSource::Both => {
                // Union of each side's additions relative to anc, preserving
                // per-side order (A's additions before B's).
                let mut out = anc.clone();
                out.extend(a.iter().filter(|v| !anc.contains(v)).cloned());
                out.extend(b.iter().filter(|v| !anc.contains(v)).cloned());
                out
            }
        };

        MergeOutcome::Resolved(Value::Array(merged))
    }
}

fn side_array(side: Option<&Value>, anc: &[Value]) -> Result<Vec<Value>, MergeOutcome> {
    match side {
        Some(v) => v.as_array().cloned().ok_or_else(non_array),
        None => Ok(anc.to_vec()),
    }
}

fn non_array() -> MergeOutcome {
    MergeOutcome::Conflict {
        reason: "append requires array values on all sides".into(),
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
    fn a_only_returns_a() {
        let anc = json!({"notes": ["x"]});
        let a = json!({"notes": ["x", "y"]});
        let b = json!({"notes": ["x"]});
        let log = three_way_diff(&anc, &a, &b);

        let out = Append.merge(&log.changes[0], &ctx());
        assert_eq!(out, MergeOutcome::Resolved(json!(["x", "y"])));
    }

    #[test]
    fn both_additions_concatenate() {
        let anc = json!({"notes": ["x"]});
        let a = json!({"notes": ["x", "y"]});
        let b = json!({"notes": ["x", "z"]});
        let log = three_way_diff(&anc, &a, &b);

        let out = Append.merge(&log.changes[0], &ctx());
        assert_eq!(out, MergeOutcome::Resolved(json!(["x", "y", "z"])));
    }

    #[test]
    fn independent_duplicates_are_kept() {
        // Both sides independently added the same new item. This is rare;
        // policy keeps both rather than silently dedupe-ing.
        let anc = json!({"notes": []});
        let a = json!({"notes": ["y"]});
        let b = json!({"notes": ["y"]});
        let log = three_way_diff(&anc, &a, &b);

        let out = Append.merge(&log.changes[0], &ctx());
        assert_eq!(out, MergeOutcome::Resolved(json!(["y", "y"])));
    }

    #[test]
    fn non_array_is_conflict() {
        let anc = json!({"notes": "text"});
        let a = json!({"notes": "text plus"});
        let b = json!({"notes": "text"});
        let log = three_way_diff(&anc, &a, &b);

        let out = Append.merge(&log.changes[0], &ctx());
        assert!(matches!(out, MergeOutcome::Conflict { .. }));
    }
}
