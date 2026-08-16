---
name: porting-rust-to-typescript
description: Use when translating Rust source to TypeScript and you need vetted translation rules (Result/enum/Option/serde tagged unions), cross-runtime hashing/JSON gotchas (number rendering, BTreeMap key order, atomic rename), or a TDD-first workflow that ports Rust #[test] cases verbatim before implementing
---

# Porting Rust → TypeScript

## When to use

- Translating a Rust crate or module to TypeScript.
- The Rust source uses `serde`, `Result<T,E>`, traits, `Option<T>`, or `BTreeMap`.
- You need byte-identical output across runtimes (shared on-disk format, shared digest, shared idempotency key).

## Core workflow

1. **Scaffold once.** Node ≥ 20, ESM, `"module": "NodeNext"`, `strict`, `exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`. Vitest + fast-check + `@noble/hashes` if you need blake3. Two tsconfigs (`tsconfig.json` for src, `tsconfig.test.json` extending it for tests with `noEmit`).
2. **Per module, port tests FIRST.** For each Rust `#[test] fn foo_bar` write `it("foo_bar", () => …)` — same name, snake_case preserved. Watch them fail. Then implement until green. `proptest!` → `fast-check`.
3. **Cross-runtime byte-identity → generate from Rust.** Add `examples/gen_*.rs` that emits golden vectors as JSON; commit to `ts/tests/fixtures/`; TS test reads + asserts. Rust is the source of truth.

## Quick reference (top rules — full table in `translation-rules.md`)

| Rust | TypeScript |
|---|---|
| `Result<T, E>` | **Throw** at IO/adapter boundary; **return** discriminated union for in-domain branching. No Result library. |
| Rust enum deriving `thiserror::Error` | `class extends Error` with `kind` discriminator + static constructors. Stack traces + `instanceof` rely on the class. Don't use a bare union for error types. |
| `enum Foo { A, B { x: i32 } }` (non-error) | `\| { kind: "A" } \| { kind: "B"; x: number }`. Always literal `kind` tag, never positional. **Casing**: preserve PascalCase by default; match `#[serde(rename_all = "...")]` only when declared. |
| Exhaustive `match` | `switch (x.kind)` with `default: const _: never = x; throw new Error(...)`. |
| `trait T` + `impl T for S` | `interface T` + `class S implements T`. |
| `Box<dyn T>` / `Arc<dyn T>` | Just the interface type. No ref-counting. |
| `Option<T>` | `T \| undefined`, never `null`. **Omit** the field when None — don't set it to `undefined`. |
| `BTreeMap<String, _>` | JS objects don't preserve order — sort keys recursively before hashing or serialising (`canonicalStringify`). |
| `#[serde(tag = "kind")]` | TS literal `kind` matches Rust JSON output exactly, including casing. |
| `#[async_trait]` | Plain `async` on interface methods. |
| `Vec<u8>` | `Uint8Array`. |
| `u64` ms timestamp | `number` (safe through year 5138). Avoid `BigInt`. |

## Cross-runtime gotchas

**Read `cross-runtime-gotchas.md` before shipping anything that has to match Rust byte-for-byte.** Highlights:

- `5.0` renders `"5"` (TS) vs `"5.0"` (Rust). `-0.0` renders `"0"` vs `"-0.0"`.
- Integers > `2^53 − 1` lose precision in JS.
- Object key order is not stable in JS — sort before hashing.
- `fs.rename` is not atomic across volumes on Windows.

## Common mistakes

- **Translating test names to English.** Keep `transient_is_retriable` snake_case; don't rewrite as `"is retriable when transient"`.
- **Adding embellishment tests** beyond the Rust `#[cfg(test)] mod tests` block. Port verbatim FIRST; add TS-specific tests in a separate `describe`.
- **Lowercasing variant tags arbitrarily** (`"transient"` for Rust `Transient` with no serde annotation). Preserve PascalCase by default.
- **Bare union for error types.** Use `class extends Error` — JS code relies on `instanceof` and stack traces.
- **Positional union variants** (`["Resolved", value]`) — use `{ kind: "Resolved", value }`.
- **`null` for optionals** — use `undefined`.
- **`field: undefined` instead of omitting** — `exactOptionalPropertyTypes` flags this.
- **Forgetting `.js` import suffix** under NodeNext.
- **Skipping `canonicalStringify`** — silent hash divergence.
- **`BigInt` for u64 ms timestamps** — `number` is safe through year 5138.
- **`5.0`-shaped floats or `-0.0` in cross-runtime fixtures** — silent runtime divergence.

## Worked example

`example-port.md` walks `src/domain/idempotency.rs` → `ts/src/domain/idempotency.ts` with each rule annotated.
