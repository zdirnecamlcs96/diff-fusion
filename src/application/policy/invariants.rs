//! Tier 2 — post-merge invariants.
//!
//! Rules about *the result*, not about merging. A candidate merged value
//! must satisfy every registered invariant; if it doesn't, the outcome is
//! either a rewrite (`transform`) or a rejection (`reject` / `escalate`).
//!
//! Example (from App.md § 04):
//!
//! ```text
//! "closed POs don't accept more receipts" → invariant on PO, rejects merge
//! ```
//!
//! Invariants run *after* Tier 1 produces a candidate. They never get to
//! choose between A and B — that's Tier 1's job. They only check that the
//! result is a valid entity.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum InvariantOutcome {
    /// The candidate passes; keep it as-is.
    Pass,
    /// The candidate passes after this replacement is applied.
    Transform(Value),
    /// The candidate violates the invariant. The orchestrator reverts to
    /// the previous value and escalates with `reason`.
    Reject { reason: String },
}

/// A predicate that runs on a merged candidate, possibly transforming it.
/// Implementors are pure functions of (previous canonical, candidate).
pub trait Invariant: Send + Sync {
    fn name(&self) -> &'static str;
    fn check(&self, previous: &Value, candidate: &Value) -> InvariantOutcome;
}

/// A bundle of invariants to apply in order. Stops at the first `Reject`.
#[derive(Default)]
pub struct InvariantSet {
    invariants: Vec<Box<dyn Invariant>>,
}

impl InvariantSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, invariant: Box<dyn Invariant>) -> Self {
        self.invariants.push(invariant);
        self
    }

    /// Apply every invariant. If any rejects, returns the rejection
    /// immediately. Otherwise returns the (possibly transformed) candidate.
    pub fn apply(&self, previous: &Value, candidate: &Value) -> InvariantOutcome {
        let mut current = candidate.clone();
        for inv in &self.invariants {
            match inv.check(previous, &current) {
                InvariantOutcome::Pass => {}
                InvariantOutcome::Transform(v) => current = v,
                InvariantOutcome::Reject { reason } => {
                    return InvariantOutcome::Reject {
                        reason: format!("{}: {reason}", inv.name()),
                    };
                }
            }
        }
        if &current == candidate {
            InvariantOutcome::Pass
        } else {
            InvariantOutcome::Transform(current)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct NoReceiveAgainstClosed;
    impl Invariant for NoReceiveAgainstClosed {
        fn name(&self) -> &'static str {
            "no_receive_against_closed"
        }
        fn check(&self, previous: &Value, candidate: &Value) -> InvariantOutcome {
            let status = candidate
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if status != "closed" {
                return InvariantOutcome::Pass;
            }
            let prev_qty = previous.get("qty_recv").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let cand_qty = candidate.get("qty_recv").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if cand_qty > prev_qty {
                InvariantOutcome::Reject {
                    reason: "cannot increase qty_recv on a closed entity".into(),
                }
            } else {
                InvariantOutcome::Pass
            }
        }
    }

    #[test]
    fn pass_when_invariant_holds() {
        let set = InvariantSet::new().with(Box::new(NoReceiveAgainstClosed));
        let prev = json!({"status": "open", "qty_recv": 5.0});
        let cand = json!({"status": "open", "qty_recv": 7.0});
        assert_eq!(set.apply(&prev, &cand), InvariantOutcome::Pass);
    }

    #[test]
    fn reject_when_closed_and_qty_increases() {
        let set = InvariantSet::new().with(Box::new(NoReceiveAgainstClosed));
        let prev = json!({"status": "closed", "qty_recv": 5.0});
        let cand = json!({"status": "closed", "qty_recv": 7.0});
        match set.apply(&prev, &cand) {
            InvariantOutcome::Reject { reason } => {
                assert!(reason.contains("no_receive_against_closed"));
                assert!(reason.contains("cannot increase"));
            }
            other => panic!("expected Reject, got {other:?}"),
        }
    }

    #[test]
    fn transform_is_visible_to_caller() {
        struct CapQty;
        impl Invariant for CapQty {
            fn name(&self) -> &'static str {
                "cap_qty"
            }
            fn check(&self, _prev: &Value, candidate: &Value) -> InvariantOutcome {
                let q = candidate.get("qty").and_then(|v| v.as_f64()).unwrap_or(0.0);
                if q > 100.0 {
                    let mut fixed = candidate.clone();
                    fixed["qty"] = json!(100.0);
                    InvariantOutcome::Transform(fixed)
                } else {
                    InvariantOutcome::Pass
                }
            }
        }

        let set = InvariantSet::new().with(Box::new(CapQty));
        let prev = json!({"qty": 50.0});
        let cand = json!({"qty": 150.0});
        match set.apply(&prev, &cand) {
            InvariantOutcome::Transform(v) => assert_eq!(v, json!({"qty": 100.0})),
            other => panic!("expected Transform, got {other:?}"),
        }
    }

    #[test]
    fn first_rejection_stops_evaluation() {
        struct AlwaysReject(&'static str);
        impl Invariant for AlwaysReject {
            fn name(&self) -> &'static str {
                self.0
            }
            fn check(&self, _: &Value, _: &Value) -> InvariantOutcome {
                InvariantOutcome::Reject {
                    reason: "by design".into(),
                }
            }
        }

        let set = InvariantSet::new()
            .with(Box::new(AlwaysReject("first")))
            .with(Box::new(AlwaysReject("second")));

        match set.apply(&json!({}), &json!({})) {
            InvariantOutcome::Reject { reason } => assert!(reason.starts_with("first")),
            _ => panic!("expected Reject"),
        }
    }
}
