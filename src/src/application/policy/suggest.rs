//! Heuristic policy suggestions derived from a CIF schema.
//!
//! A single pure function — [`suggest_policies`] — walks a CIF schema and
//! returns a draft `per_field` policy declaration that callers (today: the
//! playground's `POST /api/suggest` endpoint; tomorrow: a CLI or library
//! user) can feed straight back into the merge engine after review.
//!
//! The output is **JSON shaped like the playground's existing `per_field`
//! block**, not a `HashMap<String, MergePolicyRef>` — so adding this
//! function doesn't require extending the declaration enum. The three
//! kinds emitted (`owned_by`, `additive`, `set_by_key`) are exactly the
//! three the existing `pipeline::build_policy` already accepts.
//!
//! # Heuristics
//!
//! - A field carrying `source_of_truth: "X"` → `{"kind": "owned_by", "system": "X"}`.
//!   Source-of-truth wins over every other rule.
//! - A `type: "array"` field whose `element` declares exactly one
//!   `anchor: "a"` and one `anchor: "b"` → `set_by_key` seeded with those
//!   anchors. `identity` defaults to the element's non-anchor **required
//!   string** fields, falling back to every non-anchor string field when
//!   nothing is marked required.
//! - A `type: "number"` field without `source_of_truth` → `additive`.
//! - Anything else is omitted. Undeclared paths escalate at merge time by
//!   design — the suggester does not invent a guess.
//!
//! The function is lenient on malformed input: missing or non-object
//! schemas return an empty map rather than panicking. Validation is the
//! caller's job (`PolicyMap::validate_against_schema` already handles it).

use serde_json::{Map, Value, json};

/// Walk `schema.cif_schema.*` and emit a draft `per_field` declaration.
///
/// The returned value is always a `Value::Object` whose keys are CIF
/// field paths and whose values are policy declarations in the same shape
/// the playground's `pipeline::build_policy` already consumes.
pub fn suggest_policies(schema: &Value) -> Value {
    let mut out = Map::new();
    let Some(cif) = schema.get("cif_schema").and_then(Value::as_object) else {
        return Value::Object(out);
    };
    for (path, field_def) in cif {
        if let Some(policy) = suggest_for_field(field_def) {
            out.insert(path.clone(), policy);
        }
    }
    Value::Object(out)
}

fn suggest_for_field(def: &Value) -> Option<Value> {
    let obj = def.as_object()?;

    if let Some(system) = str_field(obj, "source_of_truth") {
        return Some(json!({ "kind": "owned_by", "system": system }));
    }

    match str_field(obj, "type") {
        Some("array") => suggest_set_by_key(obj.get("element")?.as_object()?),
        Some("number") => Some(json!({ "kind": "additive" })),
        _ => None,
    }
}

fn str_field<'a>(obj: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    obj.get(key).and_then(Value::as_str)
}

fn type_of(def: &Value) -> Option<&str> {
    def.get("type").and_then(Value::as_str)
}

fn suggest_set_by_key(element: &Map<String, Value>) -> Option<Value> {
    let (a_anchor, b_anchor, identity) = extract_anchors_and_identity(element)?;
    let mut policy = json!({
        "kind": "set_by_key",
        "identity": identity,
        "a_anchor": a_anchor,
        "b_anchor": b_anchor,
    });
    let nested = suggest_nested(element);
    if !nested.is_empty() {
        policy["nested"] = Value::Object(nested);
    }
    Some(policy)
}

/// Scan an array's element schema for one `anchor: "a"` and one
/// `anchor: "b"`. Returns `None` if either anchor is missing or ambiguous
/// (e.g. two fields both marked `anchor: "a"`) so the caller treats that
/// field as un-suggestable rather than emitting a broken policy.
fn extract_anchors_and_identity(
    element: &Map<String, Value>,
) -> Option<(String, String, Vec<String>)> {
    let mut a_anchor: Option<String> = None;
    let mut b_anchor: Option<String> = None;

    for (name, def) in element {
        let slot = match def.get("anchor").and_then(Value::as_str) {
            Some("a") => &mut a_anchor,
            Some("b") => &mut b_anchor,
            _ => continue,
        };
        if slot.is_some() {
            // Duplicate anchor on this side — refuse to guess.
            return None;
        }
        *slot = Some(name.clone());
    }

    let a = a_anchor?;
    let b = b_anchor?;

    // Prefer required string fields for identity; fall back to every
    // non-anchor string field when nothing's marked required. Users can
    // override either way in the dialog — this is a seed, not a decree.
    let mut required_strings: Vec<String> = Vec::new();
    let mut all_strings: Vec<String> = Vec::new();
    for (name, def) in element {
        if name == &a || name == &b || def.get("anchor").is_some() {
            continue;
        }
        if type_of(def) != Some("string") {
            continue;
        }
        all_strings.push(name.clone());
        if def.get("required").and_then(Value::as_bool) == Some(true) {
            required_strings.push(name.clone());
        }
    }
    let identity = if required_strings.is_empty() {
        all_strings
    } else {
        required_strings
    };

    Some((a, b, identity))
}

/// For each field in `element` that is itself an array with its own
/// `anchor: a` / `anchor: b` pair, emit a nested `set_by_key` declaration.
/// Matches the recursive `nested` map the playground's pipeline parses.
fn suggest_nested(element: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (name, def) in element {
        if type_of(def) != Some("array") {
            continue;
        }
        let Some(inner) = def.get("element").and_then(Value::as_object) else {
            continue;
        };
        if let Some(nested) = suggest_set_by_key(inner) {
            out.insert(name.clone(), nested);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_schema_returns_empty_map() {
        assert_eq!(suggest_policies(&json!({})), json!({}));
        assert_eq!(suggest_policies(&json!({ "cif_schema": {} })), json!({}));
    }

    #[test]
    fn source_of_truth_becomes_owned_by() {
        let schema = json!({
            "cif_schema": {
                "price": { "type": "number", "source_of_truth": "erp" }
            }
        });
        assert_eq!(
            suggest_policies(&schema),
            json!({ "price": { "kind": "owned_by", "system": "erp" } }),
        );
    }

    #[test]
    fn number_without_source_of_truth_becomes_additive() {
        let schema = json!({
            "cif_schema": {
                "qty_recv": { "type": "number" }
            }
        });
        assert_eq!(
            suggest_policies(&schema),
            json!({ "qty_recv": { "kind": "additive" } }),
        );
    }

    #[test]
    fn source_of_truth_overrides_additive_on_numbers() {
        let schema = json!({
            "cif_schema": {
                "price": { "type": "number", "source_of_truth": "pricing" }
            }
        });
        let policy = suggest_policies(&schema);
        assert_eq!(policy["price"]["kind"], json!("owned_by"));
    }

    #[test]
    fn string_without_hint_is_not_suggested() {
        let schema = json!({
            "cif_schema": {
                "name": { "type": "string" }
            }
        });
        assert_eq!(suggest_policies(&schema), json!({}));
    }

    #[test]
    fn array_with_both_anchors_becomes_set_by_key() {
        let schema = json!({
            "cif_schema": {
                "items": {
                    "type": "array",
                    "element": {
                        "externalId": { "type": "string", "anchor": "a" },
                        "internalId": { "type": "string", "anchor": "b" },
                        "sku": { "type": "string", "required": true },
                        "uom": { "type": "string", "required": true },
                        "qty": { "type": "number" }
                    }
                }
            }
        });
        let got = suggest_policies(&schema);
        let items = &got["items"];
        assert_eq!(items["kind"], "set_by_key");
        assert_eq!(items["a_anchor"], "externalId");
        assert_eq!(items["b_anchor"], "internalId");
        let identity: Vec<&str> = items["identity"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(identity.contains(&"sku"));
        assert!(identity.contains(&"uom"));
        assert!(!identity.contains(&"qty"));
        assert!(!identity.contains(&"externalId"));
    }

    #[test]
    fn array_without_both_anchors_is_not_suggested() {
        let schema = json!({
            "cif_schema": {
                "items": {
                    "type": "array",
                    "element": {
                        "externalId": { "type": "string", "anchor": "a" },
                        "sku": { "type": "string" }
                    }
                }
            }
        });
        assert_eq!(suggest_policies(&schema), json!({}));
    }

    #[test]
    fn array_with_duplicate_anchor_is_not_suggested() {
        let schema = json!({
            "cif_schema": {
                "items": {
                    "type": "array",
                    "element": {
                        "id1": { "type": "string", "anchor": "a" },
                        "id2": { "type": "string", "anchor": "a" },
                        "internalId": { "type": "string", "anchor": "b" }
                    }
                }
            }
        });
        assert_eq!(suggest_policies(&schema), json!({}));
    }

    #[test]
    fn identity_falls_back_to_all_non_anchor_strings_when_nothing_required() {
        let schema = json!({
            "cif_schema": {
                "items": {
                    "type": "array",
                    "element": {
                        "extId": { "type": "string", "anchor": "a" },
                        "intId": { "type": "string", "anchor": "b" },
                        "sku": { "type": "string" },
                        "lot": { "type": "string" }
                    }
                }
            }
        });
        let got = suggest_policies(&schema);
        let identity: Vec<&str> = got["items"]["identity"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(identity.contains(&"sku"));
        assert!(identity.contains(&"lot"));
    }

    #[test]
    fn nested_array_inside_element_gets_nested_policy() {
        let schema = json!({
            "cif_schema": {
                "groups": {
                    "type": "array",
                    "element": {
                        "extGid": { "type": "string", "anchor": "a" },
                        "intGid": { "type": "string", "anchor": "b" },
                        "name": { "type": "string", "required": true },
                        "lines": {
                            "type": "array",
                            "element": {
                                "extLid": { "type": "string", "anchor": "a" },
                                "intLid": { "type": "string", "anchor": "b" },
                                "sku": { "type": "string", "required": true }
                            }
                        }
                    }
                }
            }
        });
        let got = suggest_policies(&schema);
        let groups = &got["groups"];
        assert_eq!(groups["kind"], "set_by_key");
        let nested = &groups["nested"]["lines"];
        assert_eq!(nested["kind"], "set_by_key");
        assert_eq!(nested["a_anchor"], "extLid");
        assert_eq!(nested["b_anchor"], "intLid");
        let id: Vec<&str> = nested["identity"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(id, vec!["sku"]);
    }

    #[test]
    fn malformed_field_defs_are_skipped_not_panicked() {
        let schema = json!({
            "cif_schema": {
                "a": "not an object",
                "b": null,
                "c": 42,
                "d": { "type": "number" }
            }
        });
        let got = suggest_policies(&schema);
        assert_eq!(got, json!({ "d": { "kind": "additive" } }));
    }

    #[test]
    fn playground_po_sample_roundtrip() {
        // Mirrors the seeded sample in playground/web/app.js. This is the
        // end-to-end story: a user loads the sample schema, clicks Generate
        // policy, and gets a working starter policy for every field.
        let schema = json!({
            "cif_schema": {
                "po_status": { "type": "string", "required": true },
                "po_seq_number": { "type": "number", "required": true },
                "supplier_id": { "type": "string", "required": true },
                "price": { "type": "number", "required": true, "source_of_truth": "erp" },
                "qty_recv": { "type": "number", "required": true },
                "items": {
                    "type": "array",
                    "required": false,
                    "element": {
                        "externalId": { "type": "string", "anchor": "a" },
                        "internalId": { "type": "string", "anchor": "b" },
                        "sku": { "type": "string", "required": true },
                        "uom": { "type": "string", "required": true },
                        "qty": { "type": "number" }
                    }
                }
            }
        });
        let got = suggest_policies(&schema);
        assert_eq!(got["price"], json!({ "kind": "owned_by", "system": "erp" }));
        assert_eq!(got["po_seq_number"], json!({ "kind": "additive" }));
        assert_eq!(got["qty_recv"], json!({ "kind": "additive" }));
        assert_eq!(got["items"]["kind"], "set_by_key");
        assert_eq!(got["items"]["a_anchor"], "externalId");
        assert_eq!(got["items"]["b_anchor"], "internalId");
        assert_eq!(got["items"]["identity"], json!(["sku", "uom"]));
        // String-only fields without source_of_truth remain un-suggested.
        assert!(got.get("po_status").is_none());
        assert!(got.get("supplier_id").is_none());
    }
}
