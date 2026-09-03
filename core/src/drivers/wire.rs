//! Shared kernel wire layer — target-agnostic.
//! Pure, sync, JSON-string in → JSON-string out. See reframe spec.

use crate::application::orchestrator::apply_resolution;
use crate::application::policy::declaration::MergePolicyRef;
use crate::application::policy::{
    resolve, ConflictClass, MergeContext, PolicyDocument, UnresolvedConflict,
};
use crate::application::transform::{transform_from_cif, transform_to_cif};
use crate::domain::compare::compare_json;
use crate::domain::diff::{three_way_diff as diff3, ChangeSource, Changelog, FieldChange};
use serde_json::Value;

fn parse(label: &str, s: &str) -> Result<Value, String> {
    serde_json::from_str(s).map_err(|e| format!("invalid {label} JSON: {e}"))
}

/// Wire shape for a `FieldChange` at the WASM boundary.
///
/// Wire contract: an ABSENT `new_from_a`/`new_from_b` key means that side
/// didn't touch the field (domain `None`); a PRESENT key — including
/// `null` — means it changed, possibly to `null` (domain `Some(value)`,
/// where `value` may itself be `Value::Null`). This is why the fields are
/// `Option<Option<Value>>` with `skip_serializing_if`/a custom deserializer
/// instead of plain `Option<Value>`, which serde would otherwise collapse
/// `Some(Value::Null)` and `None` into the same wire `null`.
#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
struct WireFieldChange {
    path: String,
    old_value: Value,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    new_from_a: Option<Option<Value>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    new_from_b: Option<Option<Value>>,
    source: ChangeSource,
}

fn double_option<'de, D>(d: D) -> Result<Option<Option<Value>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(d).map(Some)
}

#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
struct WireChangelog {
    changes: Vec<WireFieldChange>,
}

/// Schema for [`WireChangelog`] — the real `three_way_diff` wire boundary
/// (absent key = unchanged, `null` = cleared). `WireChangelog` is module-
/// private, so this helper is the least-invasive way to reach it from
/// `examples/gen_schema.rs`: no visibility changes to the wire types
/// themselves, just one function exposed only under `schema-gen`.
#[cfg(feature = "schema-gen")]
pub fn wire_changelog_schema() -> schemars::schema::RootSchema {
    schemars::schema_for!(WireChangelog)
}

impl From<FieldChange> for WireFieldChange {
    fn from(c: FieldChange) -> Self {
        WireFieldChange {
            path: c.path,
            old_value: c.old_value,
            new_from_a: c.new_from_a.map(Some),
            new_from_b: c.new_from_b.map(Some),
            source: c.source,
        }
    }
}

// NOT `.flatten()`: that collapses `Some(None)` (present, explicit null —
// "changed to null") into `None` ("absent — unchanged"), which is exactly
// the distinction this wire format exists to preserve.
fn unwrap_presence(v: Option<Option<Value>>) -> Option<Value> {
    match v {
        None => None,
        Some(inner) => Some(inner.unwrap_or(Value::Null)),
    }
}

impl From<WireFieldChange> for FieldChange {
    fn from(w: WireFieldChange) -> Self {
        FieldChange {
            path: w.path,
            old_value: w.old_value,
            new_from_a: unwrap_presence(w.new_from_a),
            new_from_b: unwrap_presence(w.new_from_b),
            source: w.source,
        }
    }
}

impl From<Changelog> for WireChangelog {
    fn from(log: Changelog) -> Self {
        WireChangelog {
            changes: log.changes.into_iter().map(WireFieldChange::from).collect(),
        }
    }
}

pub fn three_way_diff_impl(anc: &str, a: &str, b: &str) -> Result<String, String> {
    let log = diff3(&parse("ancestor", anc)?, &parse("a", a)?, &parse("b", b)?);
    let wire: WireChangelog = log.into();
    serde_json::to_string(&wire).map_err(|e| e.to_string())
}

/// Shared source/new_from_a/new_from_b consistency check for a single wire
/// change, used by both [`merge_field_impl`] and [`merge_batch_impl`]. A
/// change whose `source` claims a side moved must carry that side's
/// (possibly present-null) new value — an absent key there means the wire
/// payload lied about who touched the field.
fn check_change_consistency(wire: &WireFieldChange) -> Result<(), String> {
    let (needs_a, needs_b, source_label) = match wire.source {
        ChangeSource::A => (true, false, "a"),
        ChangeSource::B => (false, true, "b"),
        ChangeSource::Both => (true, true, "both"),
    };
    if needs_a && wire.new_from_a.is_none() {
        return Err(format!(
            "inconsistent change: source '{source_label}' but new_from_a is absent"
        ));
    }
    if needs_b && wire.new_from_b.is_none() {
        return Err(format!(
            "inconsistent change: source '{source_label}' but new_from_b is absent"
        ));
    }
    Ok(())
}

pub fn merge_field_impl(change: &str, policy_ref: &str, ctx: &str) -> Result<String, String> {
    let wire: WireFieldChange =
        serde_json::from_str(change).map_err(|e| format!("invalid change JSON: {e}"))?;
    check_change_consistency(&wire)?;
    let change: FieldChange = wire.into();
    let decl: MergePolicyRef =
        serde_json::from_str(policy_ref).map_err(|e| format!("invalid policy_ref JSON: {e}"))?;
    #[derive(serde::Deserialize)]
    struct Ctx {
        system_a: String,
        system_b: String,
    }
    let c: Ctx = serde_json::from_str(ctx).map_err(|e| format!("invalid ctx JSON: {e}"))?;
    let policy = decl.build();
    let out = policy.merge(&change, &MergeContext::new(c.system_a, c.system_b));
    let json = match out {
        crate::application::policy::MergeOutcome::Resolved(v) => {
            serde_json::json!({ "kind": "Resolved", "value": v })
        }
        crate::application::policy::MergeOutcome::Conflict { reason } => {
            serde_json::json!({ "kind": "Conflict", "reason": reason })
        }
    };
    Ok(json.to_string())
}

/// Wire-encode a `Resolution`'s conflicts, shared by [`merge_batch_impl`] and
/// [`fuse_impl`] so both emit byte-identical `{path, class, reason, change}`
/// conflict entries.
fn conflicts_to_wire(conflicts: Vec<UnresolvedConflict>) -> Vec<Value> {
    conflicts
        .into_iter()
        .map(|c| {
            let class = match c.class {
                ConflictClass::NoPolicy => "no_policy",
                ConflictClass::PolicyConflict => "policy_conflict",
                ConflictClass::InvariantViolation => "invariant_violation",
            };
            let wire_change: WireFieldChange = c.change.into();
            serde_json::json!({
                "path": c.path,
                "class": class,
                "reason": c.reason,
                "change": wire_change,
            })
        })
        .collect()
}

/// `merge_batch` wire boundary — runs the existing application-layer
/// [`resolve`] over a whole changelog against a [`PolicyDocument`], instead
/// of one field at a time like [`merge_field_impl`]. `changelog` is the same
/// `{"changes": [...]}` shape [`three_way_diff_impl`] emits, so its output
/// can be piped straight in.
pub fn merge_batch_impl(changelog: &str, policy_doc: &str, ctx: &str) -> Result<String, String> {
    let wire_log: WireChangelog = serde_json::from_str(changelog)
        .map_err(|e| format!("invalid changelog JSON: {e}"))?;
    for (i, w) in wire_log.changes.iter().enumerate() {
        check_change_consistency(w).map_err(|e| format!("change[{i}]: {e}"))?;
    }
    let doc: PolicyDocument =
        serde_json::from_str(policy_doc).map_err(|e| format!("invalid policy_doc JSON: {e}"))?;
    #[derive(serde::Deserialize)]
    struct Ctx {
        system_a: String,
        system_b: String,
    }
    let c: Ctx = serde_json::from_str(ctx).map_err(|e| format!("invalid ctx JSON: {e}"))?;

    let log = Changelog {
        changes: wire_log.changes.into_iter().map(FieldChange::from).collect(),
    };
    let policies = doc.build();
    let merge_ctx = MergeContext::new(c.system_a, c.system_b);
    let resolution = resolve(&log, &policies, &merge_ctx);

    let resolved: Vec<Value> = resolution
        .resolved
        .into_iter()
        .map(|(path, value)| serde_json::json!({ "path": path, "value": value }))
        .collect();
    let conflicts = conflicts_to_wire(resolution.conflicts);

    let out = serde_json::json!({ "resolved": resolved, "conflicts": conflicts });
    Ok(out.to_string())
}

/// `resolve` wire boundary — runs the existing application-layer [`resolve`]
/// over an already-computed `changelog` (the same `{"changes": [...]}` shape
/// [`three_way_diff_impl`] emits) against a [`PolicyDocument`], applies the
/// resolution onto `ancestor`, and returns the merged document. This is the
/// second half of [`fuse_impl`] — `three_way_diff` → `resolve` → apply the
/// resolution onto the ancestor — split out so a caller that already has a
/// changelog (e.g. from [`three_way_diff_impl`]) doesn't have to re-diff.
pub fn resolve_impl(
    ancestor: &str,
    changelog: &str,
    policy_doc: &str,
    ctx: &str,
) -> Result<String, String> {
    let ancestor_v = parse("ancestor", ancestor)?;
    let wire_log: WireChangelog = serde_json::from_str(changelog)
        .map_err(|e| format!("invalid changelog JSON: {e}"))?;
    for (i, w) in wire_log.changes.iter().enumerate() {
        check_change_consistency(w).map_err(|e| format!("change[{i}]: {e}"))?;
    }
    let doc: PolicyDocument =
        serde_json::from_str(policy_doc).map_err(|e| format!("invalid policy_doc JSON: {e}"))?;
    #[derive(serde::Deserialize)]
    struct Ctx {
        system_a: String,
        system_b: String,
    }
    let c: Ctx = serde_json::from_str(ctx).map_err(|e| format!("invalid ctx JSON: {e}"))?;

    let log = Changelog {
        changes: wire_log.changes.into_iter().map(FieldChange::from).collect(),
    };
    let policies = doc.build();
    let merge_ctx = MergeContext::new(c.system_a, c.system_b);
    let resolution = resolve(&log, &policies, &merge_ctx);
    let merged = apply_resolution(&ancestor_v, &resolution);
    let conflicts = conflicts_to_wire(resolution.conflicts);

    let out = serde_json::json!({ "value": merged, "conflicts": conflicts });
    Ok(out.to_string())
}

/// `fuse` wire boundary — the solution-shaped kernel entry composing the
/// three pieces most callers otherwise wire together by hand:
/// `three_way_diff` → `resolve` → apply the resolution onto the ancestor.
/// Unlike [`merge_batch_impl`] (which takes an already-computed changelog and
/// emits `{resolved, conflicts}` for the caller to apply itself), `fuse_impl`
/// takes the three raw documents and returns the merged document directly —
/// resolved values are folded into `value`, so there is no separate
/// `resolved` list in the output.
pub fn fuse_impl(
    ancestor: &str,
    a: &str,
    b: &str,
    policy_doc: &str,
    ctx: &str,
) -> Result<String, String> {
    let changelog = three_way_diff_impl(ancestor, a, b)?;
    resolve_impl(ancestor, &changelog, policy_doc, ctx)
}

/// `compare_json` wire boundary. serde serializes the domain
/// `Vec<(String, (Value, Value))>` as `[[path, [old, new]], ...]` — byte-for-byte
/// the shape TS's `CompareChange[]` already is, so no translation layer.
pub fn compare_json_impl(a: &str, b: &str) -> Result<String, String> {
    let diffs = compare_json(&parse("a", a)?, &parse("b", b)?);
    serde_json::to_string(&diffs).map_err(|e| e.to_string())
}

pub fn transform_to_cif_impl(source: &str, schema: &str, format_id: &str) -> Result<String, String> {
    let cif = transform_to_cif(&parse("source", source)?, &parse("schema", schema)?, format_id)
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&cif).map_err(|e| e.to_string())
}

pub fn transform_from_cif_impl(cif: &str, schema: &str, format_id: &str) -> Result<String, String> {
    let source = transform_from_cif(&parse("cif", cif)?, &parse("schema", schema)?, format_id)
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&source).map_err(|e| e.to_string())
}

pub fn canonical_json_impl(doc: &str) -> Result<String, String> {
    serde_json::to_string(&parse("doc", doc)?).map_err(|e| e.to_string())
}

pub fn idempotency_key_hex_impl(id: &str, op: &str, payload: &str) -> Result<String, String> {
    Ok(crate::domain::idempotency::idempotency_key_hex(
        id,
        op,
        &parse("payload", payload)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_roundtrip() {
        let out = three_way_diff_impl(
            r#"{"qty":1}"#, r#"{"qty":2}"#, r#"{"qty":1}"#).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["changes"][0]["source"], "a");
    }

    #[test]
    fn merge_field_additive_resolves() {
        // Real FieldChange serde fields (three_way.rs:46-51): path, old_value,
        // new_from_a, new_from_b, source.
        let change = r#"{"path":"qty","old_value":1,"new_from_a":3,"new_from_b":4,"source":"both"}"#;
        let out = merge_field_impl(change, r#"{"kind":"additive"}"#,
            r#"{"system_a":"x","system_b":"y"}"#).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["kind"], "Resolved");
        // Additive::merge always produces Number::from_f64 (N::Float), so
        // 1 + (3-1) + (4-1) = 6 serializes/reparses as 6.0, not 6.
        assert_eq!(v["value"], 6.0);
    }

    #[test]
    fn compare_json_emits_sorted_paths() {
        let out = compare_json_impl(r#"{"z":1,"a":1,"m":1}"#, r#"{"z":2,"a":2,"m":2}"#).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let paths: Vec<&str> = v.as_array().unwrap().iter().map(|e| e[0].as_str().unwrap()).collect();
        assert_eq!(paths, vec!["a", "m", "z"]);
    }

    #[test]
    fn transform_to_cif_maps_source_path() {
        let schema = r#"{"cif_schema":{"name":{"type":"string","required":true}},
            "transformations":{"f":{"name":{"source_path":"n","type":"string"}}}}"#;
        let out = transform_to_cif_impl(r#"{"n":"Widget"}"#, schema, "f").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["name"], "Widget");
    }

    #[test]
    fn canonical_sorts_keys() {
        assert_eq!(canonical_json_impl(r#"{"b":1,"a":2}"#).unwrap(), r#"{"a":2,"b":1}"#);
    }

    #[test]
    fn idempotency_matches_domain() {
        let hex = idempotency_key_hex_impl("id1", "upsert", r#"{"x":1}"#).unwrap();
        let expected = crate::domain::idempotency::idempotency_key_hex(
            "id1", "upsert", &serde_json::from_str(r#"{"x":1}"#).unwrap());
        assert_eq!(hex, expected);
    }

    #[test]
    fn bad_json_is_err() {
        assert!(three_way_diff_impl("{", "{}", "{}").is_err());
    }

    #[test]
    fn inconsistent_source_a_with_absent_new_from_a_is_err() {
        // Key is ABSENT (not present-null) — that's the new inconsistent case.
        let change = r#"{"path":"s","old_value":"draft","new_from_b":"open","source":"a"}"#;
        let policy_ref = r#"{"kind":"state_machine","transitions":[{"from":"draft","to":"open"}]}"#;
        let out = merge_field_impl(change, policy_ref, r#"{"system_a":"x","system_b":"y"}"#);
        assert!(out.is_err());
    }

    #[test]
    fn inconsistent_source_b_with_absent_new_from_b_is_err() {
        // Key is ABSENT (not present-null) — mirrors the source "a" case above.
        let change = r#"{"path":"s","old_value":"draft","new_from_a":"open","source":"b"}"#;
        let policy_ref = r#"{"kind":"state_machine","transitions":[{"from":"draft","to":"open"}]}"#;
        let out = merge_field_impl(change, policy_ref, r#"{"system_a":"x","system_b":"y"}"#);
        assert!(out.is_err());
    }

    #[test]
    fn inconsistent_source_both_with_absent_new_from_b_is_err() {
        let change = r#"{"path":"qty","old_value":1,"new_from_a":3,"source":"both"}"#;
        let out = merge_field_impl(change, r#"{"kind":"additive"}"#,
            r#"{"system_a":"x","system_b":"y"}"#);
        assert!(out.is_err());
    }

    #[test]
    fn diff_null_clear_is_present_null_and_untouched_side_is_omitted() {
        // A cleared "status" to null; B left it untouched. The wire output
        // must carry `"new_from_a":null` (present) and OMIT `new_from_b`
        // entirely (absent = unchanged).
        let out = three_way_diff_impl(
            r#"{"status":"draft"}"#,
            r#"{"status":null}"#,
            r#"{"status":"draft"}"#,
        )
        .unwrap();
        assert!(out.contains(r#""new_from_a":null"#), "output: {out}");
        assert!(!out.contains("new_from_b"), "output: {out}");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["changes"][0]["source"], "a");
    }

    #[test]
    fn merge_field_accepts_changed_to_null() {
        // source "a", new_from_a present-null (a legitimate clear), policy
        // owned_by system "x" matching ctx.system_a — resolves to null, no error.
        let change = r#"{"path":"status","old_value":"draft","new_from_a":null,"source":"a"}"#;
        let out = merge_field_impl(
            change,
            r#"{"kind":"owned_by","system":"x"}"#,
            r#"{"system_a":"x","system_b":"y"}"#,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["kind"], "Resolved");
        assert_eq!(v["value"], serde_json::Value::Null);
    }

    #[test]
    fn merge_batch_all_resolved() {
        let changelog = r#"{"changes":[
            {"path":"price","old_value":10,"new_from_a":15,"source":"a"},
            {"path":"qty","old_value":5,"new_from_a":6,"new_from_b":7,"source":"both"}
        ]}"#;
        let policy_doc = r#"{"fields":{
            "price":{"kind":"owned_by","system":"x"},
            "qty":{"kind":"additive"}
        }}"#;
        let out = merge_batch_impl(changelog, policy_doc, r#"{"system_a":"x","system_b":"y"}"#).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["conflicts"].as_array().unwrap().is_empty());
        let resolved = v["resolved"].as_array().unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0]["path"], "price");
        assert_eq!(resolved[0]["value"], 15);
        assert_eq!(resolved[1]["path"], "qty");
        assert_eq!(resolved[1]["value"], 8.0);
    }

    #[test]
    fn merge_batch_policy_conflict() {
        let changelog = r#"{"changes":[
            {"path":"status","old_value":"closed","new_from_a":"draft","source":"a"}
        ]}"#;
        let policy_doc = r#"{"fields":{
            "status":{"kind":"state_machine","transitions":[{"from":"draft","to":"open"}]}
        }}"#;
        let out = merge_batch_impl(changelog, policy_doc, r#"{"system_a":"x","system_b":"y"}"#).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["resolved"].as_array().unwrap().is_empty());
        let conflicts = v["conflicts"].as_array().unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0]["path"], "status");
        assert_eq!(conflicts[0]["class"], "policy_conflict");
    }

    #[test]
    fn merge_batch_no_policy_conflict() {
        let changelog = r#"{"changes":[
            {"path":"mystery","old_value":1,"new_from_a":2,"source":"a"}
        ]}"#;
        let out = merge_batch_impl(changelog, r#"{"fields":{}}"#, r#"{"system_a":"x","system_b":"y"}"#).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["resolved"].as_array().unwrap().is_empty());
        let conflicts = v["conflicts"].as_array().unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0]["path"], "mystery");
        assert_eq!(conflicts[0]["class"], "no_policy");
    }

    #[test]
    fn merge_batch_mixed_resolved_and_conflict() {
        let changelog = r#"{"changes":[
            {"path":"price","old_value":10,"new_from_a":15,"source":"a"},
            {"path":"mystery","old_value":1,"new_from_a":2,"source":"a"}
        ]}"#;
        let policy_doc = r#"{"fields":{"price":{"kind":"owned_by","system":"x"}}}"#;
        let out = merge_batch_impl(changelog, policy_doc, r#"{"system_a":"x","system_b":"y"}"#).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let resolved = v["resolved"].as_array().unwrap();
        let conflicts = v["conflicts"].as_array().unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0]["path"], "price");
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0]["path"], "mystery");
        assert_eq!(conflicts[0]["class"], "no_policy");
    }

    #[test]
    fn merge_batch_inconsistent_change_errors_with_index() {
        // Second change (index 1) claims source "a" but new_from_a is absent.
        let changelog = r#"{"changes":[
            {"path":"price","old_value":10,"new_from_a":15,"source":"a"},
            {"path":"status","old_value":"draft","new_from_b":"open","source":"a"}
        ]}"#;
        let policy_doc = r#"{"fields":{"price":{"kind":"owned_by","system":"x"}}}"#;
        let out = merge_batch_impl(changelog, policy_doc, r#"{"system_a":"x","system_b":"y"}"#);
        let err = out.unwrap_err();
        assert!(err.contains("change[1]"), "error: {err}");
        assert!(err.contains("inconsistent change"), "error: {err}");
    }

    #[test]
    fn merge_batch_absent_vs_null_round_trips_in_conflict_change() {
        // No policy declared for "status" — echoes back as a no_policy
        // conflict. new_from_a is a legitimate present-null clear;
        // new_from_b is untouched (absent), and must stay absent in the
        // echoed change, not collapse into `null`.
        let changelog = r#"{"changes":[
            {"path":"status","old_value":"draft","new_from_a":null,"source":"a"}
        ]}"#;
        let out = merge_batch_impl(changelog, r#"{"fields":{}}"#, r#"{"system_a":"x","system_b":"y"}"#).unwrap();
        assert!(out.contains(r#""new_from_a":null"#), "output: {out}");
        assert!(!out.contains("new_from_b"), "output: {out}");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["conflicts"][0]["change"]["new_from_a"], serde_json::Value::Null);
        assert!(v["conflicts"][0]["change"].get("new_from_b").is_none());
    }

    #[test]
    fn fuse_all_resolved_nested_dotted_path() {
        let out = fuse_impl(
            r#"{"pricing":{"amount":10,"currency":"usd"}}"#,
            r#"{"pricing":{"amount":12,"currency":"usd"}}"#,
            r#"{"pricing":{"amount":10,"currency":"usd"}}"#,
            r#"{"fields":{"pricing.amount":{"kind":"owned_by","system":"x"}}}"#,
            r#"{"system_a":"x","system_b":"y"}"#,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["conflicts"].as_array().unwrap().is_empty());
        assert_eq!(
            v["value"],
            serde_json::json!({"pricing":{"amount":12,"currency":"usd"}})
        );
    }

    #[test]
    fn fuse_mixed_resolved_and_conflict() {
        let out = fuse_impl(
            r#"{"price":10,"mystery":1}"#,
            r#"{"price":15,"mystery":2}"#,
            r#"{"price":10,"mystery":1}"#,
            r#"{"fields":{"price":{"kind":"owned_by","system":"x"}}}"#,
            r#"{"system_a":"x","system_b":"y"}"#,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        // Resolved overlay applied: price wins; mystery has no policy, so it
        // stays at the ancestor's value in `value` and is escalated instead.
        assert_eq!(v["value"], serde_json::json!({"price":15,"mystery":1}));
        let conflicts = v["conflicts"].as_array().unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0]["path"], "mystery");
        assert_eq!(conflicts[0]["class"], "no_policy");
    }

    #[test]
    fn fuse_no_policy_conflict() {
        let out = fuse_impl(
            r#"{"mystery":1}"#,
            r#"{"mystery":2}"#,
            r#"{"mystery":1}"#,
            r#"{"fields":{}}"#,
            r#"{"system_a":"x","system_b":"y"}"#,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["value"], serde_json::json!({"mystery":1}));
        let conflicts = v["conflicts"].as_array().unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0]["path"], "mystery");
        assert_eq!(conflicts[0]["class"], "no_policy");
    }

    #[test]
    fn fuse_set_by_key_lands_merged_array_in_value() {
        // Same SetByKey policy shape as the merge_field "additions-both-sides-
        // merge" vector: both sides add a distinct element, minimal
        // constructor defaults on_both_changed to Escalate (irrelevant here
        // since no element was changed by both sides).
        let out = fuse_impl(
            r#"{"items":[{"sku":"x","q":1}]}"#,
            r#"{"items":[{"sku":"x","q":1},{"sku":"y","q":2}]}"#,
            r#"{"items":[{"sku":"x","q":1},{"sku":"z","q":3}]}"#,
            r#"{"fields":{"items":{"kind":"set_by_key","identity":["sku"],"a_anchor":"sku","b_anchor":"sku"}}}"#,
            r#"{"system_a":"x","system_b":"y"}"#,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["conflicts"].as_array().unwrap().is_empty());
        assert_eq!(
            v["value"]["items"],
            serde_json::json!([{"sku":"x","q":1},{"sku":"y","q":2},{"sku":"z","q":3}])
        );
    }

    #[test]
    fn fuse_cleared_field_is_literal_null_in_value() {
        let out = fuse_impl(
            r#"{"status":"draft"}"#,
            r#"{"status":null}"#,
            r#"{"status":"draft"}"#,
            r#"{"fields":{"status":{"kind":"owned_by","system":"x"}}}"#,
            r#"{"system_a":"x","system_b":"y"}"#,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["conflicts"].as_array().unwrap().is_empty());
        assert!(v["value"].as_object().unwrap().contains_key("status"));
        assert_eq!(v["value"]["status"], serde_json::Value::Null);
    }

    #[test]
    fn fuse_invalid_input_errors() {
        let policy_doc = r#"{"fields":{}}"#;
        let ctx = r#"{"system_a":"x","system_b":"y"}"#;

        let err = fuse_impl("{bad", "{}", "{}", policy_doc, ctx).unwrap_err();
        assert!(err.starts_with("invalid ancestor JSON:"), "error: {err}");

        let err = fuse_impl("{}", "{bad", "{}", policy_doc, ctx).unwrap_err();
        assert!(err.starts_with("invalid a JSON:"), "error: {err}");

        let err = fuse_impl("{}", "{}", "{bad", policy_doc, ctx).unwrap_err();
        assert!(err.starts_with("invalid b JSON:"), "error: {err}");

        let err = fuse_impl("{}", "{}", "{}", "{bad", ctx).unwrap_err();
        assert!(err.starts_with("invalid policy_doc JSON:"), "error: {err}");

        let err = fuse_impl("{}", "{}", "{}", policy_doc, "{bad").unwrap_err();
        assert!(err.starts_with("invalid ctx JSON:"), "error: {err}");
    }

    #[test]
    fn fuse_dotted_key_lands_in_real_key_not_phantom_branch() {
        // Regression for the phantom-branch bug: a JSON key containing a
        // literal '.' (e.g. a timestamp-derived SKU) must not be split by
        // set_at_path into a fake nested branch. compare_json now escapes
        // each key segment before joining; set_at_path (via split_path)
        // undoes the escaping to land back on the real key.
        use crate::domain::json_path::escape_segment;

        let key = "key_2025-01-01T00:00:00.000Z_demo";
        let make_doc = |name: &str| format!(r#"{{"items":{{"{key}":{{"name":"{name}"}}}}}}"#);
        let escaped_path = format!("items.{}.name", escape_segment(key));

        let mut fields = serde_json::Map::new();
        fields.insert(
            escaped_path.clone(),
            serde_json::json!({"kind": "owned_by", "system": "x"}),
        );
        let policy_doc = serde_json::json!({ "fields": fields }).to_string();

        let out = fuse_impl(
            &make_doc("old"),
            &make_doc("new"),
            &make_doc("old"),
            &policy_doc,
            r#"{"system_a":"x","system_b":"y"}"#,
        )
        .unwrap();

        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["conflicts"].as_array().unwrap().is_empty(), "output: {out}");
        let expected: serde_json::Value = serde_json::from_str(&make_doc("new")).unwrap();
        assert_eq!(v["value"], expected, "output: {out}");
        // No phantom branch created by a naive split on '.'.
        assert!(v["value"]["items"]
            .as_object()
            .unwrap()
            .get("key_2025-01-01T00:00:00")
            .is_none());
    }

    #[test]
    fn three_way_diff_to_merge_batch_resolves_via_escaped_dotted_key_path() {
        // The wire changelog from three_way_diff_impl must carry the escaped
        // path (dot inside the key literally escaped as '\.'), and
        // merge_batch_impl's exact-string PolicyMap lookup must match it
        // when the policy doc declares the same escaped path.
        let anc = r#"{"a.b":1}"#;
        let a = r#"{"a.b":2}"#;
        let b = r#"{"a.b":1}"#;

        let changelog = three_way_diff_impl(anc, a, b).unwrap();
        assert!(changelog.contains(r#""path":"a\\.b""#), "changelog: {changelog}");

        let escaped_path = "a\\.b"; // one literal backslash: escape_segment("a.b")
        let mut fields = serde_json::Map::new();
        fields.insert(
            escaped_path.to_string(),
            serde_json::json!({"kind": "owned_by", "system": "x"}),
        );
        let policy_doc = serde_json::json!({ "fields": fields }).to_string();

        let out = merge_batch_impl(&changelog, &policy_doc, r#"{"system_a":"x","system_b":"y"}"#).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["conflicts"].as_array().unwrap().is_empty(), "output: {out}");
        assert_eq!(v["resolved"][0]["path"], escaped_path);
        assert_eq!(v["resolved"][0]["value"], 2);
    }

    #[test]
    fn transform_from_cif_maps_source_path() {
        let schema = r#"{"cif_schema":{"name":{"type":"string","required":true}},
            "transformations":{"f":{"name":{"source_path":"n","type":"string"}}}}"#;
        let out = transform_from_cif_impl(r#"{"name":"Widget"}"#, schema, "f").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["n"], "Widget");
    }

    #[test]
    fn transform_round_trip_nested_children_and_array_element() {
        let source = r#"{"lineItems":[{"extId":"A-1","sku":"SKU-X","quantity":3},{"extId":"A-2","sku":"SKU-Y","quantity":5}],"vendor":{"companyName":"Acme","contact":{"email":"a@acme.io"}}}"#;
        let schema = r#"{"cif_schema":{
            "items":{"type":"array","required":true,"element":{"externalId":{"type":"string"},"sku":{"type":"string"},"qty":{"type":"number"}}},
            "supplier":{"type":"object","required":true,"children":{"name":{"type":"string"},"email":{"type":"string"}}}
        },"transformations":{"erp":{
            "items":{"source_path":"lineItems","type":"array","element":{"externalId":{"source_path":"extId","type":"string"},"sku":{"source_path":"sku","type":"string"},"qty":{"source_path":"quantity","type":"number"}}},
            "supplier":{"source_path":"vendor","type":"object","children":{"name":{"source_path":"companyName","type":"string"},"email":{"source_path":"contact.email","type":"string"}}}
        }}}"#;

        let cif = transform_to_cif_impl(source, schema, "erp").unwrap();
        let round_tripped = transform_from_cif_impl(&cif, schema, "erp").unwrap();
        let v: serde_json::Value = serde_json::from_str(&round_tripped).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "lineItems": [
                    {"extId": "A-1", "sku": "SKU-X", "quantity": 3},
                    {"extId": "A-2", "sku": "SKU-Y", "quantity": 5}
                ],
                "vendor": {"companyName": "Acme", "contact": {"email": "a@acme.io"}}
            })
        );
    }

    #[test]
    fn transform_from_cif_dot_at_element_level_round_trips_scalar() {
        let schema = r#"{"cif_schema":{"meta":{"type":"object","children":{"raw":{"type":"string"}}}},
            "transformations":{"f":{"meta":{"source_path":"tag","type":"object","children":{"raw":{"source_path":".","type":"string"}}}}}}"#;
        let source = r#"{"tag":"urgent"}"#;

        let cif = transform_to_cif_impl(source, schema, "f").unwrap();
        let out = transform_from_cif_impl(&cif, schema, "f").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v, serde_json::json!({"tag": "urgent"}));
    }

    #[test]
    fn transform_from_cif_dotted_real_key_writes_escaped_source_path() {
        let schema = r#"{"cif_schema":{"name":{"type":"string"}},
            "transformations":{"f":{"name":{"source_path":"a\\.b","type":"string"}}}}"#;
        let out = transform_from_cif_impl(r#"{"name":"Widget"}"#, schema, "f").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v, serde_json::json!({"a.b": "Widget"}));
    }

    #[test]
    fn transform_from_cif_missing_cif_field_is_skipped_not_error() {
        let schema = r#"{"cif_schema":{"name":{"type":"string"},"price":{"type":"number"}},
            "transformations":{"f":{"name":{"source_path":"n","type":"string"},"price":{"source_path":"p","type":"number"}}}}"#;
        let out = transform_from_cif_impl(r#"{"name":"Widget"}"#, schema, "f").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v, serde_json::json!({"n": "Widget"}));
    }

    #[test]
    fn transform_from_cif_unknown_format_errors() {
        let schema = r#"{"cif_schema":{},"transformations":{"f":{}}}"#;
        let out = transform_from_cif_impl(r#"{}"#, schema, "nonexistent_format");
        assert!(out.is_err());
    }

    #[test]
    fn fuse_equals_three_way_diff_then_resolve() {
        let ancestor = r#"{"pricing":{"amount":10,"currency":"usd"}}"#;
        let a = r#"{"pricing":{"amount":12,"currency":"usd"}}"#;
        let b = r#"{"pricing":{"amount":10,"currency":"usd"}}"#;
        let policy_doc = r#"{"fields":{"pricing.amount":{"kind":"owned_by","system":"x"}}}"#;
        let ctx = r#"{"system_a":"x","system_b":"y"}"#;

        let fused = fuse_impl(ancestor, a, b, policy_doc, ctx).unwrap();
        let changelog = three_way_diff_impl(ancestor, a, b).unwrap();
        let resolved = resolve_impl(ancestor, &changelog, policy_doc, ctx).unwrap();
        assert_eq!(fused, resolved);
    }
}
