# Cross-runtime gotchas

These are the silent-divergence traps. They affect anything that must produce *byte-identical* output on Rust and TypeScript: idempotency keys, on-disk filenames, hashes, signatures, anything compared `===` across runtimes.

If your port doesn't have a cross-runtime guarantee, skim this anyway — at least three of these have bitten projects that thought they didn't have one.

## 1. Integer-valued floats (`5.0` vs `5`)

**Symptom:** `JSON.stringify(5.0)` → `"5"`. Rust `serde_json::to_string(&json!(5.0))` → `"5.0"`. Hashes diverge.

**Why:** JS `number` is IEEE 754 double — there's no type-level distinction between `5` (integer) and `5.0` (float). Rust `serde_json::Value::Number` keeps the source representation.

**Mitigation:** In any cross-runtime fixture, keep numbers either *purely integer* (`5`, `-3`, `100`) or *purely fractional* (`5.5`, `-0.001`). Never `5.0`-shaped. Document this rule at the top of `examples/gen_*.rs` so the next person adding vectors sees it.

## 2. Negative zero (`-0.0` vs `0`)

**Symptom:** `JSON.stringify(-0)` → `"0"`. Rust `serde_json::to_string(&json!(-0.0))` → `"-0.0"`.

**Why:** JS `JSON.stringify` normalises `-0` to `0`. Rust serde preserves the sign.

**Mitigation:** Never emit `-0.0` from fixture generators. If your domain actually requires preserving negative zero (rare), you'd need a custom JSON encoder on both sides — out of scope for most ports.

## 3. Integers beyond `Number.MAX_SAFE_INTEGER`

**Symptom:** Rust `i64` value `9_007_199_254_740_993` (= `2^53 + 1`) parses on the TS side as `9_007_199_254_740_992` — last bit lost.

**Why:** JS `number` is 53-bit precision. Anything beyond `±(2^53 − 1) = ±9_007_199_254_740_991` rounds.

**Mitigation:**
- Cap fixture i64 literals at `±(2^53 − 1)`.
- If your domain genuinely needs full 64-bit integers (account balances in cents, nano-second timestamps, IDs from systems that allocate the full range), use string representation in JSON on both sides — not `BigInt`, which doesn't `JSON.stringify` cleanly.

## 4. Object key order — JS objects vs Rust `BTreeMap`

**Symptom:** Two semantically-equal payloads `{ a: 1, b: 2 }` and `{ b: 2, a: 1 }` hash differently in TS until you sort. Rust's default `serde_json::Map` is `BTreeMap`-backed, so it always sorts.

**Why:** JS object property iteration is *insertion order* for string keys (with a wrinkle for integer-string keys, which iterate ascending — yet another foot-gun, but unrelated here).

**Mitigation:** Always `canonicalStringify` (recursively sort keys) before hashing or comparing. The helper lives in `ts/src/domain/idempotency.ts`. Don't inline; import.

## 5. `fs.rename` atomicity differs by OS and volume

**Symptom:** `fs.rename(tmp, final)` is atomic on POSIX. On Windows, it's atomic *within* a single volume but **not** across volumes — and Node throws `EXDEV` when the temp directory is on a different drive.

**Why:** NTFS rename across volumes requires a copy + delete, not a directory-entry rewrite.

**Mitigation:**
- On Windows, ensure the store root and `os.tmpdir()` live on the same volume. Document this constraint in the adapter README.
- For full Windows safety, write tmp inside the store root: `path.join(storeRoot, ".tmp-" + crypto.randomUUID())` then rename. Same volume guaranteed.
- Wrap rename in a try/catch + copy-fallback if cross-volume rename is unavoidable.

## 6. Numeric epsilon: `f64::EPSILON` ≡ `Number.EPSILON`

**Symptom:** None — these are exactly equal (`2.220446049250313e-16`).

**Why:** Both are IEEE 754 double precision.

**Mitigation:** No action. Three-way diff numeric equality ports cleanly. Document the equivalence in the test so a future reader doesn't assume divergence.

## 7. Unicode normalisation is NOT applied automatically

**Symptom:** `"café"` (NFC, precomposed `U+00E9`) and `"café"` (NFD, base + combining acute) hash to *different* idempotency keys. This happens on both runtimes — Rust and TS agree — but it can surprise callers who expected Unicode-aware equality.

**Why:** Both `serde_json::to_string` and `JSON.stringify` emit the bytes they were given. No NFC/NFD normalisation.

**Mitigation:** This is the *correct* byte-level behaviour for a hash. If a caller needs normalisation-invariant idempotency keys, normalise *before* calling `idempotencyKey(...)`:

```ts
const normalised = canonicalId.normalize("NFC");
const key = idempotencyKey(normalised, op, payload);
```

Document this at the call site, not inside the helper. The helper's job is byte-fidelity; normalisation is policy.

## 8. `tsconfig.json` strictness flags that catch all of these (and more)

These are non-negotiable for porting work where correctness depends on the type system catching mistakes:

```json
{
  "strict": true,
  "exactOptionalPropertyTypes": true,
  "noUncheckedIndexedAccess": true,
  "module": "NodeNext",
  "moduleResolution": "NodeNext"
}
```

- `exactOptionalPropertyTypes` rejects `{ field: undefined }` where the type is `{ field?: T }`. Forces you to *omit* the field, which is what JSON serialisation expects.
- `noUncheckedIndexedAccess` makes `arr[i]` typed as `T | undefined` — no more silent `undefined` flowing into hash inputs.
- `strict` enables null checks, no implicit any, etc.

Without these, a TS port can pass `tsc --noEmit` and still produce wire-format incompatibilities at runtime.

## 9. Memoise this list

Every gotcha here was discovered the hard way — by a divergence that didn't show up until cross-runtime fixtures were running. Add new ones as you find them, and update the corresponding `examples/gen_*.rs` header comment so future fixture authors see them before adding bad data.
