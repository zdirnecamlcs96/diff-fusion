// Generates cross-language filesystem ancestor filename vectors.
//
// Run:
//   cargo run --example gen_filesystem_filenames > ../spec/vectors/filesystem-filenames.json
//
// The TS port (`ts/tests/unit/adapters/filesystemAncestor.test.ts`) reads the
// resulting JSON and asserts byte-identical path derivation. Each vector is
// `{ entityType, canonicalId, expectedRelPath }`, where `expectedRelPath` is
// the `<sanitized_entity>/<blake3_hash>.json` path relative to the store root,
// using forward slashes (the adapter joins with the platform separator, but
// the fixture encodes the canonical Rust form that tests decompose into its
// two parts).

use blake3;
use serde_json::{Value, json};

fn sanitize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}

fn hash_id(raw: &str) -> String {
    let digest = blake3::hash(raw.as_bytes());
    let bytes = digest.as_bytes();
    let mut out = String::with_capacity(32);
    for &b in bytes.iter().take(16) {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

fn vec_entry(entity_type: &str, canonical_id: &str) -> Value {
    let rel_path = format!(
        "{}/{}.json",
        sanitize(entity_type),
        hash_id(canonical_id),
    );
    json!({
        "entityType": entity_type,
        "canonicalId": canonical_id,
        "expectedRelPath": rel_path,
        "expectedEntityDir": sanitize(entity_type),
        "expectedFile": format!("{}.json", hash_id(canonical_id)),
    })
}

fn main() {
    let vectors = vec![
        // Plain ASCII identifiers — the happy path.
        vec_entry("purchase_order", "PO-1"),
        vec_entry("invoice", "INV-999"),
        vec_entry("item", "SKU-1"),
        // Entity type is preserved as-is when already safe.
        vec_entry("PurchaseOrder", "order-42"),
        // Unsafe chars in canonical_id are handled purely by the hash.
        vec_entry("thing", "customer/42:abc"),
        vec_entry("thing", "vendor:abc"),
        // Unicode canonical_id still hashes fine.
        vec_entry("customer", "üñîçødé"),
        // Entity type with slashes/spaces/punctuation -> all become '_'.
        vec_entry("weird type/with:chars", "id-1"),
        vec_entry("with spaces", "id-2"),
        // Empty entity type -> sanitized to "_".
        vec_entry("", "nonempty-id"),
        // Empty canonical_id is valid (blake3 of empty input is well-defined).
        vec_entry("entity", ""),
        // Long ids still collapse to 32-hex filename.
        vec_entry(
            "bulk",
            "this-is-a-very-long-canonical-identifier-that-would-be-unwieldy-on-disk",
        ),
    ];

    let out = Value::Array(vectors);
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
