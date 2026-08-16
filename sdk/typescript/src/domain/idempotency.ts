/**
 * Deterministic idempotency keys for pushes to external systems.
 *
 * `idempotencyKey(canonicalId, operation, payload)` is a pure function of its
 * inputs — no timestamps, no random IDs. When an adapter retries a push after
 * a transient failure, the external system sees the same key and treats the
 * second attempt as a no-op instead of a duplicate record.
 *
 * Byte-identical to the Rust implementation at `src/domain/idempotency.rs`.
 * Divergence would break cross-runtime replay safety, so this file is covered
 * by cross-language golden vectors in `spec/vectors/idempotency-vectors.json`.
 *
 * # Framing (must match Rust byte-for-byte)
 *
 * BLAKE3 is fed three length-prefixed fields, in order:
 *   1. `canonicalId` bytes (UTF-8)
 *   2. `operation` bytes (UTF-8)
 *   3. canonical JSON bytes of `payload` (UTF-8)
 *
 * Each field is prefixed with its byte length as a little-endian `u64` (8
 * bytes). Length-prefixing prevents boundary collisions like
 * `("a", "bc", ...)` vs `("ab", "c", ...)`.
 *
 * Canonical JSON is what Rust's `serde_json::to_string` produces on a
 * `BTreeMap`-backed `Value`: object keys sorted lexicographically, no extra
 * whitespace, standard JSON escaping. `canonicalStringify` below mirrors that.
 */

import { kernelCanonicalJson, kernelIdempotencyKeyHex } from "../kernel.js";
import type { JsonValue } from "./types.js";

/**
 * Serialise a `JsonValue` to the same bytes Rust's `serde_json::to_string`
 * produces on a semantically-equal `BTreeMap`-backed `Value`.
 *
 * Key invariants:
 * - object keys are emitted in code-point (UTF-8 byte) order, matching the
 *   Rust kernel's `BTreeMap<String, _>` iteration order
 * - array order is preserved (Rust does the same)
 * - no whitespace between tokens (compact form)
 * - string escaping matches serde_json's default: only control chars (< 0x20),
 *   `"` and `\` are escaped; non-ASCII is passed through as UTF-8
 *
 * Delegates to the WASM kernel (`kernelCanonicalJson`), which does the actual
 * sorting and encoding.
 */
export function canonicalStringify(value: JsonValue): string {
  return kernelCanonicalJson(value);
}

/**
 * Compute the 32-byte idempotency key.
 *
 * Stable under:
 * - retries (same inputs → same key)
 * - object-key reordering inside the payload (canonical serialisation)
 *
 * Sensitive to:
 * - any change in `canonicalId`, `operation`, or the canonical payload bytes
 */
export function idempotencyKey(
  canonicalId: string,
  operation: string,
  payload: JsonValue,
): Uint8Array {
  const hex = idempotencyKeyHex(canonicalId, operation, payload);
  return Uint8Array.from(hex.match(/.{2}/g)!.map((b) => Number.parseInt(b, 16)));
}

/** Hex-encoded form of {@link idempotencyKey} — 64 lowercase hex chars. */
export function idempotencyKeyHex(
  canonicalId: string,
  operation: string,
  payload: JsonValue,
): string {
  return kernelIdempotencyKeyHex(canonicalId, operation, payload);
}

