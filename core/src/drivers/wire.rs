//! Shared kernel wire layer — target-agnostic.
//! Pure, sync, JSON-string in → JSON-string out. See reframe spec.

use crate::application::policy::declaration::MergePolicyRef;
use crate::application::policy::MergeContext;
use crate::application::transform::transform_to_cif;
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

#[derive(serde::Serialize)]
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

pub fn merge_field_impl(change: &str, policy_ref: &str, ctx: &str) -> Result<String, String> {
    let wire: WireFieldChange =
        serde_json::from_str(change).map_err(|e| format!("invalid change JSON: {e}"))?;
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
}
