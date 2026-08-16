//! Three-way diff against a stored ancestor.
//!
//! Given `(ancestor, a, b)` this produces a [`Changelog`] where every entry
//! records *which side changed* — the provenance signal that makes policy
//! resolution possible. Without it, reconciliation reduces to time-based
//! tie-breaking, which fails at scale (clock skew, batch windows).
//!
//! # Semantics
//!
//! For each leaf path touched by A or B relative to the ancestor:
//!
//! | A changed? | B changed? | [`ChangeSource`] |
//! | ---------- | ---------- | ---------------- |
//! | yes        | no         | [`A`][ChangeSource::A] |
//! | no         | yes        | [`B`][ChangeSource::B] |
//! | yes        | yes        | [`Both`][ChangeSource::Both] |
//!
//! "Both" does **not** mean conflict — it means both sides moved from the
//! ancestor. The resolver decides whether the two values agree or conflict.
//!
//! # Implementation
//!
//! Composes two two-way diffs (ancestor→a, ancestor→b) via
//! [`crate::domain::compare::compare_json`]. No rewrite of the leaf comparison logic.

use crate::domain::compare::compare_json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

/// Which side originated the change at a given path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum ChangeSource {
    A,
    B,
    Both,
}

/// One per-field change record.
///
/// `new_from_a` is `None` when A did not touch this path; same for
/// `new_from_b`. When both are `Some`, resolvers may still decide the values
/// happen to agree.
///
/// Domain serde collapses `Some(Value::Null)` and `None` to the same wire
/// `null` — that distinction only survives at the host boundary, where
/// `drivers/wasm.rs`'s `WireFieldChange` re-encodes it as absent-key
/// (unchanged) vs. present-key (changed, possibly to `null`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct FieldChange {
    pub path: String,
    pub old_value: Value,
    pub new_from_a: Option<Value>,
    pub new_from_b: Option<Value>,
    pub source: ChangeSource,
}

/// Output of a three-way diff.
///
/// Changes are sorted by path for deterministic ordering — useful for
/// snapshot tests and stable log output.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct Changelog {
    pub changes: Vec<FieldChange>,
}

impl Changelog {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Compute a three-way diff.
///
/// Returns an empty [`Changelog`] when neither side has moved from the
/// ancestor — the orchestrator can skip the rest of the cycle in that case.
pub fn three_way_diff(ancestor: &Value, a: &Value, b: &Value) -> Changelog {
    let a_diffs: HashMap<String, (Value, Value)> = compare_json(ancestor, a).into_iter().collect();
    let b_diffs: HashMap<String, (Value, Value)> = compare_json(ancestor, b).into_iter().collect();

    // Sorted union of touched paths — deterministic iteration.
    let all_paths: BTreeSet<&String> = a_diffs.keys().chain(b_diffs.keys()).collect();

    let mut changes = Vec::with_capacity(all_paths.len());
    for path in all_paths {
        let a_entry = a_diffs.get(path);
        let b_entry = b_diffs.get(path);

        // Either side's record carries the ancestor-side value — they are
        // identical because both were diffed against the same ancestor.
        let old_value = a_entry
            .map(|(old, _)| old.clone())
            .or_else(|| b_entry.map(|(old, _)| old.clone()))
            .expect("path appears in at least one diff");

        let new_from_a = a_entry.map(|(_, new)| new.clone());
        let new_from_b = b_entry.map(|(_, new)| new.clone());

        let source = match (new_from_a.is_some(), new_from_b.is_some()) {
            (true, false) => ChangeSource::A,
            (false, true) => ChangeSource::B,
            (true, true) => ChangeSource::Both,
            (false, false) => unreachable!("path must appear in at least one side"),
        };

        changes.push(FieldChange {
            path: path.clone(),
            old_value,
            new_from_a,
            new_from_b,
            source,
        });
    }

    Changelog { changes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    #[test]
    fn identical_inputs_produce_empty_changelog() {
        let v = json!({"price": 10, "qty": 5});
        let log = three_way_diff(&v, &v, &v);
        assert!(log.is_empty());
    }

    #[test]
    fn only_a_changed_source_is_a() {
        let anc = json!({"price": 10});
        let a = json!({"price": 12});
        let b = json!({"price": 10});

        let log = three_way_diff(&anc, &a, &b);

        assert_eq!(log.changes.len(), 1);
        let c = &log.changes[0];
        assert_eq!(c.path, "price");
        assert_eq!(c.source, ChangeSource::A);
        assert_eq!(c.new_from_a, Some(json!(12)));
        assert_eq!(c.new_from_b, None);
        assert_eq!(c.old_value, json!(10));
    }

    #[test]
    fn only_b_changed_source_is_b() {
        let anc = json!({"price": 10});
        let a = json!({"price": 10});
        let b = json!({"price": 15});

        let log = three_way_diff(&anc, &a, &b);

        assert_eq!(log.changes.len(), 1);
        let c = &log.changes[0];
        assert_eq!(c.source, ChangeSource::B);
        assert_eq!(c.new_from_a, None);
        assert_eq!(c.new_from_b, Some(json!(15)));
    }

    #[test]
    fn both_changed_source_is_both_even_when_values_agree() {
        // Both sides bumped price to the same value. This is not a conflict,
        // but it IS 'source=Both' — the resolver gets to decide.
        let anc = json!({"price": 10});
        let a = json!({"price": 12});
        let b = json!({"price": 12});

        let log = three_way_diff(&anc, &a, &b);

        assert_eq!(log.changes.len(), 1);
        assert_eq!(log.changes[0].source, ChangeSource::Both);
    }

    #[test]
    fn both_changed_to_different_values() {
        let anc = json!({"price": 10});
        let a = json!({"price": 12});
        let b = json!({"price": 15});

        let log = three_way_diff(&anc, &a, &b);

        assert_eq!(log.changes.len(), 1);
        let c = &log.changes[0];
        assert_eq!(c.source, ChangeSource::Both);
        assert_eq!(c.new_from_a, Some(json!(12)));
        assert_eq!(c.new_from_b, Some(json!(15)));
    }

    #[test]
    fn independent_fields_from_each_side() {
        let anc = json!({"price": 10, "qty": 5, "name": "widget"});
        let a = json!({"price": 12, "qty": 5, "name": "widget"}); // A bumped price
        let b = json!({"price": 10, "qty": 7, "name": "widget"}); // B bumped qty

        let log = three_way_diff(&anc, &a, &b);

        assert_eq!(log.changes.len(), 2);
        let by_path: HashMap<_, _> = log.changes.iter().map(|c| (c.path.as_str(), c)).collect();
        assert_eq!(by_path["price"].source, ChangeSource::A);
        assert_eq!(by_path["qty"].source, ChangeSource::B);
    }

    #[test]
    fn changes_sorted_by_path() {
        let anc = json!({"a": 1, "b": 2, "c": 3});
        let a = json!({"a": 10, "b": 20, "c": 30});
        let b = json!({"a": 1, "b": 2, "c": 3});

        let log = three_way_diff(&anc, &a, &b);

        let paths: Vec<_> = log.changes.iter().map(|c| c.path.clone()).collect();
        assert_eq!(paths, vec!["a", "b", "c"]);
    }

    #[test]
    fn nested_paths_are_dotted() {
        let anc = json!({"pricing": {"amount": 10}});
        let a = json!({"pricing": {"amount": 12}});
        let b = json!({"pricing": {"amount": 10}});

        let log = three_way_diff(&anc, &a, &b);

        assert_eq!(log.changes[0].path, "pricing.amount");
    }

    // --- Property tests ---------------------------------------------------

    proptest! {
        /// If no side moved from the ancestor, the changelog is empty.
        #[test]
        fn no_change_is_no_entries(seed in prop::num::i64::ANY) {
            let v = json!({"n": seed});
            let log = three_way_diff(&v, &v, &v);
            prop_assert!(log.is_empty());
        }

        /// When only A moves, no entry is ever 'source=Both'.
        #[test]
        fn a_only_never_produces_both(a_val in prop::num::i64::ANY, anc_val in prop::num::i64::ANY) {
            prop_assume!(a_val != anc_val);
            let anc = json!({"n": anc_val});
            let a = json!({"n": a_val});
            let b = anc.clone();

            let log = three_way_diff(&anc, &a, &b);
            for change in &log.changes {
                prop_assert_ne!(change.source, ChangeSource::Both);
                prop_assert_eq!(change.source, ChangeSource::A);
            }
        }

        /// When only B moves, no entry is ever 'source=A' or 'source=Both'.
        #[test]
        fn b_only_never_produces_a_or_both(b_val in prop::num::i64::ANY, anc_val in prop::num::i64::ANY) {
            prop_assume!(b_val != anc_val);
            let anc = json!({"n": anc_val});
            let a = anc.clone();
            let b = json!({"n": b_val});

            let log = three_way_diff(&anc, &a, &b);
            for change in &log.changes {
                prop_assert_eq!(change.source, ChangeSource::B);
            }
        }

        /// Symmetry: swapping A and B inverts every change's source
        /// (A ↔ B, Both unchanged) without adding or dropping entries.
        #[test]
        fn swapping_a_b_inverts_source(
            anc_val in prop::num::i64::ANY,
            a_val in prop::num::i64::ANY,
            b_val in prop::num::i64::ANY,
        ) {
            let anc = json!({"n": anc_val});
            let a = json!({"n": a_val});
            let b = json!({"n": b_val});

            let forward = three_way_diff(&anc, &a, &b);
            let swapped = three_way_diff(&anc, &b, &a);

            prop_assert_eq!(forward.changes.len(), swapped.changes.len());
            for (f, s) in forward.changes.iter().zip(swapped.changes.iter()) {
                prop_assert_eq!(&f.path, &s.path);
                let expected = match f.source {
                    ChangeSource::A => ChangeSource::B,
                    ChangeSource::B => ChangeSource::A,
                    ChangeSource::Both => ChangeSource::Both,
                };
                prop_assert_eq!(s.source, expected);
            }
        }
    }
}
