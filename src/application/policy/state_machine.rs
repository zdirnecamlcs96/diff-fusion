//! `StateMachine` — enum fields with declared allowed transitions.
//!
//! For a field like `status` with values `draft | open | closed | cancelled`,
//! the policy rejects transitions not listed in its `allowed` set. This
//! prevents corrupt merges like "closed → draft" from silently taking
//! effect.
//!
//! When both sides move along different branches, the policy escalates —
//! picking a winner would require business judgement the policy doesn't have.

use super::{MergeContext, MergeOutcome, MergePolicy};
use crate::domain::diff::{ChangeSource, FieldChange};
use serde_json::Value;

/// One allowed `from -> to` transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTransition {
    pub from: String,
    pub to: String,
}

impl StateTransition {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
        }
    }
}

/// A state machine with a set of allowed transitions.
///
/// All string-valued states that do not appear in any transition are treated
/// as unreachable — a move *to* an unknown state is rejected.
#[derive(Debug, Clone)]
pub struct StateMachine {
    allowed: Vec<StateTransition>,
}

impl StateMachine {
    pub fn new(allowed: impl IntoIterator<Item = StateTransition>) -> Self {
        Self {
            allowed: allowed.into_iter().collect(),
        }
    }

    fn is_allowed(&self, from: &str, to: &str) -> bool {
        from == to || self.allowed.iter().any(|t| t.from == from && t.to == to)
    }
}

impl MergePolicy for StateMachine {
    fn name(&self) -> &'static str {
        "state_machine"
    }

    fn merge(&self, change: &FieldChange, _ctx: &MergeContext) -> MergeOutcome {
        let anc = match change.old_value.as_str() {
            Some(s) => s,
            None => return non_string("ancestor"),
        };

        let new_a = match change.new_from_a.as_ref() {
            Some(v) => match v.as_str() {
                Some(s) => Some(s),
                None => return non_string("a"),
            },
            None => None,
        };

        let new_b = match change.new_from_b.as_ref() {
            Some(v) => match v.as_str() {
                Some(s) => Some(s),
                None => return non_string("b"),
            },
            None => None,
        };

        match change.source {
            ChangeSource::A => {
                let to = new_a.expect("source=A implies new_from_a is Some");
                if self.is_allowed(anc, to) {
                    MergeOutcome::Resolved(Value::String(to.into()))
                } else {
                    MergeOutcome::Conflict {
                        reason: format!("illegal transition '{anc}' -> '{to}'"),
                    }
                }
            }
            ChangeSource::B => {
                let to = new_b.expect("source=B implies new_from_b is Some");
                if self.is_allowed(anc, to) {
                    MergeOutcome::Resolved(Value::String(to.into()))
                } else {
                    MergeOutcome::Conflict {
                        reason: format!("illegal transition '{anc}' -> '{to}'"),
                    }
                }
            }
            ChangeSource::Both => {
                let a = new_a.expect("source=Both implies new_from_a is Some");
                let b = new_b.expect("source=Both implies new_from_b is Some");
                if a == b {
                    // Both sides agree on the new state.
                    if self.is_allowed(anc, a) {
                        MergeOutcome::Resolved(Value::String(a.into()))
                    } else {
                        MergeOutcome::Conflict {
                            reason: format!("illegal transition '{anc}' -> '{a}' (both sides)"),
                        }
                    }
                } else {
                    MergeOutcome::Conflict {
                        reason: format!(
                            "divergent state transitions: '{anc}' -> '{a}' (A) vs '{anc}' -> '{b}' (B)"
                        ),
                    }
                }
            }
        }
    }
}

fn non_string(side: &str) -> MergeOutcome {
    MergeOutcome::Conflict {
        reason: format!("state_machine requires string values ({side} is not a string)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diff::three_way_diff;
    use serde_json::json;

    fn po_states() -> StateMachine {
        StateMachine::new([
            StateTransition::new("draft", "open"),
            StateTransition::new("open", "closed"),
            StateTransition::new("open", "cancelled"),
        ])
    }

    fn ctx() -> MergeContext {
        MergeContext::new("a", "b")
    }

    #[test]
    fn legal_transition_is_accepted() {
        let anc = json!({"status": "draft"});
        let a = json!({"status": "open"});
        let b = json!({"status": "draft"});
        let log = three_way_diff(&anc, &a, &b);

        let out = po_states().merge(&log.changes[0], &ctx());
        assert_eq!(out, MergeOutcome::Resolved(json!("open")));
    }

    #[test]
    fn illegal_transition_is_rejected() {
        // closed -> draft is not allowed.
        let anc = json!({"status": "closed"});
        let a = json!({"status": "draft"});
        let b = json!({"status": "closed"});
        let log = three_way_diff(&anc, &a, &b);

        let out = po_states().merge(&log.changes[0], &ctx());
        match out {
            MergeOutcome::Conflict { reason } => assert!(reason.contains("illegal transition")),
            _ => panic!("expected conflict"),
        }
    }

    #[test]
    fn both_agree_on_legal_transition() {
        let anc = json!({"status": "open"});
        let a = json!({"status": "closed"});
        let b = json!({"status": "closed"});
        let log = three_way_diff(&anc, &a, &b);

        let out = po_states().merge(&log.changes[0], &ctx());
        assert_eq!(out, MergeOutcome::Resolved(json!("closed")));
    }

    #[test]
    fn diverging_transitions_escalate() {
        let anc = json!({"status": "open"});
        let a = json!({"status": "closed"});
        let b = json!({"status": "cancelled"});
        let log = three_way_diff(&anc, &a, &b);

        let out = po_states().merge(&log.changes[0], &ctx());
        match out {
            MergeOutcome::Conflict { reason } => assert!(reason.contains("divergent")),
            _ => panic!("expected conflict"),
        }
    }
}
