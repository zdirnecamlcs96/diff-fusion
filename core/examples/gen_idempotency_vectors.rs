// Generates cross-language idempotency key test vectors.
//
// Run:
//   cargo run --example gen_idempotency_vectors > ../spec/vectors/idempotency-vectors.json
//
// The TS port (`ts/tests/unit/domain/idempotency.test.ts`) reads the resulting
// JSON and asserts byte-identical output. Each vector is
// `{ canonicalId, operation, payload, canonicalPayloadJson, expectedHex }`.
// `payload` is the structured value; `canonicalPayloadJson` is what Rust's
// `serde_json::to_string` produced — the exact bytes fed into BLAKE3.
//
// # Cross-runtime hazards deliberately avoided
//
// - Integer-valued floats (`5.0` → Rust `"5.0"` vs TS `"5"`). Stay purely
//   integer OR purely fractional (e.g. `5`, `0.5`, `3.14`).
// - `-0.0` (Rust `"-0.0"` vs `JSON.stringify(-0)` → `"0"`). Don't emit it.
// - Integers beyond `Number.MAX_SAFE_INTEGER` (2^53 − 1). `json!(i64)` keeps
//   full Rust precision; JS loses it on parse. Cap at `2^53 - 1`.
// - Lone surrogates in strings — `serde_json::Value` can't hold them anyway
//   (Rust strings are valid UTF-8), so any vector emitted here is safe.

use diff_fusion::domain::idempotency::idempotency_key_hex;
use serde_json::{Value, json};

fn vec_entry(canonical_id: &str, operation: &str, payload: Value) -> Value {
    let canonical_payload_json = serde_json::to_string(&payload).unwrap();
    let expected_hex = idempotency_key_hex(canonical_id, operation, &payload);
    json!({
        "canonicalId": canonical_id,
        "operation": operation,
        "payload": payload,
        "canonicalPayloadJson": canonical_payload_json,
        "expectedHex": expected_hex,
    })
}

fn main() {
    let vectors = vec![
        // ─── Basics ────────────────────────────────────────────────────────
        vec_entry("PO-1", "upsert", json!({"sku": "A1", "qty": 5})),
        vec_entry("PO-1", "upsert", json!({"qty": 5, "sku": "A1"})), // key-reorder of previous
        vec_entry("PO-1", "upsert", json!({})),
        vec_entry("PO-1", "delete", json!(null)),
        vec_entry("PO-1", "upsert", json!([])),

        // ─── Key-reorder permutations (same semantic payload, different ──
        //     write orders — ALL must hash identically)
        vec_entry("REORDER", "sync", json!({"a": 1, "b": 2, "c": 3})),
        vec_entry("REORDER", "sync", json!({"a": 1, "c": 3, "b": 2})),
        vec_entry("REORDER", "sync", json!({"b": 2, "a": 1, "c": 3})),
        vec_entry("REORDER", "sync", json!({"b": 2, "c": 3, "a": 1})),
        vec_entry("REORDER", "sync", json!({"c": 3, "a": 1, "b": 2})),
        vec_entry("REORDER", "sync", json!({"c": 3, "b": 2, "a": 1})),

        // ─── Nested key-reorder permutations ───────────────────────────────
        vec_entry(
            "NESTED-REORDER",
            "sync",
            json!({"outer": {"x": 1, "y": 2}, "meta": {"ver": 1, "ts": 100}}),
        ),
        vec_entry(
            "NESTED-REORDER",
            "sync",
            json!({"meta": {"ts": 100, "ver": 1}, "outer": {"y": 2, "x": 1}}),
        ),

        // ─── Deep nesting (5+ levels) ──────────────────────────────────────
        vec_entry(
            "DEEP-1",
            "upsert",
            json!({"a": {"b": {"c": {"d": {"e": {"f": 42}}}}}}),
        ),
        vec_entry(
            "DEEP-2",
            "upsert",
            json!({"l1": {"l2": {"l3": {"l4": {"l5": {"l6": {"leaf": "found"}}}}}}}),
        ),
        vec_entry(
            "DEEP-ARRAY",
            "upsert",
            json!([[[[[[[1, 2, 3]]]]]]]),
        ),
        vec_entry(
            "DEEP-MIXED",
            "sync",
            json!({
                "root": {
                    "children": [
                        {"id": 1, "kids": [{"id": 2, "kids": [{"id": 3, "kids": []}]}]}
                    ]
                }
            }),
        ),

        // ─── Numeric edge cases (integers) ─────────────────────────────────
        vec_entry("N-ZERO", "upsert", json!({"n": 0})),
        vec_entry("N-NEG", "upsert", json!({"n": -1})),
        vec_entry("N-POS", "upsert", json!({"n": 1})),
        vec_entry("N-I32-MAX", "upsert", json!({"n": 2_147_483_647i64})),
        vec_entry("N-I32-MIN", "upsert", json!({"n": -2_147_483_648i64})),
        // Safe-integer bounds: 2^53 - 1 is JS's `Number.MAX_SAFE_INTEGER`.
        vec_entry("N-SAFE-MAX", "upsert", json!({"n": 9_007_199_254_740_991i64})),
        vec_entry("N-SAFE-MIN", "upsert", json!({"n": -9_007_199_254_740_991i64})),
        // Stay inside safe range. i64::MAX (9.22e18) would corrupt on the JS side.

        // ─── Numeric edge cases (fractional — never integer-valued) ────────
        vec_entry("F-HALF", "upsert", json!({"n": 0.5})),
        vec_entry("F-NEG-HALF", "upsert", json!({"n": -0.5})),
        vec_entry("F-PI", "upsert", json!({"n": 3.14})),
        vec_entry("F-SMALL", "upsert", json!({"n": 0.001})),
        vec_entry("F-MIXED", "upsert", json!({"int": 5, "frac": 0.25, "neg_frac": -1.5})),

        // ─── Unicode edge cases ────────────────────────────────────────────
        // Non-ASCII BMP characters (should pass through as UTF-8, not \u-escaped)
        vec_entry("U-LATIN-EXT", "upsert", json!({"name": "café ☕", "tag": "naïve"})),
        vec_entry("U-CJK", "upsert", json!({"msg": "你好世界"})),
        vec_entry("U-RTL-HEBREW", "upsert", json!({"msg": "שלום עולם"})),
        vec_entry("U-RTL-ARABIC", "upsert", json!({"msg": "مرحبا بالعالم"})),
        // Bidi control marks (U+200E, U+200F) embedded mid-string
        vec_entry("U-BIDI", "upsert", json!({"s": "abc\u{200E}def\u{200F}ghi"})),
        // Zero-width characters (U+200B, U+200C, U+200D, U+FEFF)
        vec_entry(
            "U-ZWJ",
            "upsert",
            json!({"s": "a\u{200B}b\u{200C}c\u{200D}d\u{FEFF}e"}),
        ),
        // Combining characters (base + combining diacritic != precomposed)
        vec_entry(
            "U-COMBINING",
            "upsert",
            json!({"a": "café", "b": "cafe\u{0301}"}), // same glyph, different bytes — must hash DIFFERENTLY
        ),
        // Non-BMP: emoji + surrogate-pair-encoded in UTF-16 (JS), 4 UTF-8 bytes (Rust)
        vec_entry("U-EMOJI", "upsert", json!({"emoji": "😀🎉🚀"})),
        // Family emoji (ZWJ-joined sequence)
        vec_entry("U-ZWJ-FAMILY", "upsert", json!({"s": "👨\u{200D}👩\u{200D}👧\u{200D}👦"})),
        // Supplementary-plane math symbols (U+1D400+)
        vec_entry("U-MATH", "upsert", json!({"s": "𝐇𝐞𝐥𝐥𝐨"})),
        // U+2028 / U+2029 — NOT escaped by serde_json or JSON.stringify
        vec_entry("U-SEPARATORS", "upsert", json!({"s": "line\u{2028}one\u{2029}two"})),

        // ─── Unicode-heavy KEYS (not just values) ──────────────────────────
        // Mixed key byte-widths in one object: BTreeMap sorts by UTF-8 byte
        // order, not by any locale/codepoint-count notion — this is the spec,
        // not a bug. "中文" (3-byte UTF-8) sorts before the two 4-byte keys
        // below it because its lead byte (0xE4) is smaller.
        vec_entry(
            "U-KEY-MIXED",
            "upsert",
            json!({
                "🔑": "emoji-key",       // U+1F511, 4-byte UTF-8
                "𠀀": "cjk-ext-b-key",   // U+20000, CJK Ext-B, 4-byte UTF-8
                "e\u{0301}": "combining-key", // "e" + combining acute (U+0301)
                "ascii": "plain-key",
                "中文": "cjk-key",
            }),
        ),

        // ─── Kernel-pinning vectors (Task 13 differential fuzz findings) ───
        // Both shapes below are confirmed NATIVE-ONLY bugs in the TS port's
        // pre-kernel fallback (kernel/Rust is correct and is the spec); the TS
        // differential fuzz (kernelDifferential.test.ts) excludes these two
        // shapes from its generators with the same rationale. Pinned here so
        // the kernel's correct behavior has a frozen cross-language fixture.
        //
        // "__proto__" is an ordinary map key to `serde_json` — no prototype
        // semantics. The TS native `sortKeysRecursive` silently drops this key
        // (assignment through the inherited `Object.prototype.__proto__`
        // accessor instead of creating an own property).
        vec_entry(
            "U-PROTO-KEY",
            "upsert",
            json!({"__proto__": "not-special-in-rust", "ascii": "plain-key"}),
        ),
        // Supplementary-plane key vs. a BMP key in U+E000-U+FFFF: BTreeMap
        // sorts by UTF-8 byte order (== Unicode code point order), so U+F900
        // (3-byte UTF-8, code point 63744) sorts before U+1D542 (4-byte UTF-8,
        // code point 120130). TS's native key sort compares UTF-16 code units
        // instead, where U+1D542's leading surrogate (0xD800+) numerically
        // undercuts U+F900 — reversing the order.
        vec_entry(
            "U-KEY-SUPPLEMENTARY-VS-BMP",
            "upsert",
            json!({
                "𝕂": "supplementary-plane-key",  // U+1D542, 4-byte UTF-8
                "豈": "bmp-compat-ideograph-key", // U+F900, 3-byte UTF-8
            }),
        ),
        // Integer-like key vs. a key that sorts before it in byte order: BTreeMap
        // sorts "!" (0x21) before "0" (0x30) — pure byte order, no numeral
        // special-casing. TS's native key sort would agree lexicographically, but
        // JS object/JSON.stringify enumeration ALWAYS puts canonical array-index
        // keys ("0", "1", ...) first regardless of any sort, forcing "0" before "!".
        vec_entry(
            "U-KEY-INT-LIKE-ORDER",
            "upsert",
            json!({"!": 1, "0": 2}),
        ),

        // ─── Deep key-ordering stress (10+ unsorted keys) ──────────────────
        // Same 12 keys, two different insertion orders — must hash identically.
        // Also proves sort is lexicographic-by-byte, not numeric: canonical
        // order is k1, k10, k11, k12, k2, k3, ... k9 (BTreeMap byte order).
        vec_entry(
            "REORDER-BIG",
            "sync",
            json!({
                "k7": 7, "k2": 2, "k12": 12, "k1": 1, "k9": 9, "k4": 4,
                "k11": 11, "k3": 3, "k10": 10, "k6": 6, "k8": 8, "k5": 5
            }),
        ),
        vec_entry(
            "REORDER-BIG",
            "sync",
            json!({
                "k1": 1, "k2": 2, "k3": 3, "k4": 4, "k5": 5, "k6": 6,
                "k7": 7, "k8": 8, "k9": 9, "k10": 10, "k11": 11, "k12": 12
            }),
        ),

        // ─── Nested empty containers ────────────────────────────────────────
        vec_entry("EMPTY-NESTED-OBJ", "upsert", json!({"a": {}, "b": []})),
        vec_entry("EMPTY-NESTED-ARR", "upsert", json!([{}, [], {}, []])),
        vec_entry(
            "EMPTY-DEEP",
            "upsert",
            json!({"outer": {"inner_obj": {}, "inner_arr": []}}),
        ),
        vec_entry("EMPTY-ARRAY-OF-EMPTY", "upsert", json!([[], [], []])),
        vec_entry("EMPTY-OBJ-OF-EMPTY", "upsert", json!({"a": {}, "b": {}, "c": {}})),

        // ─── Long strings ───────────────────────────────────────────────────
        vec_entry("LONG-STRING", "upsert", json!({"s": "x".repeat(10_000)})),
        vec_entry("LONG-STRING-UNICODE", "upsert", json!({"s": "日".repeat(2_000)})),

        // ─── Length-prefix boundary cases ──────────────────────────────────
        // Without length-prefixing, these boundary-adjacent IDs would collide.
        vec_entry("a", "bc", json!(null)),
        vec_entry("ab", "c", json!(null)),
        vec_entry("", "", json!(null)),
        vec_entry("", "x", json!(null)),
        vec_entry("x", "", json!(null)),
        // Long ids/ops that land on 8-byte length boundaries
        vec_entry("a".repeat(7).as_str(), "op", json!({})),
        vec_entry("a".repeat(8).as_str(), "op", json!({})),
        vec_entry("a".repeat(9).as_str(), "op", json!({})),

        // ─── Arrays of mixed types ─────────────────────────────────────────
        vec_entry("A-MIX", "upsert", json!([1, "two", null, true, false, 3.14])),
        vec_entry(
            "A-NESTED-MIX",
            "upsert",
            json!([[1, 2], ["a", "b"], [null, true], [{"k": "v"}]]),
        ),
        vec_entry("A-OBJECTS", "upsert", json!([{"id": 1}, {"id": 2}, {"id": 3}])),
        vec_entry(
            "A-REORDER",
            "upsert",
            json!([{"a": 1, "b": 2}, {"b": 2, "a": 1}]), // array order preserved, inner keys canonicalised
        ),

        // ─── Null values at various depths ─────────────────────────────────
        vec_entry("NULL-ROOT", "upsert", json!(null)),
        vec_entry("NULL-LEAF", "upsert", json!({"a": null, "b": 1})),
        vec_entry("NULL-DEEP", "upsert", json!({"a": {"b": {"c": null}}})),
        vec_entry("NULL-ARRAY", "upsert", json!([null, null, null])),
        vec_entry("NULL-MIXED", "upsert", json!({"a": [null, 1, null], "b": null})),

        // ─── String escape fidelity (control + special chars) ──────────────
        vec_entry("ESC-CTRL", "upsert", json!({"s": "line1\nline2\ttab\u{0001}end"})),
        vec_entry("ESC-BACK", "upsert", json!({"s": "a\\b\"c"})),
        vec_entry("ESC-ALL-CTRL", "upsert", json!({"s": "\u{0000}\u{0001}\u{0002}\u{001F}"})),
        vec_entry("ESC-SLASH", "upsert", json!({"url": "https://example.com/path"})), // `/` not escaped
        vec_entry("ESC-MIXED", "upsert", json!({"s": "tab\there\nnewline\"quote\\back"})),

        // ─── Boolean / null / empty patterns ───────────────────────────────
        vec_entry("B-TRUE", "upsert", json!({"flag": true})),
        vec_entry("B-FALSE", "upsert", json!({"flag": false})),
        vec_entry("B-ALL", "upsert", json!({"t": true, "f": false, "n": null})),

        // ─── Same shape, different canonical_id/operation — keys MUST differ
        vec_entry("DIFF-ID-1", "upsert", json!({"x": 1})),
        vec_entry("DIFF-ID-2", "upsert", json!({"x": 1})),
        vec_entry("DIFF-OP-1", "create", json!({"x": 1})),
        vec_entry("DIFF-OP-2", "delete", json!({"x": 1})),
    ];

    let out = Value::Array(vectors);
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
