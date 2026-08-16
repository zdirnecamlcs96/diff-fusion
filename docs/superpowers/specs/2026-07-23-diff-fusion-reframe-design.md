# diff-fusion reframe: Rust SSOT kernel, WASM delivery

**Date:** 2026-07-23
**Status:** Approved design

## Problem

diff-fusion grew as a Rust library with a hand-written TypeScript port. The end goal
is now explicit: use the reconciliation concept inside an existing Node.js project
today, with a Go consumer likely later. Maintaining N hand-ports of the full library
does not scale for a solo maintainer whose primary language is not Rust, and earlier
plans (subprocess drivers, C-ABI, uniffi, full-library WASM) were scoped for a
multi-language library product that is not the actual goal.

## Decision summary

| Decision | Choice | Rationale |
|---|---|---|
| SSOT | Rust crate | Kernel semantics are "mostly frozen" after integration; zero-drift reuse beats native re-implementation when change rate is low |
| SSOT home | This (Rust) repo | Spec, kernel, vector generator, playground all live here; no new repos |
| Kernel scope | Pure functions only | Trivial WASM boundary (JSON in → JSON out, sync), no async callbacks crossing it, best debugging story: any kernel call is reproducible by replaying its JSON arguments |
| Delivery | WASM | Only in-process mechanism covering Node + browser + edge now and Go (via wazero, no cgo) later |
| Orchestrator / I-O | Host-side, per language | It is app-layer glue (~150 lines), bends to each app's storage/queue/retry conventions, and is natively debuggable where it runs |
| Conformance | Golden vectors | Rust generates; every delivery target must pass the same vector files |

## Architecture

```
┌─ application layer (per host, never ported) ────────────────┐
│  user's Node project · CLI · playground · future Go service │
│  orchestrator loop · ancestor store · escalation · adapters │
└──────────────────────────┬──────────────────────────────────┘
                           │ plain data (JSON)
┌─ delivery drivers (thin, per ecosystem) ────────────────────┐
│  rlib (Rust) · wasm-bindgen (JS/TS) · wasip1 + wazero (Go)  │
└──────────────────────────┬──────────────────────────────────┘
┌─ kernel: Rust, SSOT, pure functions ────────────────────────┐
│  three-way diff · canonical JSON · idempotency keys ·       │
│  policy resolution (Tiers 1-3) · invariants ·               │
│  conflict classification                                    │
└─────────────────────────────────────────────────────────────┘
```

## Kernel surface (v1)

All functions are pure, deterministic, and synchronous. Data crosses the boundary as
JSON. No I/O, no callbacks, no host objects.

- `three_way_diff(ancestor, a, b) -> DiffReport` — per-field provenance (`A | B | Both`).
- `resolve(diff, policy_config) -> Resolution` — merged document plus classified
  conflicts. Policies are configured as **data** using the existing serde-ready
  `MergePolicyRef` declarations, not as host callbacks. Covers Tier 1 strategies,
  the LWW escape hatch (mandatory `reason`), Tier 2 invariants, Tier 3 structural
  merges.
- `canonical_json(doc) -> string` — canonical form used for hashing; key-order and
  number-rendering rules are part of the spec.
- `idempotency_key(canonical_id, op, payload) -> hex string`.

Exact exported signatures are finalized in the implementation plan; the constraint
that binds them is *pure, sync, JSON-in/JSON-out*.

**Explicitly not in the kernel:** orchestrator cycle, ancestor store, escalation
queue, webhook parsing, observer/capture, system ports. **Out of scope for v1:**
host-defined custom policies (anything a built-in policy cannot resolve becomes a
classified conflict; the host escalates it).

## Delivery targets

1. **Rust** — the crate itself (`rlib`), used by the playground and any Rust consumer.
2. **JS/TS** — new `src/drivers/wasm.rs` using `wasm-bindgen`, embedded in the
   `diff-fusion-ts` npm package. The package keeps its current public API; kernel
   internals delegate to WASM. The package's orchestrator, adapters, and facades
   remain native TypeScript (app-layer).
   *Toolchain rule:* do **not** use `wasm-pack` (unmaintained since the rustwasm
   org sunset, July 2025). Build with plain `cargo build --target
   wasm32-unknown-unknown` plus the `wasm-bindgen` CLI, both version-pinned.
3. **Go (future, on demand)** — decision deferred to the P4 trigger, not
   pre-committed. Candidate mechanisms, to be re-evaluated against the ecosystem
   at that time: (a) `wasm32-wasip1` build + **wazero** (production-grade today,
   but a dual wasm-bindgen/wasip1 build from one codebase has no established
   precedent); (b) WASM Component Model + generated bindings (the emerging
   single-artifact path; wazero currently lags on component-model support);
   (c) native Go port of the kernel validated by the golden vectors (fallback
   path B). The conformance contract keeps all three options open.

## Conformance contract

- Golden vectors live in this repo under `spec/vectors/` (relocating the existing
  cross-runtime fixtures). The Rust vector generator (already in `examples/`) is the
  only producer.
- Every delivery target runs the same vectors in its CI. A target with red vectors
  is broken, no judgement calls.
- Kernel behavior changes follow spec-first order: update spec + regenerate vectors
  in Rust, then each delivery target updates until green.
- The existing 519-test TypeScript suite is the acceptance gate for the WASM swap:
  it must pass unchanged against the WASM-backed package.
- **Vector hardening (P2):** extend the vector set with adversarial edge cases the
  research shows static suites miss — number rendering (integer-valued floats),
  unicode-heavy map keys, key-ordering stress — and run a one-time differential
  fuzz of the Rust kernel against the still-existing TS kernel before the TS
  source is deleted (cheap while both implementations exist, impossible after).

## Fate of existing artifacts

| Artifact | Fate |
|---|---|
| Rust repo | SSOT home: spec + kernel + vector generator + playground |
| Observer/capture WIP (`crates/observe`, `src/ports/observer.rs`, `src/application/capture.rs`) | Commit as-is, parked; revisit after Node integration proves the need |
| Svelte playground rewrite (`playground/web`) | Commit as-is; remains a Rust-side dev/debug tool |
| `diff-fusion-ts` kernel source (`src/domain/`, `src/application/policy/`) | Deleted after the WASM swap is green (git history preserves it) |
| `diff-fusion-ts` orchestrator, adapters, facades, tests | Kept, native TypeScript |
| `ROADMAP.md` | Rewritten to this framing |
| `PORTABILITY-RESEARCH.md`, `TS_PORT_PLAN.md` | Kept as research records |

## Phases

- **P0 — park WIP.** Commit the observer and playground streams as-is.
- **P1 — spec.** This document, committed. Kernel API surface frozen at v1.
- **P2 — WASM kernel.** `src/drivers/wasm.rs`, cargo + wasm-bindgen CLI build
  wired into `diff-fusion-ts`, package internals swapped to WASM.
  *Gate 1 (correctness):* full TS suite (519 tests) + golden vectors + differential
  fuzz green against WASM.
  *Gate 2 (benchmark):* WASM-backed kernel benchmarked against the pure-TS kernel
  on representative payloads before any deletion. Boundary overhead must be
  immaterial for the workload (< 5% of a sync cycle). Precedent forcing this gate:
  a production team shipped this exact shape (JSON-string boundary, small frequent
  calls) and found pure TS 2.2-4.6x faster per call. If the gate fails, fall back
  to path B: keep the native TS kernel as the JS implementation, pinned by vectors.
  TS kernel source is deleted only after both gates pass.
- **P3 — Node integration.** Wire into the user's existing Node project: system
  adapters, ancestor storage in the project's database, thin cycle loop (reusing the
  package's TS orchestrator). Shadow mode first.
  *Gate:* shadow-mode diff run over real project data.
- **P4 — Go (on demand).** Re-evaluate delivery mechanism first (see Delivery
  targets §3), then build: chosen kernel delivery + Go orchestrator loop.
  *Gate:* golden vectors green in Go.

## Testing

- Kernel: existing Rust unit/property tests unchanged; vectors regenerate only on
  intentional spec changes.
- Boundary: TS suite doubles as the boundary conformance suite (P2 gate).
- App-layer per host: orchestrator ordering rules (ancestor commits last; pushes
  carry idempotency keys; OCC on every push) are enforced by integration tests that
  already exist in Rust and TS; Go ports the same scenarios in P4.

## Validation research (2026-07-23)

Four-angle web research verified the design against industry practice. Confirmed:
pure-kernel/host-shell is an established pattern (sans-io: quinn-proto, tree-sitter,
Firezone); golden-vectors-as-contract is proven at exactly this shape (BLAKE3,
RFC 8785, JOSE, Ethereum consensus-specs), with documented failures confined to
stateful/multi-call territory this kernel excludes; a single JSON blob per call is
the measured-fastest WASM boundary shape (beats fine-grained object marshaling).
Surfaced risks folded into this spec: the P2 benchmark gate (one production team
reverted this exact shape to pure TS on boundary overhead), wasm-pack abandonment
(toolchain rule above), the unprecedented dual wasm-bindgen/wasip1 build (P4
decision deferred), and vector edge-case hardening plus one-time differential
fuzzing. Accepted residual risk: TS-first maintainer with a Rust core (bus-factor
criticism known from Biome/Oxc debates), mitigated by the frozen-kernel policy.

## Debugging model

The kernel is deterministic and side-effect free, so a host never steps into WASM:
capture the JSON arguments of a failing call, replay them in a Rust test or the
playground. Everything a host developer touches day-to-day (orchestrator, adapters,
storage) is native code in that host's language with normal breakpoints and stack
traces.
