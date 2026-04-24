//! Tier 3 — structural merges for collections.
//!
//! Flat per-field policies cannot express "this line item exists in A but
//! not B — was it added in A or deleted in B?". That's a structural
//! question about set identity, not a value-level one. [`SetByKey`] answers
//! it by declaring the composite business identity **and** the stable
//! per-side anchor fields that carry each system's local row ID.
//!
//! # Why anchors are required
//!
//! Every real integration involves one system that hands out immutable
//! local IDs (e.g. NetSuite `internalId`) and lets users rename business
//! fields (SKU, UOM, line number). Without a stable anchor, a rename
//! shows up as "element removed and a new one added", which corrupts
//! three-way diffing. Anchor **field names** are therefore mandatory
//! configuration; anchor **values** on individual elements may be absent
//! (e.g. a row A just created that hasn't roundtripped to B yet), in
//! which case matching falls through to the composite identity.
//!
//! # Nested line items
//!
//! Matched elements whose `on_both_changed = Union` get a shallow field
//! union by default. For any field named in `nested`, a sub-`SetByKey`
//! recursively merges that field as its own keyed array, preserving
//! per-line detail (e.g. `deliveryFullfillment[*].items[]` inside a
//! matched fulfillment).

use super::{MergeContext, MergeOutcome, MergePolicy};
use crate::domain::diff::FieldChange;
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap};

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
    /// Shallow-union the two elements' fields. A-side wins on direct
    /// field clashes unless `prefer_a_on_field_conflict` is set to false.
    Union,
}

/// Merge arrays of objects by a composite identity, with per-side stable
/// anchors and per-side added/removed resolution strategies.
#[derive(Debug, Clone)]
pub struct SetByKey {
    /// Ordered list of business fields forming the cross-system identity
    /// (e.g. `["sku", "uom"]`). Used when anchor rehoming doesn't match.
    pub identity: Vec<String>,
    /// Stable A-side row identifier (e.g. internal `_id` / UUID). A-side
    /// rows are rehomed to their ancestor row via this anchor *before*
    /// composite-identity matching, so if A renames an identity field the
    /// row still matches its former self.
    pub a_anchor: String,
    /// Same as `a_anchor` but for B-side rows.
    pub b_anchor: String,
    pub on_added_in_a: OnAdded,
    pub on_added_in_b: OnAdded,
    pub on_removed_in_a: OnRemoved,
    pub on_removed_in_b: OnRemoved,
    pub on_both_changed: OnBothChanged,
    /// Only used when `on_both_changed == Union`. When true, A-side
    /// values win on a per-field clash between the two matched elements;
    /// when false, B wins.
    pub prefer_a_on_field_conflict: bool,
    /// Per-field nested policies. When a matched pair is unioned, any
    /// field name present here is recursively merged as its own keyed
    /// array rather than shallow-overlaid.
    pub nested: HashMap<String, SetByKey>,
}

impl SetByKey {
    /// Check that this policy's anchor / identity fields are declared at
    /// the right places in a CIF array-element schema. `element_schema`
    /// is the `element` object from a CIF schema's array field — e.g.
    /// `schema.cif_schema.items.element`. Returns the list of validation
    /// errors (empty Vec if all good).
    ///
    /// Verifies:
    /// * `a_anchor` is declared with `anchor: "a"`.
    /// * `b_anchor` is declared with `anchor: "b"`.
    /// * every `identity` field exists on the element.
    /// * any `nested` policy's declared path exists on the element with
    ///   `type: array` and recurses into the nested element schema.
    pub fn validate_against_element_schema(
        &self,
        element_schema: &serde_json::Value,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        let obj = match element_schema.as_object() {
            Some(o) => o,
            None => {
                errors.push("element schema must be an object".into());
                return errors;
            }
        };

        let anchor_role = |field: &str| -> Option<String> {
            obj.get(field)
                .and_then(|f| f.get("anchor"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };

        match anchor_role(&self.a_anchor) {
            Some(r) if r == "a" => {}
            Some(other) => errors.push(format!(
                "a_anchor '{}' is declared with anchor='{}', expected 'a'",
                self.a_anchor, other
            )),
            None => errors.push(format!(
                "a_anchor '{}' is not declared as an anchor field in the element schema",
                self.a_anchor
            )),
        }
        match anchor_role(&self.b_anchor) {
            Some(r) if r == "b" => {}
            Some(other) => errors.push(format!(
                "b_anchor '{}' is declared with anchor='{}', expected 'b'",
                self.b_anchor, other
            )),
            None => errors.push(format!(
                "b_anchor '{}' is not declared as an anchor field in the element schema",
                self.b_anchor
            )),
        }

        for id_field in &self.identity {
            if !obj.contains_key(id_field) {
                errors.push(format!(
                    "identity field '{id_field}' not declared in element schema"
                ));
            }
        }

        for (nested_field, nested_policy) in &self.nested {
            match obj.get(nested_field) {
                Some(f) => {
                    let ty = f.get("type").and_then(serde_json::Value::as_str);
                    if ty != Some("array") {
                        errors.push(format!(
                            "nested field '{nested_field}' must be declared as type='array' \
                             in the element schema (got {ty:?})"
                        ));
                        continue;
                    }
                    match f.get("element") {
                        Some(inner) => {
                            for e in nested_policy.validate_against_element_schema(inner) {
                                errors.push(format!("nested.{nested_field}: {e}"));
                            }
                        }
                        None => errors.push(format!(
                            "nested field '{nested_field}' has no 'element' schema declared"
                        )),
                    }
                }
                None => errors.push(format!(
                    "nested field '{nested_field}' not declared in element schema"
                )),
            }
        }

        errors
    }

    /// Minimal constructor — composite identity plus the two anchor field
    /// names. Defaults: include additions from either side, escalate when
    /// a side modified an element the other side removed, escalate when
    /// both modified the same element.
    pub fn new(
        identity: Vec<String>,
        a_anchor: impl Into<String>,
        b_anchor: impl Into<String>,
    ) -> Self {
        Self {
            identity,
            a_anchor: a_anchor.into(),
            b_anchor: b_anchor.into(),
            on_added_in_a: OnAdded::Include,
            on_added_in_b: OnAdded::Include,
            on_removed_in_a: OnRemoved::EscalateIfChanged,
            on_removed_in_b: OnRemoved::EscalateIfChanged,
            on_both_changed: OnBothChanged::Escalate,
            prefer_a_on_field_conflict: true,
            nested: HashMap::new(),
        }
    }
}

impl MergePolicy for SetByKey {
    fn name(&self) -> &'static str {
        "set_by_key"
    }

    fn validate_against_schema(&self, field_schema: &Value) -> Vec<String> {
        // An empty / missing field schema means "no declared element
        // shape" — can't verify anchors. That's a gap worth surfacing so
        // the user either declares the schema or accepts the risk.
        if field_schema.is_null() {
            return vec![
                "no CIF schema declared for this field; cannot verify anchor wiring".into(),
            ];
        }
        let ty = field_schema.get("type").and_then(Value::as_str);
        if ty != Some("array") {
            return vec![format!(
                "field declared as type={ty:?}, but set_by_key requires type='array'"
            )];
        }
        match field_schema.get("element") {
            Some(elem) => self.validate_against_element_schema(elem),
            None => vec![
                "array field has no 'element' schema; declare one to verify anchor wiring".into(),
            ],
        }
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
        match self.merge_arrays(anc, a, b) {
            Ok(arr) => MergeOutcome::Resolved(Value::Array(arr)),
            Err(outcome) => outcome,
        }
    }
}

impl SetByKey {
    /// Three-way merge three arrays by composite identity with anchor
    /// rehoming. Recursively invoked by `union_elements` for any nested
    /// array field declared in `self.nested`.
    fn merge_arrays(
        &self,
        anc: &[Value],
        a: &[Value],
        b: &[Value],
    ) -> Result<Vec<Value>, MergeOutcome> {
        let idx_anc = index_by(anc, &self.identity)?;

        let anc_by_a_anchor = build_anchor_index(anc, &self.a_anchor, &self.identity);
        let anc_by_b_anchor = build_anchor_index(anc, &self.b_anchor, &self.identity);

        let idx_a = index_with_anchor(a, &self.identity, &self.a_anchor, &anc_by_a_anchor)?;
        let idx_b = index_with_anchor(b, &self.identity, &self.b_anchor, &anc_by_b_anchor)?;

        let all_keys: BTreeSet<&String> = idx_anc
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
                // Both sides converged on the same value (possibly after
                // making the identical edit). No conflict regardless of
                // whether either side changed from ancestor.
                (Some(_), Some(elem_a), Some(elem_b)) if elem_a == elem_b => {
                    out.push(elem_a.clone());
                }
                // Changed only on A (B matches ancestor).
                (Some(ea), Some(elem_a), Some(elem_b)) if ea == elem_b => {
                    out.push(elem_a.clone());
                }
                // Changed only on B (A matches ancestor).
                (Some(ea), Some(elem_a), Some(elem_b)) if ea == elem_a => {
                    out.push(elem_b.clone());
                }
                // Changed on both — apply on_both_changed.
                (Some(ea), Some(elem_a), Some(elem_b)) => match self.on_both_changed {
                    OnBothChanged::PreferA => out.push(elem_a.clone()),
                    OnBothChanged::PreferB => out.push(elem_b.clone()),
                    OnBothChanged::Union => {
                        out.push(self.union_elements(Some(ea), elem_a, elem_b)?);
                    }
                    OnBothChanged::Escalate => {
                        return Err(MergeOutcome::Conflict {
                            reason: format!("element '{key}' changed on both sides"),
                        });
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
                            OnBothChanged::Union => {
                                out.push(self.union_elements(None, elem_a, elem_b)?);
                            }
                            OnBothChanged::Escalate => {
                                return Err(MergeOutcome::Conflict {
                                    reason: format!(
                                        "element '{key}' added divergently on both sides"
                                    ),
                                });
                            }
                        }
                    }
                }
                // Removed in A.
                (Some(ea), None, Some(elem_b)) => {
                    if ea == elem_b {
                        // B didn't change it; honor the removal.
                    } else if self.on_removed_in_a == OnRemoved::EscalateIfChanged {
                        return Err(MergeOutcome::Conflict {
                            reason: format!("element '{key}' removed in A but changed in B"),
                        });
                    }
                    // else OnRemoved::Remove: drop it.
                }
                // Removed in B.
                (Some(ea), Some(elem_a), None) => {
                    if ea == elem_a {
                        // A didn't change it; honor the removal.
                    } else if self.on_removed_in_b == OnRemoved::EscalateIfChanged {
                        return Err(MergeOutcome::Conflict {
                            reason: format!("element '{key}' removed in B but changed in A"),
                        });
                    }
                    // else OnRemoved::Remove: drop it.
                }
                // Removed in both — honor the removal everywhere.
                (Some(_), None, None) => {}
                (None, None, None) => unreachable!("key not present anywhere"),
            }
        }

        // Preserve ancestor's order where possible.
        let identity = self.identity.clone();
        let anc_snapshot = anc.to_vec();
        out.sort_by_key(move |elem| {
            composite_key(elem, &identity)
                .ok()
                .and_then(|k| {
                    anc_snapshot
                        .iter()
                        .position(|a| composite_key(a, &identity).ok() == Some(k.clone()))
                })
                .unwrap_or(usize::MAX)
        });

        Ok(out)
    }

    fn union_elements(
        &self,
        anc: Option<&Value>,
        a: &Value,
        b: &Value,
    ) -> Result<Value, MergeOutcome> {
        let mut out: Map<String, Value> = Map::new();
        if let Some(o) = anc.and_then(Value::as_object) {
            for (k, v) in o {
                if self.nested.contains_key(k) {
                    continue;
                }
                out.insert(k.clone(), v.clone());
            }
        }
        let (loser, winner) = if self.prefer_a_on_field_conflict {
            (b, a)
        } else {
            (a, b)
        };
        for side in [loser, winner] {
            if let Some(o) = side.as_object() {
                for (k, v) in o {
                    if self.nested.contains_key(k) {
                        continue;
                    }
                    out.insert(k.clone(), v.clone());
                }
            }
        }

        for (field, sub_policy) in &self.nested {
            // Skip entirely when no side carries this field — avoids
            // injecting an empty array where none existed.
            if anc.and_then(|v| v.get(field)).is_none()
                && a.get(field).is_none()
                && b.get(field).is_none()
            {
                continue;
            }
            let anc_arr = anc
                .and_then(|v| v.get(field))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let a_arr = a
                .get(field)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let b_arr = b
                .get(field)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let merged = sub_policy.merge_arrays(&anc_arr, &a_arr, &b_arr)?;
            out.insert(field.clone(), Value::Array(merged));
        }

        Ok(Value::Object(out))
    }
}

/// Build the composite key by joining each identity field's stringified
/// value with a unit-separator that won't appear in normal input.
fn composite_key(elem: &Value, identity: &[String]) -> Result<String, MergeOutcome> {
    let mut parts = Vec::with_capacity(identity.len());
    for field in identity {
        match elem.get(field) {
            Some(Value::String(s)) => parts.push(s.clone()),
            Some(Value::Number(n)) => parts.push(n.to_string()),
            Some(Value::Bool(b)) => parts.push(b.to_string()),
            Some(v) => parts.push(v.to_string()),
            None => {
                return Err(MergeOutcome::Conflict {
                    reason: format!("element missing identity field '{field}'"),
                });
            }
        }
    }
    Ok(parts.join("\u{1f}"))
}

fn index_by(arr: &[Value], identity: &[String]) -> Result<HashMap<String, usize>, MergeOutcome> {
    let mut out = HashMap::new();
    for (i, elem) in arr.iter().enumerate() {
        let key = composite_key(elem, identity)?;
        out.insert(key, i);
    }
    Ok(out)
}

/// Map an ancestor's anchor field value → ancestor's composite-identity
/// key. Empty when no ancestor element has the anchor populated (e.g.
/// pre-anchor rows still in ancestor storage).
fn build_anchor_index(
    anc: &[Value],
    anchor: &str,
    identity: &[String],
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for elem in anc {
        let Some(anchor_val) = elem.get(anchor).and_then(anchor_to_string) else {
            continue;
        };
        let Ok(key) = composite_key(elem, identity) else {
            continue;
        };
        map.insert(anchor_val, key);
    }
    map
}

/// Index `side` rows by composite identity, but first try to rehome each
/// row to an ancestor key via the anchor field. Rows without an anchor
/// value fall through to composite identity — that's the normal case for
/// rows a side just created that haven't roundtripped yet.
fn index_with_anchor(
    side: &[Value],
    identity: &[String],
    anchor: &str,
    anc_anchor_map: &HashMap<String, String>,
) -> Result<HashMap<String, usize>, MergeOutcome> {
    let mut out = HashMap::new();
    for (i, elem) in side.iter().enumerate() {
        let key = match elem.get(anchor).and_then(anchor_to_string) {
            Some(anchor_val) => anc_anchor_map
                .get(&anchor_val)
                .cloned()
                .unwrap_or_else(|| composite_key(elem, identity).unwrap_or_default()),
            None => composite_key(elem, identity)?,
        };
        out.insert(key, i);
    }
    Ok(out)
}

fn anchor_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
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

    /// Small helper: build a `SetByKey` keyed by a single field with
    /// identity-as-anchor (anchor rehoming collapses to a no-op when the
    /// test data doesn't carry separate local IDs).
    fn policy_by(id: &str) -> SetByKey {
        SetByKey::new(vec![id.to_string()], id, id)
    }

    #[test]
    fn additions_on_both_sides_merge() {
        let anc = json!({"items": [{"sku": "x", "q": 1}]});
        let a = json!({"items": [{"sku": "x", "q": 1}, {"sku": "y", "q": 2}]});
        let b = json!({"items": [{"sku": "x", "q": 1}, {"sku": "z", "q": 3}]});
        let log = three_way_diff(&anc, &a, &b);

        let policy = policy_by("sku");
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

        let policy = policy_by("sku");
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

        let policy = policy_by("sku");
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

        let policy = policy_by("sku");
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

        let policy = policy_by("sku");
        assert!(matches!(
            policy.merge(&log.changes[0], &ctx()),
            MergeOutcome::Conflict { .. }
        ));
    }

    #[test]
    fn anchor_rehomes_row_across_identity_mutation() {
        // A renamed the identity field from "old" to "new" for the row
        // whose stable anchor is "id":1. Without anchor rehoming this
        // would look like "old" removed + "new" added; with anchor it's
        // a plain field edit.
        let anc = json!({
            "items": [{"sku": "old", "id": 1, "q": 5}]
        });
        let a = json!({
            "items": [{"sku": "new", "id": 1, "q": 5}]
        });
        let b = json!({
            "items": [{"sku": "old", "id": 1, "q": 5}]
        });
        let log = three_way_diff(&anc, &a, &b);

        let policy = SetByKey::new(vec!["sku".into()], "id", "id");
        match policy.merge(&log.changes[0], &ctx()) {
            MergeOutcome::Resolved(Value::Array(arr)) => {
                assert_eq!(arr.len(), 1);
                assert_eq!(arr[0]["sku"], json!("new"));
                assert_eq!(arr[0]["id"], json!(1));
            }
            other => panic!("expected merged array, got {other:?}"),
        }
    }

    #[test]
    fn union_strategy_merges_matched_element_fields() {
        // A added a field, B added a different field, both on the same
        // element. Union preserves both.
        let anc = json!({"items": [{"sku": "x", "q": 1}]});
        let a = json!({"items": [{"sku": "x", "q": 1, "a_extra": true}]});
        let b = json!({"items": [{"sku": "x", "q": 1, "b_extra": 42}]});
        let log = three_way_diff(&anc, &a, &b);

        let mut policy = policy_by("sku");
        policy.on_both_changed = OnBothChanged::Union;
        match policy.merge(&log.changes[0], &ctx()) {
            MergeOutcome::Resolved(Value::Array(arr)) => {
                assert_eq!(arr.len(), 1);
                assert_eq!(arr[0]["a_extra"], json!(true));
                assert_eq!(arr[0]["b_extra"], json!(42));
            }
            other => panic!("expected merged array, got {other:?}"),
        }
    }

    #[test]
    fn validate_against_element_schema_passes_when_anchors_declared() {
        let schema = json!({
            "externalId": {"type": "string", "anchor": "a"},
            "internalId": {"type": "string", "anchor": "b"},
            "sku": {"type": "string"},
            "uom": {"type": "string"}
        });
        let policy = SetByKey::new(vec!["sku".into(), "uom".into()], "externalId", "internalId");
        assert!(policy.validate_against_element_schema(&schema).is_empty());
    }

    #[test]
    fn validate_against_element_schema_flags_missing_anchor() {
        let schema = json!({
            "externalId": {"type": "string", "anchor": "a"},
            // missing internalId anchor
            "sku": {"type": "string"}
        });
        let policy = SetByKey::new(vec!["sku".into()], "externalId", "internalId");
        let errs = policy.validate_against_element_schema(&schema);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("b_anchor 'internalId'"));
    }

    #[test]
    fn validate_against_element_schema_flags_wrong_anchor_role() {
        let schema = json!({
            "externalId": {"type": "string", "anchor": "b"}, // role flipped
            "internalId": {"type": "string", "anchor": "a"}, // role flipped
            "sku": {"type": "string"}
        });
        let policy = SetByKey::new(vec!["sku".into()], "externalId", "internalId");
        let errs = policy.validate_against_element_schema(&schema);
        assert_eq!(errs.len(), 2);
        assert!(errs.iter().any(|e| e.contains("expected 'a'")));
        assert!(errs.iter().any(|e| e.contains("expected 'b'")));
    }

    #[test]
    fn validate_against_element_schema_recurses_into_nested() {
        let schema = json!({
            "externalId": {"type": "string", "anchor": "a"},
            "internalId": {"type": "string", "anchor": "b"},
            "sku": {"type": "string"},
            "subLines": {
                "type": "array",
                "element": {
                    "extSubId": {"type": "string", "anchor": "a"},
                    "intSubId": {"type": "string", "anchor": "b"},
                    "sku": {"type": "string"}
                }
            }
        });
        let mut policy = SetByKey::new(vec!["sku".into()], "externalId", "internalId");
        policy.nested.insert(
            "subLines".into(),
            SetByKey::new(vec!["sku".into()], "extSubId", "intSubId"),
        );
        assert!(policy.validate_against_element_schema(&schema).is_empty());
    }

    #[test]
    fn nested_policy_merges_sub_array_recursively() {
        // Both sides added distinct items under the same parent element.
        // Nested SetByKey on "items" reconciles them; outer Union keeps
        // the parent record.
        let anc = json!({
            "groups": [
                {"gid": "G1", "items": [{"sku": "A", "q": 1}]}
            ]
        });
        let a = json!({
            "groups": [
                {"gid": "G1", "items": [{"sku": "A", "q": 1}, {"sku": "B", "q": 2}]}
            ]
        });
        let b = json!({
            "groups": [
                {"gid": "G1", "items": [{"sku": "A", "q": 1}, {"sku": "C", "q": 3}]}
            ]
        });
        let log = three_way_diff(&anc, &a, &b);

        let mut outer = policy_by("gid");
        outer.on_both_changed = OnBothChanged::Union;
        outer.nested.insert("items".into(), policy_by("sku"));

        match outer.merge(&log.changes[0], &ctx()) {
            MergeOutcome::Resolved(Value::Array(arr)) => {
                assert_eq!(arr.len(), 1);
                let items = arr[0]["items"].as_array().unwrap();
                let skus: Vec<_> = items.iter().map(|v| v["sku"].as_str().unwrap()).collect();
                assert!(skus.contains(&"A") && skus.contains(&"B") && skus.contains(&"C"));
                assert_eq!(items.len(), 3);
            }
            other => panic!("expected merged array, got {other:?}"),
        }
    }
}
