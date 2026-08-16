//! Schema-side declarations for merge policies.
//!
//! [`MergePolicyRef`] is the serde-friendly shape that lives in the schema
//! JSON. At runtime the orchestrator calls [`MergePolicyRef::build`] to turn
//! each declaration into a boxed [`MergePolicy`] trait object.
//!
//! `LastWriteWins` is declarable here, but every declaration still carries
//! the same written `reason` and per-cycle timestamps the runtime
//! constructor requires — see
//! [`escape_hatch`][crate::application::policy::escape_hatch] for why it
//! should almost never be a schema's default strategy.

use super::{
    Additive, Append, LastWriteWins, MergePolicy, OnAdded, OnBothChanged, OnRemoved, OwnedBy,
    PolicyMap, SetByKey, StateMachine, StateTransition,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Serializable declaration of a per-field merge policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MergePolicyRef {
    /// One side owns the field. `system` must match `MergeContext::system_a`
    /// or `system_b` at merge time.
    OwnedBy {
        /// The owning system's label.
        system: String,
    },

    /// Numeric counters — both sides' deltas accumulate.
    Additive,

    /// Array fields — concatenate both sides' additions.
    Append,

    /// Enum fields — only listed `(from, to)` transitions are allowed.
    StateMachine {
        /// The allowed `(from, to)` transitions.
        transitions: Vec<TransitionRef>,
    },

    /// Escape hatch — requires a written reason and per-cycle timestamps.
    /// Never a schema's default strategy; see
    /// [`crate::application::policy::escape_hatch`] for rationale.
    LastWriteWins {
        /// Written justification for using this escape hatch; surfaces in
        /// logs and conflict reports.
        reason: String,
        /// Millisecond epoch for the A-side value.
        timestamp_a: u64,
        /// Millisecond epoch for the B-side value.
        timestamp_b: u64,
    },

    /// Keyed-set structural merge for arrays of objects — the full
    /// per-side add/remove/union configuration from
    /// [`crate::application::policy::structural::SetByKey`]. Only
    /// `identity`/`a_anchor`/`b_anchor` are required; the rest default to
    /// [`SetByKey::new`]'s defaults.
    SetByKey {
        /// Business fields forming the cross-system identity.
        identity: Vec<String>,
        /// Stable A-side row identifier used to rehome a row before
        /// identity matching.
        a_anchor: String,
        /// Same as `a_anchor` but for the B side.
        b_anchor: String,
        #[serde(default = "default_on_added")]
        #[cfg_attr(feature = "schema-gen", schemars(with = "Option<OnAdded>"))]
        on_added_in_a: OnAdded,
        #[serde(default = "default_on_added")]
        #[cfg_attr(feature = "schema-gen", schemars(with = "Option<OnAdded>"))]
        on_added_in_b: OnAdded,
        #[serde(default = "default_on_removed")]
        #[cfg_attr(feature = "schema-gen", schemars(with = "Option<OnRemoved>"))]
        on_removed_in_a: OnRemoved,
        #[serde(default = "default_on_removed")]
        #[cfg_attr(feature = "schema-gen", schemars(with = "Option<OnRemoved>"))]
        on_removed_in_b: OnRemoved,
        #[serde(default = "default_on_both_changed")]
        #[cfg_attr(feature = "schema-gen", schemars(with = "Option<OnBothChanged>"))]
        on_both_changed: OnBothChanged,
        #[serde(default = "default_true")]
        prefer_a_on_field_conflict: bool,
        /// Per-field nested policies, recursively merged. See
        /// [`SetByKeyRef`].
        #[serde(default)]
        nested: HashMap<String, SetByKeyRef>,
    },
}

fn default_on_added() -> OnAdded {
    OnAdded::Include
}

fn default_on_removed() -> OnRemoved {
    OnRemoved::EscalateIfChanged
}

fn default_on_both_changed() -> OnBothChanged {
    OnBothChanged::Escalate
}

fn default_true() -> bool {
    true
}

// `schemars(with = "Option<T>")` on the five enum-typed fields above is a
// schema-generation-only override — it does not change the field's actual
// Rust type or serde behavior. Without it, schemars 0.8's draft-07 output
// wraps any field that is both a bare `$ref` (a named, referenceable type
// like `OnAdded`) and carries a `default` in an `allOf: [{"$ref": ...}]`,
// which `scripts/schema-to-md.py` doesn't handle. `Option<T>`'s generated
// schema is an `anyOf`, not a bare `$ref`, so the `default` merges in
// directly with no `allOf`.

/// Satellite of [`MergePolicyRef::SetByKey`] used for its recursive
/// `nested` field — a `SetByKey` declaration can't recurse into itself as
/// an enum variant, so `nested` policies are this flat struct instead.
/// Same fields, same defaults; converts to the runtime
/// [`crate::application::policy::structural::SetByKey`] via [`From`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SetByKeyRef {
    /// Business fields forming the cross-system identity.
    pub identity: Vec<String>,
    /// Stable A-side row identifier used to rehome a row before identity
    /// matching.
    pub a_anchor: String,
    /// Same as `a_anchor` but for the B side.
    pub b_anchor: String,
    #[serde(default = "default_on_added")]
    #[cfg_attr(feature = "schema-gen", schemars(with = "Option<OnAdded>"))]
    pub on_added_in_a: OnAdded,
    #[serde(default = "default_on_added")]
    #[cfg_attr(feature = "schema-gen", schemars(with = "Option<OnAdded>"))]
    pub on_added_in_b: OnAdded,
    #[serde(default = "default_on_removed")]
    #[cfg_attr(feature = "schema-gen", schemars(with = "Option<OnRemoved>"))]
    pub on_removed_in_a: OnRemoved,
    #[serde(default = "default_on_removed")]
    #[cfg_attr(feature = "schema-gen", schemars(with = "Option<OnRemoved>"))]
    pub on_removed_in_b: OnRemoved,
    #[serde(default = "default_on_both_changed")]
    #[cfg_attr(feature = "schema-gen", schemars(with = "Option<OnBothChanged>"))]
    pub on_both_changed: OnBothChanged,
    #[serde(default = "default_true")]
    pub prefer_a_on_field_conflict: bool,
    /// Per-field nested policies. A field named here is recursively
    /// merged as its own keyed array rather than shallow-overlaid.
    #[serde(default)]
    pub nested: HashMap<String, SetByKeyRef>,
}

impl From<&SetByKeyRef> for SetByKey {
    fn from(r: &SetByKeyRef) -> Self {
        SetByKey {
            identity: r.identity.clone(),
            a_anchor: r.a_anchor.clone(),
            b_anchor: r.b_anchor.clone(),
            on_added_in_a: r.on_added_in_a,
            on_added_in_b: r.on_added_in_b,
            on_removed_in_a: r.on_removed_in_a,
            on_removed_in_b: r.on_removed_in_b,
            on_both_changed: r.on_both_changed,
            prefer_a_on_field_conflict: r.prefer_a_on_field_conflict,
            nested: r.nested.iter().map(|(k, v)| (k.clone(), SetByKey::from(v))).collect(),
        }
    }
}

/// Flat serde shape for a single `(from, to)` allowed transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct TransitionRef {
    /// State the transition starts from.
    pub from: String,
    /// State the transition moves to.
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
            Self::LastWriteWins {
                reason,
                timestamp_a,
                timestamp_b,
            } => Box::new(LastWriteWins::with_reason(
                reason.clone(),
                *timestamp_a,
                *timestamp_b,
            )),
            Self::SetByKey {
                identity,
                a_anchor,
                b_anchor,
                on_added_in_a,
                on_added_in_b,
                on_removed_in_a,
                on_removed_in_b,
                on_both_changed,
                prefer_a_on_field_conflict,
                nested,
            } => {
                let r = SetByKeyRef {
                    identity: identity.clone(),
                    a_anchor: a_anchor.clone(),
                    b_anchor: b_anchor.clone(),
                    on_added_in_a: *on_added_in_a,
                    on_added_in_b: *on_added_in_b,
                    on_removed_in_a: *on_removed_in_a,
                    on_removed_in_b: *on_removed_in_b,
                    on_both_changed: *on_both_changed,
                    prefer_a_on_field_conflict: *prefer_a_on_field_conflict,
                    nested: nested.clone(),
                };
                Box::new(SetByKey::from(&r))
            }
        }
    }
}

/// A whole entity type's worth of field policies, as authored by a host
/// (e.g. a JSON document in a `jsonb` column). `entity_type` is the lookup
/// key into a [`crate::ports::policy_store::PolicyStore`], not embedded
/// here — mirrors [`crate::ports::ancestor::AncestorKey`] being separate
/// from the entry it addresses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct PolicyDocument {
    /// Per-field policy declarations, keyed by canonical field path.
    #[serde(default)]
    pub fields: HashMap<String, MergePolicyRef>,
    /// Fallback policy for paths without an explicit entry in `fields`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<MergePolicyRef>,
}

impl PolicyDocument {
    /// Instantiate a [`PolicyMap`] from this document's declarations.
    pub fn build(&self) -> PolicyMap {
        let mut map = PolicyMap::new();
        for (path, decl) in &self.fields {
            map = map.with(path.clone(), decl.build());
        }
        if let Some(decl) = &self.default {
            map = map.with_default(decl.build());
        }
        map
    }
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
        use crate::application::policy::PolicyMap;
        use std::collections::HashMap;

        let mut decls: HashMap<String, MergePolicyRef> = HashMap::new();
        decls.insert(
            "price".into(),
            MergePolicyRef::OwnedBy {
                system: "pricing".into(),
            },
        );
        decls.insert("qty".into(), MergePolicyRef::Additive);

        let mut policies = PolicyMap::new();
        for (path, decl) in &decls {
            policies = policies.with(path.clone(), decl.build());
        }

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
    fn last_write_wins_roundtrip_and_build() {
        let decl: MergePolicyRef = from_value(json!({
            "kind": "last_write_wins",
            "reason": "vendor feed wins",
            "timestamp_a": 100,
            "timestamp_b": 50
        }))
        .unwrap();
        let policy = decl.build();
        assert_eq!(policy.name(), "last_write_wins");
    }

    #[test]
    fn set_by_key_roundtrip_and_build() {
        let decl: MergePolicyRef = from_value(json!({
            "kind": "set_by_key",
            "identity": ["sku"], "a_anchor": "warehouse", "b_anchor": "channel"
        }))
        .unwrap();
        let policy = decl.build();
        assert_eq!(policy.name(), "set_by_key");
    }

    #[test]
    fn set_by_key_widened_shape_roundtrips_through_json() {
        let decl: MergePolicyRef = from_value(json!({
            "kind": "set_by_key",
            "identity": ["sku"],
            "a_anchor": "warehouse",
            "b_anchor": "channel",
            "on_added_in_a": "Exclude",
            "on_added_in_b": "Include",
            "on_removed_in_a": "Remove",
            "on_removed_in_b": "EscalateIfChanged",
            "on_both_changed": "PreferB",
            "prefer_a_on_field_conflict": false,
            "nested": {}
        }))
        .unwrap();
        let j = serde_json::to_value(&decl).unwrap();
        let parsed: MergePolicyRef = from_value(j).unwrap();
        assert_eq!(parsed, decl);
    }

    #[test]
    fn set_by_key_nested_roundtrips_through_json() {
        let decl: MergePolicyRef = from_value(json!({
            "kind": "set_by_key",
            "identity": ["gid"],
            "a_anchor": "gid",
            "b_anchor": "gid",
            "on_both_changed": "Union",
            "nested": {
                "items": {"identity": ["sku"], "a_anchor": "sku", "b_anchor": "sku"}
            }
        }))
        .unwrap();
        let j = serde_json::to_value(&decl).unwrap();
        let parsed: MergePolicyRef = from_value(j).unwrap();
        assert_eq!(parsed, decl);
    }

    #[test]
    fn set_by_key_build_with_union_actually_unions() {
        let decl: MergePolicyRef = from_value(json!({
            "kind": "set_by_key",
            "identity": ["sku"],
            "a_anchor": "sku",
            "b_anchor": "sku",
            "on_both_changed": "Union"
        }))
        .unwrap();
        let policy = decl.build();

        let anc = json!({"items": [{"sku": "x", "q": 1}]});
        let a = json!({"items": [{"sku": "x", "q": 1, "a_extra": true}]});
        let b = json!({"items": [{"sku": "x", "q": 1, "b_extra": 42}]});
        let log = three_way_diff(&anc, &a, &b);
        let ctx = MergeContext::new("a", "b");

        match policy.merge(&log.changes[0], &ctx) {
            MergeOutcome::Resolved(v) => {
                let arr = v.as_array().unwrap();
                assert_eq!(arr.len(), 1);
                assert_eq!(arr[0]["a_extra"], json!(true));
                assert_eq!(arr[0]["b_extra"], json!(42));
            }
            other => panic!("expected merged array, got {other:?}"),
        }
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

    #[test]
    fn policy_document_roundtrips_through_json() {
        let doc: PolicyDocument = from_value(json!({
            "fields": {
                "price": {"kind": "owned_by", "system": "netsuite"},
                "notes": {"kind": "append"}
            },
            "default": {"kind": "additive"}
        }))
        .unwrap();

        assert_eq!(doc.fields.len(), 2);
        assert_eq!(doc.default, Some(MergePolicyRef::Additive));

        let j = serde_json::to_value(&doc).unwrap();
        let parsed: PolicyDocument = from_value(j).unwrap();
        assert_eq!(parsed, doc);
    }

    #[test]
    fn policy_document_defaults_to_empty_fields_and_no_default() {
        let doc: PolicyDocument = from_value(json!({})).unwrap();
        assert!(doc.fields.is_empty());
        assert!(doc.default.is_none());
    }

    #[test]
    fn policy_document_build_drives_resolution() {
        let doc: PolicyDocument = from_value(json!({
            "fields": {"price": {"kind": "owned_by", "system": "pricing"}},
            "default": {"kind": "additive"}
        }))
        .unwrap();
        let policies = doc.build();

        let anc = json!({"price": 10, "qty": 5});
        let a = json!({"price": 15, "qty": 6});
        let b = json!({"price": 99, "qty": 7});
        let log = three_way_diff(&anc, &a, &b);
        let ctx = MergeContext::new("pricing", "ops");

        let r = resolve(&log, &policies, &ctx);
        assert!(r.conflicts.is_empty());
        let resolved: HashMap<_, _> = r.resolved.into_iter().collect();
        assert_eq!(resolved["price"], json!(15));
        assert_eq!(resolved["qty"], json!(8.0));
    }
}
