//! Playground-local policies for cross-system array merging.
//!
//! The library ships `SetByKey` but it only takes a single identity field,
//! picks one whole side for matched elements, and never unions their fields.
//! Real reconciliations often need more:
//!
//! * **Composite identity** — same `sku` may appear on multiple lines with
//!   different `uom`, so the true identity is `(sku, uom)`, not `sku` alone.
//! * **Field union** — each system has its own local row ID (`externalId`
//!   vs `internalId`); the merged element should preserve BOTH.
//!
//! `SetByKeyComposite` covers both: identity is an ordered list of fields,
//! and `on_both_changed` selects how matched-but-divergent elements resolve.

use diff_fusion::application::policy::{MergeContext, MergeOutcome, MergePolicy};
use diff_fusion::domain::diff::FieldChange;
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Copy)]
pub enum OnBothChanged {
    /// Shallow-union the two elements' fields. A-side wins on direct field
    /// clashes by default; swap via `prefer_a_on_field_conflict`.
    Union,
    PreferA,
    PreferB,
    Escalate,
}

#[derive(Debug, Clone)]
pub struct SetByKeyComposite {
    /// Composite identity built from the canonical business fields
    /// (e.g. `["sku", "uom"]`). Used when no anchor match is found.
    pub identity: Vec<String>,
    /// Optional stable anchor field carried by every A-side element and by
    /// every ancestor element. When set, A-side rows are re-homed to their
    /// ancestor row via this anchor *before* composite-identity matching —
    /// so if A renames a business field that is part of `identity` (e.g.
    /// changes `uom` from CTN to BOX), the row still matches its old self.
    pub a_anchor: Option<String>,
    /// Same as `a_anchor` but for B-side rows.
    pub b_anchor: Option<String>,
    pub on_both_changed: OnBothChanged,
    pub prefer_a_on_field_conflict: bool,
}

impl SetByKeyComposite {
    pub fn new(identity: Vec<String>) -> Self {
        Self {
            identity,
            a_anchor: None,
            b_anchor: None,
            on_both_changed: OnBothChanged::Union,
            prefer_a_on_field_conflict: true,
        }
    }
}

impl MergePolicy for SetByKeyComposite {
    fn name(&self) -> &'static str {
        "set_by_key_composite"
    }

    fn merge(&self, change: &FieldChange, _ctx: &MergeContext) -> MergeOutcome {
        let anc = match change.old_value.as_array() {
            Some(a) => a,
            None => return conflict("ancestor is not an array"),
        };
        let a = match change.new_from_a.as_ref() {
            Some(v) => match v.as_array() {
                Some(arr) => arr,
                None => return conflict("A-side value is not an array"),
            },
            None => anc,
        };
        let b = match change.new_from_b.as_ref() {
            Some(v) => match v.as_array() {
                Some(arr) => arr,
                None => return conflict("B-side value is not an array"),
            },
            None => anc,
        };

        let idx_anc = match index_by(anc, &self.identity) {
            Ok(m) => m,
            Err(e) => return e,
        };

        // Build anchor lookups on the ancestor so each side's rows can be
        // re-homed to their original ancestor key even if the row has
        // mutated one of the identity fields.
        let anc_by_a_anchor = build_anchor_index(anc, self.a_anchor.as_deref(), &self.identity);
        let anc_by_b_anchor = build_anchor_index(anc, self.b_anchor.as_deref(), &self.identity);

        let idx_a = match index_with_anchor(
            a,
            &self.identity,
            self.a_anchor.as_deref(),
            &anc_by_a_anchor,
        ) {
            Ok(m) => m,
            Err(e) => return e,
        };
        let idx_b = match index_with_anchor(
            b,
            &self.identity,
            self.b_anchor.as_deref(),
            &anc_by_b_anchor,
        ) {
            Ok(m) => m,
            Err(e) => return e,
        };

        let keys: BTreeSet<&String> = idx_anc
            .keys()
            .chain(idx_a.keys())
            .chain(idx_b.keys())
            .collect();

        let mut out = Vec::new();
        for key in keys {
            let in_anc = idx_anc.get(key).map(|&i| &anc[i]);
            let in_a = idx_a.get(key).map(|&i| &a[i]);
            let in_b = idx_b.get(key).map(|&i| &b[i]);

            match (in_anc, in_a, in_b) {
                (_, Some(ea), Some(eb)) if ea == eb => out.push(ea.clone()),
                (_, Some(ea), Some(eb)) => match self.on_both_changed {
                    OnBothChanged::Union => {
                        out.push(self.union_elements(in_anc, ea, eb));
                    }
                    OnBothChanged::PreferA => out.push(ea.clone()),
                    OnBothChanged::PreferB => out.push(eb.clone()),
                    OnBothChanged::Escalate => {
                        return MergeOutcome::Conflict {
                            reason: format!("element `{key}` changed on both sides"),
                        };
                    }
                },
                (_, Some(ea), None) => out.push(ea.clone()),
                (_, None, Some(eb)) => out.push(eb.clone()),
                (Some(_), None, None) => { /* removed on both */ }
                (None, None, None) => unreachable!("key absent everywhere"),
            }
        }

        // Stable order: ancestor first, then new elements at the end.
        let identity = self.identity.clone();
        out.sort_by_key(move |elem| {
            composite_key(elem, &identity)
                .ok()
                .and_then(|k| {
                    anc.iter()
                        .position(|a| composite_key(a, &identity).ok() == Some(k.clone()))
                })
                .unwrap_or(usize::MAX)
        });

        MergeOutcome::Resolved(Value::Array(out))
    }
}

impl SetByKeyComposite {
    fn union_elements(&self, anc: Option<&Value>, a: &Value, b: &Value) -> Value {
        let mut out: Map<String, Value> = Map::new();
        if let Some(o) = anc.and_then(Value::as_object) {
            for (k, v) in o {
                out.insert(k.clone(), v.clone());
            }
        }
        let (loser, winner) = if self.prefer_a_on_field_conflict {
            (b, a)
        } else {
            (a, b)
        };
        if let Some(o) = loser.as_object() {
            for (k, v) in o {
                out.insert(k.clone(), v.clone());
            }
        }
        if let Some(o) = winner.as_object() {
            for (k, v) in o {
                out.insert(k.clone(), v.clone());
            }
        }
        Value::Object(out)
    }
}

/// Build the composite key for one element by joining each identity field's
/// stringified value with a unit-separator that won't appear in normal input.
fn composite_key(elem: &Value, identity: &[String]) -> Result<String, MergeOutcome> {
    let mut parts = Vec::with_capacity(identity.len());
    for field in identity {
        match elem.get(field) {
            Some(Value::String(s)) => parts.push(s.clone()),
            Some(Value::Number(n)) => parts.push(n.to_string()),
            Some(Value::Bool(b)) => parts.push(b.to_string()),
            Some(v) => parts.push(v.to_string()),
            None => {
                return Err(conflict(&format!(
                    "element missing identity field `{field}`"
                )));
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

/// Map an ancestor's anchor field value → the ancestor's composite-identity
/// key. Returns an empty map when no anchor is configured or the ancestor
/// predates anchor tracking.
fn build_anchor_index(
    anc: &[Value],
    anchor: Option<&str>,
    identity: &[String],
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(field) = anchor else {
        return map;
    };
    for elem in anc {
        let Some(anchor_val) = elem.get(field).and_then(anchor_to_string) else {
            continue;
        };
        let Ok(key) = composite_key(elem, identity) else {
            continue;
        };
        map.insert(anchor_val, key);
    }
    map
}

/// Index `side` rows by composite identity, but first try to re-home each
/// row to an ancestor key via the anchor field. This is what lets the merge
/// survive mutation of identity fields on one side.
fn index_with_anchor(
    side: &[Value],
    identity: &[String],
    anchor: Option<&str>,
    anc_anchor_map: &HashMap<String, String>,
) -> Result<HashMap<String, usize>, MergeOutcome> {
    let mut out = HashMap::new();
    for (i, elem) in side.iter().enumerate() {
        let key = if let Some(field) = anchor {
            if let Some(anchor_val) = elem.get(field).and_then(anchor_to_string) {
                anc_anchor_map
                    .get(&anchor_val)
                    .cloned()
                    .unwrap_or_else(|| composite_key(elem, identity).unwrap_or_default())
            } else {
                composite_key(elem, identity)?
            }
        } else {
            composite_key(elem, identity)?
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

fn conflict(reason: &str) -> MergeOutcome {
    MergeOutcome::Conflict {
        reason: reason.to_string(),
    }
}
