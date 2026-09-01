//! Replays `spec/vectors/kernel-vectors.json` through the same
//! `drivers::wire` functions the generator (`examples/gen_kernel_vectors.rs`)
//! used to produce them.
//!
//! Not tautological: this catches kernel drift where someone changes Rust
//! behavior and forgets to regenerate the vector file. The TS and Go ports
//! read the same JSON and assert the same string equality at their own
//! wire boundary.

use diff_fusion::drivers::wire::{
    compare_json_impl, fuse_impl, merge_batch_impl, merge_field_impl, three_way_diff_impl,
    transform_to_cif_impl,
};
use serde_json::Value;

// spec/ lives at the repo root, one level above this crate (src/).
// Anchor on CARGO_MANIFEST_DIR so the path survives any invocation CWD.
const VECTORS_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../spec/vectors/kernel-vectors.json"));

fn vectors() -> Value {
    serde_json::from_str(VECTORS_JSON).expect("kernel-vectors.json must be valid JSON")
}

#[test]
fn three_way_diff_vectors_match() {
    let v = vectors();
    let cases = v["threeWayDiff"].as_array().expect("threeWayDiff must be an array");
    assert_eq!(cases.len(), 17, "threeWayDiff vector count changed — update this and the TS/Go counts");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let ancestor = case["ancestor"].as_str().unwrap();
        let a = case["a"].as_str().unwrap();
        let b = case["b"].as_str().unwrap();
        let expected = case["expected"].as_str().unwrap();
        let is_err = case["isErr"].as_bool().unwrap();

        let result = three_way_diff_impl(ancestor, a, b);
        match result {
            Ok(s) => {
                assert!(!is_err, "vector '{name}': expected an error but got Ok({s})");
                assert_eq!(s, expected, "vector '{name}': Ok output mismatch");
            }
            Err(e) => {
                assert!(is_err, "vector '{name}': expected Ok but got Err({e})");
                assert_eq!(e, expected, "vector '{name}': error message mismatch");
            }
        }
    }
}

#[test]
fn merge_field_vectors_match() {
    let v = vectors();
    let cases = v["mergeField"].as_array().expect("mergeField must be an array");
    assert_eq!(cases.len(), 29, "mergeField vector count changed — update this and the TS/Go counts");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let change = case["change"].as_str().unwrap();
        let policy_ref = case["policyRef"].as_str().unwrap();
        let ctx = case["ctx"].as_str().unwrap();
        let expected = case["expected"].as_str().unwrap();
        let is_err = case["isErr"].as_bool().unwrap();

        let result = merge_field_impl(change, policy_ref, ctx);
        match result {
            Ok(s) => {
                assert!(!is_err, "vector '{name}': expected an error but got Ok({s})");
                assert_eq!(s, expected, "vector '{name}': Ok output mismatch");
            }
            Err(e) => {
                assert!(is_err, "vector '{name}': expected Ok but got Err({e})");
                assert_eq!(e, expected, "vector '{name}': error message mismatch");
            }
        }
    }
}

#[test]
fn merge_batch_vectors_match() {
    let v = vectors();
    let cases = v["mergeBatch"].as_array().expect("mergeBatch must be an array");
    assert_eq!(cases.len(), 5, "mergeBatch vector count changed — update this and the TS/Go counts");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let changelog = case["changelog"].as_str().unwrap();
        let policy_doc = case["policyDoc"].as_str().unwrap();
        let ctx = case["ctx"].as_str().unwrap();
        let expected = case["expected"].as_str().unwrap();
        let is_err = case["isErr"].as_bool().unwrap();

        let result = merge_batch_impl(changelog, policy_doc, ctx);
        match result {
            Ok(s) => {
                assert!(!is_err, "vector '{name}': expected an error but got Ok({s})");
                assert_eq!(s, expected, "vector '{name}': Ok output mismatch");
            }
            Err(e) => {
                assert!(is_err, "vector '{name}': expected Ok but got Err({e})");
                assert_eq!(e, expected, "vector '{name}': error message mismatch");
            }
        }
    }
}

#[test]
fn compare_json_vectors_match() {
    let v = vectors();
    let cases = v["compareJson"].as_array().expect("compareJson must be an array");
    assert_eq!(cases.len(), 8, "compareJson vector count changed — update this and the TS/Go counts");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let a = case["a"].as_str().unwrap();
        let b = case["b"].as_str().unwrap();
        let expected = case["expected"].as_str().unwrap();
        let is_err = case["isErr"].as_bool().unwrap();

        let result = compare_json_impl(a, b);
        match result {
            Ok(s) => {
                assert!(!is_err, "vector '{name}': expected an error but got Ok({s})");
                assert_eq!(s, expected, "vector '{name}': Ok output mismatch");
            }
            Err(e) => {
                assert!(is_err, "vector '{name}': expected Ok but got Err({e})");
                assert_eq!(e, expected, "vector '{name}': error message mismatch");
            }
        }
    }
}

#[test]
fn fuse_vectors_match() {
    let v = vectors();
    let cases = v["fuse"].as_array().expect("fuse must be an array");
    assert_eq!(cases.len(), 7, "fuse vector count changed — update this and the TS/Go counts");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let ancestor = case["ancestor"].as_str().unwrap();
        let a = case["a"].as_str().unwrap();
        let b = case["b"].as_str().unwrap();
        let policy_doc = case["policyDoc"].as_str().unwrap();
        let ctx = case["ctx"].as_str().unwrap();
        let expected = case["expected"].as_str().unwrap();
        let is_err = case["isErr"].as_bool().unwrap();

        let result = fuse_impl(ancestor, a, b, policy_doc, ctx);
        match result {
            Ok(s) => {
                assert!(!is_err, "vector '{name}': expected an error but got Ok({s})");
                assert_eq!(s, expected, "vector '{name}': Ok output mismatch");
            }
            Err(e) => {
                assert!(is_err, "vector '{name}': expected Ok but got Err({e})");
                assert_eq!(e, expected, "vector '{name}': error message mismatch");
            }
        }
    }
}

#[test]
fn transform_to_cif_vectors_match() {
    let v = vectors();
    let cases = v["transformToCif"].as_array().expect("transformToCif must be an array");
    assert_eq!(cases.len(), 13, "transformToCif vector count changed — update this and the TS/Go counts");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let source = case["source"].as_str().unwrap();
        let schema = case["schema"].as_str().unwrap();
        let format_id = case["formatId"].as_str().unwrap();
        let expected = case["expected"].as_str().unwrap();
        let is_err = case["isErr"].as_bool().unwrap();

        let result = transform_to_cif_impl(source, schema, format_id);
        match result {
            Ok(s) => {
                assert!(!is_err, "vector '{name}': expected an error but got Ok({s})");
                assert_eq!(s, expected, "vector '{name}': Ok output mismatch");
            }
            Err(e) => {
                assert!(is_err, "vector '{name}': expected Ok but got Err({e})");
                assert_eq!(e, expected, "vector '{name}': error message mismatch");
            }
        }
    }
}
