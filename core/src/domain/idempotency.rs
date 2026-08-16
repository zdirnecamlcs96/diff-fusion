//! Idempotency keys for pushes to external systems.
//!
//! `idempotency_key(canonical_id, operation, payload)` is a pure function of
//! its inputs — no timestamps, no random IDs. When an adapter retries a push
//! after a transient failure, the external system sees the same key and
//! treats the second attempt as a no-op instead of a duplicate record.
//!
//! This is the specific fix for the legacy NetSuite item-receipt duplication
//! class of bug: duplicate records that appeared when webhooks fired
//! mid-cycle and the push was retried.
//!
//! # Determinism
//!
//! The payload is serialized via `serde_json::to_string`. `serde_json::Map`
//! is `BTreeMap`-backed by default (no `preserve_order` feature enabled), so
//! object keys are always sorted — two semantically-equal `Value`s hash
//! identically.
//!
//! Each field is length-prefixed before hashing so
//! `("a", "bc", ...)` and `("ab", "c", ...)` never collide.

use blake3::Hasher;
use serde_json::Value;

/// Compute a 32-byte idempotency key.
///
/// Stable under:
/// - retries (same inputs → same key)
/// - key reordering inside JSON objects (BTreeMap-backed `Map`)
///
/// Sensitive to:
/// - any change in `canonical_id`, `operation`, or the serialized payload
pub fn idempotency_key(canonical_id: &str, operation: &str, payload: &Value) -> [u8; 32] {
    let mut hasher = Hasher::new();
    write_field(&mut hasher, canonical_id.as_bytes());
    write_field(&mut hasher, operation.as_bytes());
    let payload_str = serde_json::to_string(payload)
        .expect("serde_json::Value always serializes to string");
    write_field(&mut hasher, payload_str.as_bytes());
    *hasher.finalize().as_bytes()
}

/// Hex-encoded form of [`idempotency_key`] — 64 lowercase hex chars.
pub fn idempotency_key_hex(canonical_id: &str, operation: &str, payload: &Value) -> String {
    hex_lower(&idempotency_key(canonical_id, operation, payload))
}

fn write_field(hasher: &mut Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    #[test]
    fn deterministic_for_same_inputs() {
        let p = json!({"qty": 5, "sku": "A1"});
        let k1 = idempotency_key("PO-1", "upsert", &p);
        let k2 = idempotency_key("PO-1", "upsert", &p);
        assert_eq!(k1, k2);
    }

    #[test]
    fn key_order_in_payload_does_not_matter() {
        // BTreeMap-backed Map guarantees serialization reorders keys.
        let p1 = json!({"a": 1, "b": 2});
        let p2 = json!({"b": 2, "a": 1});
        assert_eq!(
            idempotency_key("ID", "upsert", &p1),
            idempotency_key("ID", "upsert", &p2)
        );
    }

    #[test]
    fn different_canonical_id_changes_key() {
        let p = json!({"x": 1});
        assert_ne!(
            idempotency_key("PO-1", "upsert", &p),
            idempotency_key("PO-2", "upsert", &p)
        );
    }

    #[test]
    fn different_operation_changes_key() {
        let p = json!({"x": 1});
        assert_ne!(
            idempotency_key("PO-1", "upsert", &p),
            idempotency_key("PO-1", "delete", &p)
        );
    }

    #[test]
    fn different_payload_changes_key() {
        assert_ne!(
            idempotency_key("PO-1", "upsert", &json!({"x": 1})),
            idempotency_key("PO-1", "upsert", &json!({"x": 2}))
        );
    }

    #[test]
    fn length_prefix_prevents_boundary_collision() {
        // Without length-prefixing, ("a", "bc", ...) and ("ab", "c", ...)
        // could hash the same if fields were simply concatenated.
        let p = json!(null);
        assert_ne!(
            idempotency_key("a", "bc", &p),
            idempotency_key("ab", "c", &p)
        );
    }

    #[test]
    fn hex_form_is_64_chars_lowercase() {
        let h = idempotency_key_hex("PO-1", "upsert", &json!({}));
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    proptest! {
        #[test]
        fn same_inputs_same_key(id in "[A-Z0-9-]{1,16}", op in "[a-z]{3,10}", n in prop::num::i64::ANY) {
            let p = json!({"n": n});
            let k1 = idempotency_key(&id, &op, &p);
            let k2 = idempotency_key(&id, &op, &p);
            prop_assert_eq!(k1, k2);
        }

        #[test]
        fn different_id_or_op_different_key(
            id1 in "[A-Z]{2}",
            id2 in "[A-Z]{2}",
            op in "[a-z]{4}",
        ) {
            prop_assume!(id1 != id2);
            let p = json!({});
            prop_assert_ne!(
                idempotency_key(&id1, &op, &p),
                idempotency_key(&id2, &op, &p)
            );
        }
    }
}
