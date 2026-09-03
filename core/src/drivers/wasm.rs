//! wasm-bindgen kernel driver — JS/TS delivery of the shared wire layer.
//! Pure, sync, JSON-string in → JSON-string out. See reframe spec.
//! All semantics live in `super::wire`; this file only exports them.

use super::wire::{
    canonical_json_impl, compare_json_impl, fuse_impl, idempotency_key_hex_impl, merge_batch_impl,
    merge_field_impl, resolve_impl, three_way_diff_impl, transform_from_cif_impl,
    transform_to_cif_impl,
};
use wasm_bindgen::prelude::*;

macro_rules! export {
    ($wasm_name:ident, $impl_name:ident, ($($arg:ident),+)) => {
        #[wasm_bindgen]
        pub fn $wasm_name($($arg: &str),+) -> Result<String, JsError> {
            $impl_name($($arg),+).map_err(|e| JsError::new(&e))
        }
    };
}
export!(three_way_diff, three_way_diff_impl, (ancestor, a, b));
export!(merge_field, merge_field_impl, (change, policy_ref, ctx));
export!(merge_batch, merge_batch_impl, (changelog, policy_doc, ctx));
export!(fuse, fuse_impl, (ancestor, a, b, policy_doc, ctx));
export!(resolve, resolve_impl, (ancestor, changelog, policy_doc, ctx));
export!(canonical_json, canonical_json_impl, (doc));
export!(compare_json, compare_json_impl, (a, b));
export!(transform_to_cif, transform_to_cif_impl, (source, schema, format_id));
export!(transform_from_cif, transform_from_cif_impl, (cif, schema, format_id));
export!(
    idempotency_key_hex,
    idempotency_key_hex_impl,
    (canonical_id, operation, payload)
);
