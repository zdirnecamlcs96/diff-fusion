# diff-fusion/ts — Agent Context

TypeScript port of the Rust `diff-fusion` crate. This file captures cross-cutting conventions so teammates don't need them re-briefed. (The original phased port plan, `TS_PORT_PLAN.md`, is retired — git history has it.)

## Repo geometry

- Rust source of truth: `../../src/src/` (hexagonal: `domain / application / ports / adapters / drivers`).
- TS target: `./src/` — mirrors the Rust layout file-for-file.
- Tests: `./tests/{unit,integration,contract}` — unit mirrors `src/`, integration ports Rust `../../src/tests/integration/` 1:1.
- Cross-language golden fixtures live in `../../spec/vectors/` (e.g. `idempotency-vectors.json` generated from Rust); tests read them directly.

## Runtime + toolchain

- Node ≥ 20, ESM-only, NodeNext module resolution.
- `tsconfig.json` — strict, `exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`.
- `tsconfig.test.json` — same but includes `tests/` and `noEmit`.
- Test runner: `vitest`; property tests: `fast-check`; hash: `@noble/hashes/blake3`; CLI: `commander` + `picocolors`.

### Common commands (run from `sdk/typescript/`)

```bash
npx tsc --noEmit                       # src-only typecheck
npx tsc --noEmit -p tsconfig.test.json # full typecheck including tests
npx vitest run                         # full suite
npx vitest run tests/unit/<path>       # single file
```

Installs use a project-local cache: `npm install --cache "$(pwd)/.npm-cache"` — the user's `~/.npm` is currently root-owned.

## Kernel (Rust WASM, SSOT)

Core semantics (three-way diff, canonical JSON, idempotency keys, built-in
policy resolution) are computed by the Rust crate compiled to WASM, vendored
at `wasm/`. The Rust crate is SSOT; this package's internals delegate to it.

- **`src/kernel.ts` is the only file allowed to touch the wasm module.**
  Every other caller goes through its typed wrapper functions
  (`kernelThreeWayDiff`, `kernelMergeField`, `kernelCanonicalJson`,
  `kernelIdempotencyKeyHex`).
- **Wire protocol.** JSON strings cross the boundary; `FieldChange` wire
  shape is `snake_case`. For a wire field, an *absent* key means "that side
  didn't touch the field"; a *present* `null` means "cleared to null" — never
  collapse the two (no `?? null` / `?? undefined`). The camelCase +
  `undefined` ↔ wire translation lives only in `kernel.ts`.
- **Native TS bodies retired (Task 15, kernel-v2).** The hand-written TS
  twins of the kernel (three-way diff, canonical JSON, idempotency key hex,
  each policy's TS-side merge, `compare.ts`, `transform.ts`) were deleted
  once each differential fuzz/gate passed; git history preserves them. The
  wasm kernel is SSOT — don't reintroduce parallel native bodies.
- **Drift pairs — deliberately not kernel-backed:**
  `invariants.ts` (not kernel-able: `Invariant` is a per-host extension
  point — users register their own TS objects via `SyncEngine.invariant()`
  — while Rust's `InvariantSet` takes compiled-in `Box<dyn Invariant>`, so
  there's no wire shape for an arbitrary user predicate).
- **Rebuild the artifact** from the repo root: `./scripts/build-wasm.sh`.
  Requires the rustup-managed `cargo` (with `wasm32-unknown-unknown`
  installed) first in `PATH` — Homebrew's `cargo` lacks that target and fails
  with `error[E0463]: can't find crate for core`.
- **Conformance.** Golden vectors live in `../../spec/vectors/` (Rust generator
  is the sole producer); tests read them from there directly.
  Boundary JSON Schema lives in `../spec/schema/` (generate via
  `cargo run --example gen_schema --features schema-gen` from `src/`).

## Translation conventions (inherited from the retired port plan §5)

- **`.js` import extensions under NodeNext.** Example: `import { foo } from "../domain/types.js";` (TS source, `.js` because NodeNext resolves the emitted path).
- **Discriminated unions with literal `kind` tag** — never positional. Exhaustive switches need `default: const _: never = x; throw new Error(...)`.
- **Match Rust `#[serde(tag = "type")]` names AND casing** — JSON declarations must cross runtimes unchanged.
- **`undefined` not `null`** for optionals. Use `exactOptionalPropertyTypes`: omit the field rather than setting `undefined`.
- **`Map<K, V>` not POJO** when keys aren't fixed strings.
- **Policies = classes** (plan §5 preference for Rust-reader familiarity). Pure data types = plain interfaces + free functions.
- **Throw** for adapter/IO failures; **return** discriminated unions for in-domain branching (`MergeOutcome`, `InvariantOutcome`, `SyncOutcome`, `CycleOutcome`).
- **Canonical JSON for hashing**: sort keys recursively before digest. Lives in `domain/idempotency.ts` — reuse, don't reinvent.

## Byte-identical cross-runtime guarantees

Non-negotiable: `idempotencyKeyHex`, filesystem ancestor filenames, and any format shared with Rust must match the Rust output byte-for-byte. Phase 9 commits golden vectors generated from Rust; TS tests read and match.

Known corner case: JS `number` has no int/float tag, so integer-valued floats (e.g. `5.0`) render as `"5"` in TS vs `"5.0"` in Rust. Shipped vectors deliberately avoid that shape. See `ts/src/domain/idempotency.ts` comments and `~/.claude/projects/.../memory/feedback_canonical_json.md`.

This applies to the WASM kernel too, since it's still JS crossing the boundary: `Additive` merging `1 + 5` emits `6.0` on the Rust side but JS sees plain `6` once the wasm string result is `JSON.parse`d — the tag is lost at the JS/JSON boundary regardless of which side computed the number. Canonical output for the same logical value can therefore differ depending on *which side* (Rust vs TS) produced it first, even for a single field. This is a single-runtime-deterministic guarantee, not a cross-runtime one: replaying the same input on the same runtime always reproduces the same output; the drift is only possible when a value crosses runtimes mid-flow.

## Task + team workflow

- Team: `diff-fusion-ts`. Config: `~/.claude/teams/diff-fusion-ts/config.json`.
- Tasks are the single source of truth for phase progress. Always `TaskGet` before starting; `TaskUpdate` to claim (`owner`, `status: "in_progress"`) and when done (`status: "completed"`).
- Refer to teammates by name. After finishing, send a short summary to `team-lead` and let the status note any shape decisions worth reviewing.

## TDD default

Use the `superpowers:test-driven-development` skill: port every Rust `#[test]` in the source you're porting as a vitest test FIRST (same name where possible), then implement until green. Add fast-check property tests where the plan calls them out (commutativity for `Additive`, determinism for `idempotencyKey`, no-op for `threeWayDiff(x,x,x)`, etc.).
