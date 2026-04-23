//! Tier 3 — structural merges for collections.
//!
//! Flat per-field policies cannot express "this line item exists in A but
//! not B — was it added in A or deleted in B?". That's a structural
//! question about set identity, not a value-level one. [`SetByKey`] answers
//! it by declaring which field on each element is the identity.
//!
//! # Caveat
//!
//! This is not a deep recursive merger — elements that appear on both sides
//! with differences are reported per the `on_both_changed` setting. For
//! element-level recursion, compose with Tier 1 policies on the nested
//! paths.

use super::{MergeContext, MergeOutcome, MergePolicy};
use crate::domain::diff::{ChangeSource, FieldChange};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnAdded {
    Include,
    Exclude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnRemoved {
    Remove,
    /// Remove only if the other side did not modify the same element.
    /// Modified-and-removed surfaces as a conflict.
    EscalateIfChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnBothChanged {
    Escalate,
    PreferA,
    PreferB,
}

/// Merge arrays of objects by an identity field.
#[derive(Debug, Clone)]
pub struct SetByKey {
    pub identity: String,
    pub on_added_in_a: OnAdded,
    pub on_added_in_b: OnAdded,
    pub on_removed_in_a: OnRemoved,
    pub on_removed_in_b: OnRemoved,
    pub on_both_changed: OnBothChanged,
}

impl SetByKey {
    /// Sensible defaults: include additions on either side, escalate when a
    /// side modified an element the other side removed, escalate when both
    /// modified the same element.
    pub fn new(identity: impl Into<String>) -> Self {
        Self {
            identity: identity.into(),
            on_added_in_a: OnAdded::Include,
            on_added_in_b: OnAdded::Include,
            on_removed_in_a: OnRemoved::EscalateIfChanged,
            on_removed_in_b: OnRemoved::EscalateIfChanged,
            on_both_changed: OnBothChanged::Escalate,
        }
    }
}

impl MergePolicy for SetByKey {
    fn name(&self) -> &'static str {
        "set_by_key"
    }

    fn merge(&self, change: &FieldChange, _ctx: &MergeContext) -> MergeOutcome {
        let anc = match change.old_value.as_array() {
            Some(arr) => arr,
            None => return non_array("ancestor"),
        };
        let a = match change.new_from_a.as_ref() {
            Some(v) => match v.as_array() {
                Some(arr) => arr,
                None => return non_array("a"),
            },
            None => anc,
        };
        let b = match change.new_from_b.as_ref() {
            Some(v) => match v.as_array() {
                Some(arr) => arr,
                None => return non_array("b"),
            },
            None => anc,
        };

        // Build identity → element indices on each side. Missing identity
        // is a conflict — we can't reconcile anonymous objects.
        let idx_anc = match index_by(anc, &self.identity) {
            Ok(m) => m,
            Err(e) => return e,
        };
        let idx_a = match index_by(a, &self.identity) {
            Ok(m) => m,
            Err(e) => return e,
        };
        let idx_b = match index_by(b, &self.identity) {
            Ok(m) => m,
            Err(e) => return e,
        };

        let all_keys: std::collections::BTreeSet<&String> = idx_anc
            .keys()
            .chain(idx_a.keys())
            .chain(idx_b.keys())
            .collect();

        let mut out = Vec::new();
        for key in all_keys {
            let in_anc = idx_anc.get(key).map(|&i| &anc[i]);
            let in_a = idx_a.get(key).map(|&i| &a[i]);
            let in_b = idx_b.get(key).map(|&i| &b[i]);

            match (in_anc, in_a, in_b) {
                // Present everywhere, unchanged on both sides.
                (Some(_), Some(ea), Some(eb)) if ea == eb => out.push(ea.clone()),
                // Changed only on A.
                (Some(ea), Some(elem_a), Some(eb)) if ea == eb => {
                    // ancestor matches B, so B didn't change; A did.
                    let _ = ea; // already matched
                    out.push(elem_a.clone());
                }
                // Changed only on B.
                (Some(ea), Some(elem_a), Some(eb)) if ea == elem_a && ea != eb => {
                    out.push(eb.clone());
                }
                // Changed on both — apply on_both_changed.
                (Some(_), Some(elem_a), Some(elem_b)) => match self.on_both_changed {
                    OnBothChanged::PreferA => out.push(elem_a.clone()),
                    OnBothChanged::PreferB => out.push(elem_b.clone()),
                    OnBothChanged::Escalate => {
                        return MergeOutcome::Conflict {
                            reason: format!("element '{key}' changed on both sides"),
                        };
                    }
                },
                // Added in A only.
                (None, Some(elem_a), None) => {
                    if self.on_added_in_a == OnAdded::Include {
                        out.push(elem_a.clone());
                    }
                }
                // Added in B only.
                (None, None, Some(elem_b)) => {
                    if self.on_added_in_b == OnAdded::Include {
                        out.push(elem_b.clone());
                    }
                }
                // Added on both sides independently — treat like both-changed.
                (None, Some(elem_a), Some(elem_b)) => {
                    if elem_a == elem_b {
                        out.push(elem_a.clone());
                    } else {
                        match self.on_both_changed {
                            OnBothChanged::PreferA => out.push(elem_a.clone()),
                            OnBothChanged::PreferB => out.push(elem_b.clone()),
                            OnBothChanged::Escalate => {
                                return MergeOutcome::Conflict {
                                    reason: format!("element '{key}' added divergently on both sides"),
                                };
                            }
                        }
                    }
                }
                // Removed in A.
                (Some(ea), None, Some(eb)) => {
                    if ea == eb {
                        // B didn't change it; honor the removal.
                    } else if self.on_removed_in_a == OnRemoved::EscalateIfChanged {
                        return MergeOutcome::Conflict {
                            reason: format!("element '{key}' removed in A but changed in B"),
                        };
                    }
                    // else OnRemoved::Remove: drop it.
                }
                // Removed in B.
                (Some(ea), Some(elem_a), None) => {
                    if ea == elem_a {
                        // A didn't change it; honor the removal.
                    } else if self.on_removed_in_b == OnRemoved::EscalateIfChanged {
                        return MergeOutcome::Conflict {
                            reason: format!("element '{key}' removed in B but changed in A"),
                        };
                    }
                    // else OnRemoved::Remove: drop it.
                }
                // Removed in both.
                (Some(_), None, None) => {
                    // honor the removal everywhere.
                }
                // Present in ancestor only, but also not in A or B — already handled above.
                // unreachable variants left to the compiler's exhaustiveness check.
                (None, None, None) => unreachable!("key not present anywhere"),
            }
        }

        // Preserve ancestor's order where possible.
        let ordered = order_like_ancestor(out, anc, &self.identity);
        match change.source {
            ChangeSource::A | ChangeSource::B | ChangeSource::Both => {
                MergeOutcome::Resolved(Value::Array(ordered))
            }
        }
    }
}

fn index_by(
    arr: &[Value],
    identity: &str,
) -> Result<std::collections::HashMap<String, usize>, MergeOutcome> {
    let mut out = std::collections::HashMap::new();
    for (i, elem) in arr.iter().enumerate() {
        let key = match elem.get(identity).and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                return Err(MergeOutcome::Conflict {
                    reason: format!("element missing string identity field '{identity}'"),
                });
            }
        };
        out.insert(key, i);
    }
    Ok(out)
}

fn order_like_ancestor(mut merged: Vec<Value>, ancestor: &[Value], identity: &str) -> Vec<Value> {
    let priority = |elem: &Value| -> Option<usize> {
        let k = elem.get(identity)?.as_str()?;
        ancestor
            .iter()
            .position(|a| a.get(identity).and_then(|v| v.as_str()) == Some(k))
    };
    merged.sort_by_key(|e| priority(e).unwrap_or(usize::MAX));
    merged
}

fn non_array(side: &str) -> MergeOutcome {
    MergeOutcome::Conflict {
        reason: format!("set_by_key requires array values ({side} is not an array)"),
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
    fn additions_on_both_sides_merge() {
        let anc = json!({"items": [{"sku": "x", "q": 1}]});
        let a = json!({"items": [{"sku": "x", "q": 1}, {"sku": "y", "q": 2}]});
        let b = json!({"items": [{"sku": "x", "q": 1}, {"sku": "z", "q": 3}]});
        let log = three_way_diff(&anc, &a, &b);

        let policy = SetByKey::new("sku");
        match policy.merge(&log.changes[0], &ctx()) {
            MergeOutcome::Resolved(Value::Array(arr)) => {
                let skus: Vec<_> = arr
                    .iter()
                    .map(|v| v["sku"].as_str().unwrap().to_string())
                    .collect();
                assert!(skus.contains(&"x".to_string()));
                assert!(skus.contains(&"y".to_string()));
                assert!(skus.contains(&"z".to_string()));
                assert_eq!(arr.len(), 3);
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn removal_in_a_unchanged_in_b_drops_element() {
        let anc = json!({"items": [{"sku": "x"}, {"sku": "y"}]});
        let a = json!({"items": [{"sku": "x"}]});
        let b = json!({"items": [{"sku": "x"}, {"sku": "y"}]});
        let log = three_way_diff(&anc, &a, &b);

        let policy = SetByKey::new("sku");
        match policy.merge(&log.changes[0], &ctx()) {
            MergeOutcome::Resolved(Value::Array(arr)) => {
                assert_eq!(arr.len(), 1);
                assert_eq!(arr[0]["sku"], json!("x"));
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn removal_in_a_but_changed_in_b_escalates() {
        let anc = json!({"items": [{"sku": "x", "q": 1}]});
        let a = json!({"items": []});
        let b = json!({"items": [{"sku": "x", "q": 99}]});
        let log = three_way_diff(&anc, &a, &b);

        let policy = SetByKey::new("sku");
        match policy.merge(&log.changes[0], &ctx()) {
            MergeOutcome::Conflict { reason } => {
                assert!(reason.contains("'x'"));
                assert!(reason.contains("removed in A"));
            }
            other => panic!("expected conflict, got {other:?}"),
        }
    }

    #[test]
    fn both_changed_escalates_by_default() {
        let anc = json!({"items": [{"sku": "x", "q": 1}]});
        let a = json!({"items": [{"sku": "x", "q": 10}]});
        let b = json!({"items": [{"sku": "x", "q": 20}]});
        let log = three_way_diff(&anc, &a, &b);

        let policy = SetByKey::new("sku");
        match policy.merge(&log.changes[0], &ctx()) {
            MergeOutcome::Conflict { reason } => assert!(reason.contains("changed on both")),
            other => panic!("expected conflict, got {other:?}"),
        }
    }

    #[test]
    fn missing_identity_is_a_conflict() {
        let anc = json!({"items": [{"q": 1}]});
        let a = json!({"items": [{"q": 2}]});
        let b = json!({"items": [{"q": 1}]});
        let log = three_way_diff(&anc, &a, &b);

        let policy = SetByKey::new("sku");
        assert!(matches!(
            policy.merge(&log.changes[0], &ctx()),
            MergeOutcome::Conflict { .. }
        ));
    }
}
