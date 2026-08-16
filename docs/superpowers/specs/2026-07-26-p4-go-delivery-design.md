# P4 — Go delivery of the diff-fusion kernel (design)

**Date:** 2026-07-26
**Status:** approved (mechanism + scope confirmed by maintainer)
**Prerequisite reading:** `2026-07-23-diff-fusion-reframe-design.md` (kernel/SSOT
framing, delivery-target options).

## Goal

Deliver the four kernel functions — `three_way_diff`, `merge_field`,
`canonical_json`, `idempotency_key_hex` — to Go consumers, with the same
wire contract the TS delivery uses.

**Gate:** the 82 golden idempotency vectors (`spec/vectors/idempotency-vectors.json`)
green under `go test`, plus boundary smoke tests mirroring the Rust
`drivers/wasm.rs` test set (absent-vs-null, error paths).

**Out of scope:** a Go orchestrator loop. That is app-layer, per-host code;
it waits for a real Go consumer with real adapters. The reframe spec lists it
under P4, but the gate does not require it and building it against no
consumer is speculation.

## Mechanism decision

**Chosen: `wasm32-wasip1` artifact run by wazero** (pure-Go WASM runtime,
zero cgo).

- Zero drift: the same Rust kernel bytes are the SSOT — the exact reason the
  TS package moved onto the WASM kernel after the differential fuzz caught
  three hand-port bugs.
- wazero is production-grade today and imports as a normal Go module.

Rejected:

- **Native Go port** — fallback only. Vectors would pin it, but the TS
  experience is the documented case against hand-ports.
- **WASM Component Model** — wazero's component-model support still lags;
  revisit if that changes.

The reframe spec flagged "no established precedent for a dual
wasm-bindgen/wasip1 build from one codebase" as a risk. Resolution: Cargo
per-target dependencies — `wasm-bindgen` (and `drivers/wasm.rs`) compile only
for `wasm32-unknown-unknown`; the wasip1 driver compiles only for
`target_os = "wasi"`. The two builds never see each other's code.

## Rust changes

1. **Extract shared wire layer.** Move `WireFieldChange`, `WireChangelog`,
   `double_option`, `unwrap_presence`, and the four `*_impl` functions from
   `src/drivers/wasm.rs` into `src/drivers/wire.rs`, compiled for every
   target (the `_impl`s are already pure `&str → Result<String, String>`).
   `wasm.rs` keeps only the `#[wasm_bindgen]` export macro; its tests move
   with the impls. No behavior change. Shipped deviation: the four
   `*_impl` functions are `pub`, not `pub(crate)` — cfg-gating the wasm
   driver out of host builds would otherwise leave them dead code there;
   `pub` also serves the rlib delivery target this roadmap already lists.
   Don't revert this to `pub(crate)`.
2. **New `src/drivers/wasip1.rs`** (compiled only for
   `all(target_arch = "wasm32", target_os = "wasi")`):
   - Exports `df_alloc(len: u32) -> u32` and `df_dealloc(ptr: u32, len: u32)`
     backed by `Vec` allocation.
   - Exports `df_three_way_diff`, `df_merge_field`, `df_canonical_json`,
     `df_idempotency_key_hex`. Each takes `(ptr, len)` `u32` pairs per string
     argument (UTF-8 JSON), returns a packed `u64` = `(ptr << 32) | len` of a
     guest-allocated UTF-8 buffer the host must read then free via
     `df_dealloc`.
   - Return buffer is a JSON envelope: `{"ok": <result-string>}` or
     `{"err": "<message>"}` — errors travel in-band; no ABI error channel.
3. **Cargo:** move `wasm-bindgen` under
   `[target.'cfg(all(target_arch = "wasm32", target_os = "unknown"))'.dependencies]`;
   gate the driver modules in `src/drivers/mod.rs` with matching `cfg`s.
4. **Build script** `scripts/build-wasm-wasip1.sh`: `cargo build --release
   --target wasm32-wasip1`, copy artifact to `go/kernel/diff_fusion.wasm`.
   Same toolchain rule as `build-wasm.sh`: rustup-managed cargo with the
   target installed.

## Go package

New Go module at `go/` (monorepo sibling of `ts/`, same absorb pattern):

- Package `kernel` (import path suffix `go/kernel`).
- Go floor is `1.25.0` — wazero v1.12.0 requires it; this supersedes the
  plan's `go 1.24` line.
- Artifact embedded via `go:embed diff_fusion.wasm`; wazero compiles and
  instantiates it in `New(ctx)`.
- API — thin, JSON bytes in/out, consumer decodes:

  ```go
  k, err := kernel.New(ctx)
  defer k.Close(ctx)
  out, err := k.ThreeWayDiff(ctx, ancestor, a, b []byte) ([]byte, error)
  out, err := k.MergeField(ctx, change, policyRef, mergeCtx []byte) ([]byte, error)
  out, err := k.CanonicalJSON(ctx, doc []byte) ([]byte, error)
  hex, err := k.IdempotencyKeyHex(ctx, canonicalID, operation string, payload []byte) (string, error)
  ```

- `{"err": ...}` envelope surfaces as a Go `error`.
- Concurrency: one wazero module instance per `Kernel`; instances are not
  goroutine-safe. `// ponytail: single instance, add a pool if contention shows`.

## Testing

- `go/kernel/kernel_test.go`:
  - Conformance: read `../../spec/vectors/idempotency-vectors.json` directly
    (monorepo relative path — no sync script), assert all 82 `expectedHex`
    and `canonicalPayloadJson` values via `IdempotencyKeyHex` /
    `CanonicalJSON`. This is the gate.
  - Boundary smoke tests mirroring `wire.rs` tests: diff produces
    present-`null` vs absent key correctly; inconsistent `source` errors;
    invalid JSON errors; `merge_field` additive resolve.
- Rust side: existing tests move with the impls to `wire.rs`; `cargo test`
  stays green. `build-wasm.sh` (TS artifact) must still produce a working
  artifact — verified by the existing TS suite.

## Docs follow-up (after gate passes)

- `ROADMAP.md`: P4 row → shipped via wasip1 + wazero; note vectors green.
- `go/README.md`: build/regenerate instructions, wire contract pointer to
  `spec/schema/`.
