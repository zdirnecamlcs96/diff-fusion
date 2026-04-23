//! `Additive` — counters where both sides' deltas accumulate.
//!
//! For a field like `qty_received` where System A records 3 incoming units
//! and System B records 2 others during the same cycle, the merged value is
//! `ancestor + delta_a + delta_b = new_a + new_b - ancestor`.
//!
//! Only defined for numeric fields. Non-numeric inputs return
//! [`MergeOutcome::Conflict`] rather than guessing.

use super::{MergeContext, MergeOutcome, MergePolicy};
use crate::domain::diff::{ChangeSource, FieldChange};
use serde_json::{Number, Value};

#[derive(Debug, Clone, Default)]
pub struct Additive;

impl Additive {
    pub fn new() -> Self {
        Self
    }
}

impl MergePolicy for Additive {
    fn name(&self) -> &'static str {
        "additive"
    }

    fn merge(&self, change: &FieldChange, _ctx: &MergeContext) -> MergeOutcome {
        let anc = match as_f64(&change.old_value) {
            Some(n) => n,
            None => return non_numeric("ancestor"),
        };

        let new_a = match change.new_from_a.as_ref() {
            Some(v) => match as_f64(v) {
                Some(n) => n,
                None => return non_numeric("a"),
            },
            None => anc,
        };

        let new_b = match change.new_from_b.as_ref() {
            Some(v) => match as_f64(v) {
                Some(n) => n,
                None => return non_numeric("b"),
            },
            None => anc,
        };

        let merged = match change.source {
            // Only one side moved — no accumulation needed.
            ChangeSource::A => new_a,
            ChangeSource::B => new_b,
            // Both moved — sum the deltas onto the ancestor.
            ChangeSource::Both => anc + (new_a - anc) + (new_b - anc),
        };

        match Number::from_f64(merged) {
            Some(n) => MergeOutcome::Resolved(Value::Number(n)),
            None => MergeOutcome::Conflict {
                reason: format!("additive result {merged} is not finite"),
            },
        }
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn non_numeric(side: &str) -> MergeOutcome {
    MergeOutcome::Conflict {
        reason: format!("additive requires numeric values on all sides ({side} is not numeric)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diff::three_way_diff;
    use proptest::prelude::*;
    use serde_json::json;

    fn ctx() -> MergeContext {
        MergeContext::new("a", "b")
    }

    #[test]
    fn one_side_move_passes_through() {
        let anc = json!({"q": 10});
        let a = json!({"q": 13});
        let b = json!({"q": 10});
        let log = three_way_diff(&anc, &a, &b);

        let out = Additive.merge(&log.changes[0], &ctx());
        assert_eq!(out, MergeOutcome::Resolved(json!(13.0)));
    }

    #[test]
    fn both_sides_deltas_accumulate() {
        // anc=10, a=+3 -> 13, b=+2 -> 12, merged = 10 + 3 + 2 = 15
        let anc = json!({"q": 10});
        let a = json!({"q": 13});
        let b = json!({"q": 12});
        let log = three_way_diff(&anc, &a, &b);

        let out = Additive.merge(&log.changes[0], &ctx());
        assert_eq!(out, MergeOutcome::Resolved(json!(15.0)));
    }

    #[test]
    fn both_decrement_accumulates_negative() {
        // anc=10, a=8 (-2), b=7 (-3) -> 10 - 2 - 3 = 5
        let anc = json!({"q": 10});
        let a = json!({"q": 8});
        let b = json!({"q": 7});
        let log = three_way_diff(&anc, &a, &b);

        let out = Additive.merge(&log.changes[0], &ctx());
        assert_eq!(out, MergeOutcome::Resolved(json!(5.0)));
    }

    #[test]
    fn non_numeric_is_conflict() {
        let anc = json!({"q": "ten"});
        let a = json!({"q": "eleven"});
        let b = json!({"q": "ten"});
        let log = three_way_diff(&anc, &a, &b);

        let out = Additive.merge(&log.changes[0], &ctx());
        assert!(matches!(out, MergeOutcome::Conflict { .. }));
    }

    proptest! {
        /// Commutativity: swapping A and B must not change the merged value.
        #[test]
        fn commutative(
            anc in -1_000_000i64..1_000_000,
            delta_a in -10_000i64..10_000,
            delta_b in -10_000i64..10_000,
        ) {
            let a_val = anc + delta_a;
            let b_val = anc + delta_b;

            let anc_j = json!({"q": anc});
            let a_j = json!({"q": a_val});
            let b_j = json!({"q": b_val});

            let log_ab = three_way_diff(&anc_j, &a_j, &b_j);
            let log_ba = three_way_diff(&anc_j, &b_j, &a_j);

            let ctx = MergeContext::new("a", "b");
            let r_ab = Additive.merge(&log_ab.changes[0], &ctx);
            let r_ba = Additive.merge(&log_ba.changes[0], &ctx);

            prop_assert_eq!(r_ab, r_ba);
        }
    }
}
