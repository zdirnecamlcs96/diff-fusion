# Worked example: `idempotency.rs` → `idempotency.ts`

A side-by-side port of `src/domain/idempotency.rs` (Rust) ↔ `ts/src/domain/idempotency.ts` (TS), with each translation rule annotated. Open both files alongside this document.

## Why this file is the canonical example

It exercises seven of the trickiest rules in one ~150-line module:

- `&str` and `Vec<u8>` interop (`string` and `Uint8Array`)
- BLAKE3 hashing (`@noble/hashes/blake3`)
- `BTreeMap` semantics (canonical-JSON requirement)
- Cross-runtime byte-identity guarantee
- A length-prefixed binary framing protocol
- A `proptest!` block (→ `fast-check`)
- Multiple `#[test]` blocks (→ vitest `it()` with same names)

If you understand this port, you can do every other module in the codebase.

## Module signature

| Rust | TS |
|---|---|
| `pub fn idempotency_key(canonical_id: &str, operation: &str, payload: &Value) -> [u8; 32]` | `export function idempotencyKey(canonicalId: string, operation: string, payload: JsonValue): Uint8Array` |
| `pub fn idempotency_key_hex(...) -> String` | `export function idempotencyKeyHex(...): string` |

Rules applied:
- `&str` → `string` (no borrow distinction in TS).
- `&Value` → `JsonValue`. Custom union type defined in `ts/src/domain/types.ts`; mirrors what `serde_json::Value` accepts. No external dep.
- `[u8; 32]` → `Uint8Array` (length not encoded in the type; assert in tests if needed).
- snake_case `idempotency_key` → camelCase `idempotencyKey` (function names follow JS convention; struct/JSON field names follow Rust serde tags).

## Hashing core

Rust:

```rust
let mut hasher = Hasher::new();
write_field(&mut hasher, canonical_id.as_bytes());
write_field(&mut hasher, operation.as_bytes());
let payload_str = serde_json::to_string(payload).expect("...");
write_field(&mut hasher, payload_str.as_bytes());
*hasher.finalize().as_bytes()
```

TS:

```ts
const encoder = new TextEncoder();
const idBytes = encoder.encode(canonicalId);
const opBytes = encoder.encode(operation);
const payloadBytes = encoder.encode(canonicalStringify(payload));

const totalLen = 8 + idBytes.length + 8 + opBytes.length + 8 + payloadBytes.length;
const buf = new Uint8Array(totalLen);
let offset = 0;
offset = writeField(buf, offset, idBytes);
offset = writeField(buf, offset, opBytes);
writeField(buf, offset, payloadBytes);

return blake3(buf);
```

Differences worth noting:

1. **Streaming vs buffered.** Rust uses `Hasher::new()` and feeds bytes incrementally. The `@noble/hashes` `blake3()` function takes a single buffer. We build one `Uint8Array` and pass it. Functionally identical output; minor allocation overhead.
2. **Canonical JSON helper.** Rust `serde_json::to_string` on a `BTreeMap`-backed `Value` already sorts keys. JS objects don't, so we need `canonicalStringify` (defined just above in the same file). **This is the single most important rule of the port.** Skip it and you get silent divergence the moment a payload object has out-of-order keys.
3. **`expect` vs implicit.** Rust's `serde_json::to_string` returns `Result`, but on `Value` it can't fail — hence `.expect(...)`. In TS, `JSON.stringify` only throws on cyclic graphs, which `JsonValue` (a strict union) excludes structurally. No expect/throw layer needed.

## Length-prefix framing

Rust:

```rust
fn write_field(hasher: &mut Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}
```

TS:

```ts
function writeField(buf: Uint8Array, offset: number, bytes: Uint8Array): number {
  writeU64LE(buf, offset, bytes.length);
  buf.set(bytes, offset + 8);
  return offset + 8 + bytes.length;
}

function writeU64LE(buf: Uint8Array, offset: number, value: number): void {
  // Split into low/high 32-bit halves to avoid BigInt.
  const low = value >>> 0;
  const high = Math.floor(value / 0x1_0000_0000) >>> 0;
  buf[offset]     =  low         & 0xff;
  buf[offset + 1] = (low  >>> 8) & 0xff;
  // ... etc up to offset+7
}
```

The Rust `(len as u64).to_le_bytes()` trivially writes 8 little-endian bytes. JS has no built-in u64 LE writer for `Uint8Array` (DataView's `setBigUint64` exists but takes `BigInt`). We open-code the split. Field lengths are always far below `2^32`, so the `high` half is `0` in practice — but write it anyway for byte-for-byte fidelity with Rust.

## Hex encoding

Rust:

```rust
fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}
```

TS:

```ts
const HEX = "0123456789abcdef";

function hexLower(bytes: Uint8Array): string {
  let out = "";
  for (const b of bytes) {
    out += HEX[(b >> 4) & 0xf];
    out += HEX[b & 0xf];
  }
  return out;
}
```

Direct port. `HEX[i]` returns `string | undefined` under `noUncheckedIndexedAccess` — but the indices are mathematically bounded to `[0, 15]`, so the `undefined` arm is unreachable. If your linter complains, assert with `as string` or precompute the table as a `readonly [string, string, ...]` tuple.

## Tests — one-to-one port

Rust `#[cfg(test)] mod tests` (8 test functions) maps to a single TS file `ts/tests/unit/domain/idempotency.test.ts` with one `describe` block per logical group.

| Rust `#[test] fn` | TS `it("...")` |
|---|---|
| `deterministic_for_same_inputs` | `deterministic_for_same_inputs` |
| `key_order_in_payload_does_not_matter` | `key_order_in_payload_does_not_matter` |
| `different_canonical_id_changes_key` | `different_canonical_id_changes_key` |
| `different_operation_changes_key` | `different_operation_changes_key` |
| `different_payload_changes_key` | `different_payload_changes_key` |
| `length_prefix_prevents_boundary_collision` | `length_prefix_prevents_boundary_collision` |
| `hex_form_is_64_chars_lowercase` | `hex_form_is_64_chars_lowercase` |
| `proptest! same_inputs_same_key` | `fast-check` property `idempotencyKey is deterministic` |

Names ported verbatim, snake_case preserved. The diff at the test level is now line-comparable to the Rust test block — when one of these tests changes upstream, you can spot the drift instantly.

## Cross-runtime golden vectors

Beyond the ported unit tests, this module is also covered by `ts/tests/fixtures/idempotency-vectors.json` — a Rust-generated set of `{ canonicalId, operation, payload, canonicalPayloadJson, expectedHex }` rows. The generator is `examples/gen_idempotency_vectors.rs`.

The TS test reads the fixture and asserts each row:

```ts
for (const v of vectors) {
  expect(idempotencyKeyHex(v.canonicalId, v.operation, v.payload))
    .toBe(v.expectedHex);
}
```

Rust is the source of truth; if the TS implementation diverges (e.g. someone "improves" `canonicalStringify`), this test fails immediately with the offending vector.

## Order of operations when porting a new module

This is the order that worked for diff-fusion. Every step is a compile or test that you can verify:

1. Read the Rust file end-to-end. Note dependencies (other crate modules, external crates).
2. Stub the TS file with the public function signatures and `throw new Error("not implemented")` bodies.
3. Port the `#[cfg(test)] mod tests` block first. Run vitest, watch them all fail.
4. Port the implementation. Run vitest until green.
5. Port any `proptest!` blocks to `fast-check`. Run again.
6. If the module is on the cross-runtime byte-identity list (`idempotency`, `filesystem-ancestor` filename derivation, anything sharing on-disk format with Rust), generate a golden-vector fixture from Rust and add a fixture-driven test.
7. `npx tsc --noEmit -p tsconfig.test.json` clean.
8. Diff the test file against the Rust `#[cfg(test)] mod tests` block — names should be identical, assertions should be a one-to-one mapping.
