//! Schema-side declarations for merge policies.
//!
//! [`MergePolicyRef`] is the serde-friendly shape that lives in the schema
//! JSON. At runtime the orchestrator calls [`MergePolicyRef::build`] to turn
//! each declaration into a boxed [`MergePolicy`] trait object.
//!
//! `LastWriteWins` is intentionally *not* declarable here. It requires
//! per-cycle timestamp metadata and must be installed programmatically via
//! [`PolicyMap::with`] with an explicit justification — see
//! [`escape_hatch`][crate::application::policy::escape_hatch] for rationale.

use super::{
    Additive, Append, MergePolicy, OwnedBy, PolicyMap, StateMachine, StateTransition,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Serializable declaration of a per-field merge policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MergePolicyRef {
    /// One side owns the field. `system` must match `MergeContext::system_a`
    /// or `system_b` at merge time.
    OwnedBy { system: String },

    /// Numeric counters — both sides' deltas accumulate.
    Additive,

    /// Array fields — concatenate both sides' additions.
    Append,

    /// Enum fields — only listed `(from, to)` transitions are allowed.
    StateMachine { transitions: Vec<TransitionRef> },
}

/// Flat serde shape for a single `(from, to)` allowed transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionRef {
    pub from: String,
    pub to: String,
}

impl From<&TransitionRef> for StateTransition {
    fn from(t: &TransitionRef) -> Self {
        StateTransition::new(&t.from, &t.to)
    }
}

impl MergePolicyRef {
    /// Instantiate the runtime policy trait object.
    pub fn build(&self) -> Box<dyn MergePolicy> {
        match self {
            Self::OwnedBy { system } => Box::new(OwnedBy::new(system)),
            Self::Additive => Box::new(Additive),
            Self::Append => Box::new(Append),
            Self::StateMachine { transitions } => Box::new(StateMachine::new(
                transitions.iter().map(StateTransition::from),
            )),
        }
    }
}

/// Build a [`PolicyMap`] from a `path -> MergePolicyRef` mapping. Keys are
/// canonical field paths (dotted, matching the changelog paths emitted by
/// three-way diff).
pub fn policy_map_from_declarations(
    declarations: &HashMap<String, MergePolicyRef>,
) -> PolicyMap {
    let mut map = PolicyMap::new();
    for (path, decl) in declarations {
        map = map.with(path.clone(), decl.build());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::diff::three_way_diff;
    use crate::application::policy::{MergeContext, MergeOutcome, resolve};
    use serde_json::{json, from_value};

    #[test]
    fn owned_by_roundtrips_through_json() {
        let decl = MergePolicyRef::OwnedBy {
            system: "netsuite".into(),
        };
        let j = serde_json::to_value(&decl).unwrap();
        assert_eq!(j, json!({"kind": "owned_by", "system": "netsuite"}));

        let parsed: MergePolicyRef = from_value(j).unwrap();
        assert_eq!(parsed, decl);
    }

    #[test]
    fn state_machine_roundtrips_through_json() {
        let decl = MergePolicyRef::StateMachine {
            transitions: vec![
                TransitionRef {
                    from: "draft".into(),
                    to: "open".into(),
                },
                TransitionRef {
                    from: "open".into(),
                    to: "closed".into(),
                },
            ],
        };
        let j = serde_json::to_value(&decl).unwrap();
        let parsed: MergePolicyRef = from_value(j).unwrap();
        assert_eq!(parsed, decl);
    }

    #[test]
    fn declarations_drive_resolution() {
        let mut decls = HashMap::new();
        decls.insert(
            "price".into(),
            MergePolicyRef::OwnedBy {
                system: "pricing".into(),
            },
        );
        decls.insert("qty".into(), MergePolicyRef::Additive);

        let policies = policy_map_from_declarations(&decls);

        let anc = json!({"price": 10, "qty": 5});
        let a = json!({"price": 15, "qty": 6}); // pricing side
        let b = json!({"price": 99, "qty": 7}); // non-owner attempt on price
        let log = three_way_diff(&anc, &a, &b);

        let ctx = MergeContext::new("pricing", "ops");
        let r = resolve(&log, &policies, &ctx);

        assert!(r.conflicts.is_empty());
        let resolved: HashMap<_, _> = r.resolved.into_iter().collect();
        // pricing owns price: A's 15 wins, B's 99 is vetoed
        assert_eq!(resolved["price"], json!(15));
        // qty is additive: 5 + 1 + 2 = 8
        assert_eq!(resolved["qty"], json!(8.0));
    }

    #[test]
    fn additive_declaration_builds_functional_policy() {
        let decl = MergePolicyRef::Additive;
        let policy = decl.build();
        assert_eq!(policy.name(), "additive");

        let anc = json!({"n": 10});
        let a = json!({"n": 12});
        let b = json!({"n": 11});
        let log = three_way_diff(&anc, &a, &b);
        let ctx = MergeContext::new("a", "b");

        match policy.merge(&log.changes[0], &ctx) {
            MergeOutcome::Resolved(v) => assert_eq!(v, json!(13.0)),
            _ => panic!("expected resolved"),
        }
    }
}
