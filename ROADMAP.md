# Roadmap: diff-fusion

## A note on scope

Earlier versions of this file (v0.1.x) drew a hard line: diff-fusion is
"NOT a sync engine." That statement reflected where the code *was*, not
where it was *going*. The real intent has always been **two-way
reconciliation between two authoritative systems** — the JSON diff was
the first primitive built toward that goal, not the end state.

This roadmap makes the intent explicit. The out-of-scope list is now
narrower; the feature set grows accordingly. If you rely on the old
"detection-only" framing, the Tier-0 facade API (`DiffFusion`) still works
unchanged — that's the entry point for the diff-only use case.

---

## Scope

### IN SCOPE

diff-fusion reconciles two authoritative systems using a **three-way diff
against a stored ancestor**, with per-field merge policies, optimistic
concurrency on push, and an escalation queue for unresolved conflicts.

```
System A ──▶ Canonical ──┐
                         ├── Three-way diff ──▶ Resolve ──▶ Push (both sides) ──▶ Commit ancestor
System B ──▶ Canonical ──┘                                  ▲
                                                            │
                                  Stored ancestor ──────────┘
```

Concretely, diff-fusion provides:

1. **Schema-driven canonical transformation** — map each external format to
   a common intermediate format (CIF). Linear scaling with system count
   (n transformers, not n²).
2. **Three-way diff with provenance** — per-field `source: A | B | Both`,
   which is the signal that makes policy resolution possible. Without it,
   reconciliation degrades to timestamp tie-breaking.
3. **Per-field merge policies** — tiered stack (Tier 1 strategies, Tier 2
   invariants, Tier 3 structural merges). Declarative, inline with the
   schema.
4. **Optimistic concurrency on push** — every upsert asserts the previous
   version. Mismatch → `StaleWrite` → restart the cycle.
5. **Deterministic idempotency keys** — `hash(canonical_id + op + payload)`.
   Retries collapse to no-ops.
6. **Escalation queue** — unresolvable conflicts route to human review with
   full provenance. They do not silently resolve.
7. **Shadow mode** — pull-and-diff without pushing, for new-adapter
   validation.

### OUT OF SCOPE

These are deliberately excluded. diff-fusion will say no or propose a
different solution:

- **Workflow engine** — reconciles state; does not orchestrate multi-step
  business processes. Use Temporal, Airflow, or Step Functions for that.
- **Real-time event bus** — batches in convergence windows. If you need
  sub-second propagation for a specific field, that field probably wants
  one-way ownership, not bidirectional sync.
- **Generic "integration platform"** — one-way sync and ETL are not goals.
  Use Fivetran, Airbyte, or Zapier.
- **CRDT** — merge semantics are policy-based, not mathematically
  convergent. The library accepts that some conflicts are genuine and
  require human resolution. That's a feature, not a gap to close.

---

## The hard rules

These are enforced in code and in review:

1. **Last-write-wins is not a default strategy.** It lives in
   `policy::escape_hatch::LastWriteWins` and requires a written `reason`
   at construction time. Timestamp-based resolution fails at scale (clock
   skew, batch windows) — research converges on this, going back to Saito &
   Shapiro 2005 and the CRDT work that followed. Prefer `OwnedBy`,
   `Additive`, or `StateMachine` first.
2. **The ancestor update is the last step of a cycle.** Ordering:
   resolve → push to stale sides → wait for confirmations → *then* commit
   the new ancestor. Advancing the ancestor before pushes confirm is how
   silent drift starts.
3. **Every push carries an idempotency key** derived purely from inputs.
   Non-deterministic keys (timestamps, random IDs) cause duplicate records.
4. **Every push uses optimistic concurrency.** Adapters for systems
   without native OCC fake it via read-before-write.
5. **Webhook payloads are parsed only after signature verification.**

---

## Architecture: Rust kernel, per-host delivery

As of the 2026-07-23 reframe, the library is split into three layers instead
of one growing Rust crate. The driver for the split: the actual near-term
goal is using this reconciliation logic inside an existing Node.js project,
with a Go consumer likely after that — maintaining N full hand-ports doesn't
scale for a solo, non-Rust-primary maintainer, and the kernel's semantics
are mostly frozen once integrated. Full design and rationale:
[`docs/superpowers/specs/2026-07-23-diff-fusion-reframe-design.md`](docs/superpowers/specs/2026-07-23-diff-fusion-reframe-design.md).
Implementation plan for the phases below: [`docs/superpowers/plans/2026-07-23-p0-p2-wasm-kernel.md`](docs/superpowers/plans/2026-07-23-p0-p2-wasm-kernel.md).

```
┌─ application layer (per host, never ported) ────────────────┐
│  user's Node project · CLI · future Go service             │
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

**This (Rust) repo is the SSOT home**: spec, kernel, golden vector generator
(`spec/vectors/`, `core/examples/gen_idempotency_vectors.rs` and friends), and
boundary JSON Schema (`spec/schema/`) all live here. The kernel
is pure, synchronous, JSON-in/JSON-out — no I/O, no callbacks crossing the
boundary — which keeps every delivery target's debugging story the same:
replay the failing call's JSON arguments in a Rust test.

`diff-fusion-ts` (the `sdk/typescript/` package) delivers the kernel to Node/browser
via `wasm-bindgen` (`core/src/drivers/wasm.rs` here, vendored artifact at
`sdk/typescript/wasm/`). The npm package's public API is unchanged; internals delegate
to the WASM kernel. See `sdk/typescript/README.md`'s Kernel section and `sdk/typescript/CLAUDE.md`
for the wire contract and build instructions.

### Layers unchanged by the reframe

The orchestrator cycle, ancestor store, escalation queue, port/adapter
traits, and the `SyncEngine`/`DiffFusion` facades are all still native,
per-host code — they were never candidates for the kernel, since they're
app-layer glue (storage, retries, adapters) that bends to each host's own
conventions. Everything below "What's shipped" was true before the reframe
and still is.

### What's shipped

| Layer | Module | Status |
| ----- | ------ | ------ |
| Two-way diff | `compare` | ✅ shipped |
| Canonical transformation | `transform`, `cif_trait` | ✅ shipped |
| Three-way diff with provenance | `diff::three_way` | ✅ shipped (kernel SSOT; WASM-delivered to TS) |
| Ancestor store (trait + in-memory) | `ancestor` | ✅ shipped |
| Idempotency keys | `idempotency` | ✅ shipped (kernel SSOT; WASM-delivered to TS) |
| Error categories | `error` | ✅ shipped |
| Tier 1 policies | `policy::{owned_by, additive, append, state_machine}` | ✅ shipped |
| Escape hatch (LWW w/ reason) | `policy::escape_hatch` | ✅ shipped |
| Tier 2 invariants | `policy::invariants` | ✅ shipped |
| Tier 3 structural merges | `policy::structural` | ✅ shipped |
| Port trait + capabilities | `port` | ✅ shipped |
| In-memory reference adapter | `adapters::test_memory` | ✅ shipped |
| Shared contract test suite | `tests/integration/contract_tests.rs` | ✅ shipped |
| Escalation queue | `ports::escalation` + `adapters::in_memory_escalation` | ✅ shipped |
| Orchestrator cycle (pull → diff → resolve → push → commit) | `application::orchestrator` | ✅ shipped |
| Shadow mode | `application::orchestrator::run_shadow` | ✅ shipped |
| `SyncEngine` facade | `drivers::sync_engine` | ✅ shipped |
| Invariants wired into cycle | `application::orchestrator` | ✅ shipped |
| Conflict taxonomy (class per conflict) | `application::policy::ConflictClass` | ✅ shipped |
| Filesystem ancestor store | `adapters::filesystem_ancestor` | ✅ shipped |
| Layered directory structure (domain / application / ports / adapters / drivers) | `core/src/` tree | ✅ shipped |
| WASM kernel driver (`three_way_diff`, `merge_field`, `canonical_json`, `idempotency_key_hex`) | `drivers::wasm` | ✅ shipped |
| Golden vector conformance (`spec/vectors/`, 121 vectors: 82 idempotency + 39 kernel) + boundary JSON Schema (`spec/schema/`) | `core/examples/gen_*` | ✅ shipped |
| wasip1 kernel driver (Go delivery via wazero) | `drivers::wasip1` + `sdk/golang/kernel` | ✅ shipped |
| Kernel-vector conformance for `three_way_diff`/`merge_field` (`spec/vectors/kernel-vectors.json`, byte-exact incl. error strings) — closes the coverage gap that previously left 2 of 4 kernel functions unverified across runtimes | `core/tests/kernel_vectors_tests.rs` + TS/Go equivalents | ✅ shipped |
| CI test workflow (Rust/TS/Go suites, plus a wasm-freshness job that rebuilds the committed `.wasm` artifacts and re-runs the suites against them) | `.github/workflows/test.yml` | ✅ shipped |

### Verified by tests

- Property: three-way diff of `(anc, anc, anc)` is empty; swapping A and B
  inverts `source` without changing the set of entries.
- Property: `Additive` is commutative under A/B swap.
- End-to-end: ancestor advances only after both pushes confirm; a seeded
  conflict lands in the escalation queue and neither side is mutated.
- Contract suite: an adapter is "done" when it passes every test in
  `run_contract_suite`. No judgement calls.
- Cross-runtime: the WASM kernel swap kept the pre-existing 519-test
  TypeScript suite green unchanged (Gate 1), plus golden vectors and a
  one-time differential fuzz of the Rust kernel against the (still
  present, pending deletion) native TS kernel. The fuzz caught three
  native-only canonicalization bugs the kernel didn't have — `__proto__`
  key drop, UTF-16-vs-code-point key sort, and integer-like-key
  enumeration order — now pinned as vectors (`U-PROTO-KEY`,
  `U-KEY-SUPPLEMENTARY-VS-BMP`, `U-KEY-INT-LIKE-ORDER`). This is the
  concrete case for kernel-as-SSOT: a hand-port can silently diverge on
  JS/Rust semantics that never come up in hand-written test cases.
- Boundary overhead (Gate 2): kernel `threeWayDiff` on a 50-field entity
  runs ~112-114µs/call in Node — 4.2-4.4x slower than the native TS it
  replaced, but immaterial against the 500µs budget (a sync cycle includes
  a ≥10ms network round-trip). `idempotencyKeyHex` is the other direction:
  the WASM BLAKE3 implementation is ~1.5x *faster* than the JS port.

---

## Forward roadmap

Priority order. Each item has a driver — if the driver doesn't apply yet,
the work waits. This supersedes the old "Not yet built" list: the
diff/policy/orchestrator layers above are done; what's left is delivery
and integration, not new kernel semantics.

### P3 — Node integration

Wire the kernel into the user's existing Node project: system adapters for
that project's real systems, ancestor storage in the project's own
database (replacing the in-memory/filesystem stores used for development),
and a thin cycle loop reusing the `diff-fusion-ts` orchestrator. Shadow
mode first — pull-and-diff without pushing, to validate against real data
before any write path is live.

*Gate:* a shadow-mode diff run over real project data.

### P4 — Go delivery (shipped: kernel delivery)

Mechanism resolved 2026-07-26: `wasm32-wasip1` build run by **wazero** (pure
Go, zero cgo). Full design and rationale:
[`docs/superpowers/specs/2026-07-26-p4-go-delivery-design.md`](docs/superpowers/specs/2026-07-26-p4-go-delivery-design.md).

The Go orchestrator loop remains deferred until a real Go consumer exists —
same as the rest of the app layer waits per host. This ships kernel
delivery only: `sdk/golang/kernel` wraps the four kernel functions, nothing else.

*Gate:* passed 2026-07-26 — 82 golden vectors green under `go test`
(`sdk/golang/kernel/kernel_test.go`).

### Parked (revisit on real need, not before)

- **Observer/capture stream** (`core/crates/observe`, `core/src/ports/observer.rs`,
  `core/src/application/capture.rs`) — committed as-is; revisit after the P3
  Node integration proves the need.

### Durable persistence (unchanged driver: first real integration)

- ✅ **`FilesystemAncestorStore`** — JSON-per-entity, atomic writes via
  tempfile+rename, hashed filenames for arbitrary canonical_id strings.
  Suitable for single-process deployments.
- ⏳ SQLite / Postgres ancestor store — for multi-process concurrency.
- ⏳ Durable escalation queue (SQS / Postgres / similar) — the in-memory
  queue still loses items on restart.

### Schema-carried policy declarations

`policy::declaration::MergePolicyRef` is serde-ready and now doubles as the
WASM kernel's `resolve`/`merge_field` policy-config wire shape (schema at
`spec/schema/policy-config.schema.json`), but it's still not wired into the
JSON schema parser in `transform.rs` for host-side schema-driven policy
declaration. Deferring until a real use case (P3) demands it.

### Observability

Metrics (cycles run, conflicts escalated, stale-write restarts), tracing
spans per cycle, and structured logging. Deferred until deployed.

### Webhook ingestion path

The port trait declares `parse_webhook` / `verify_webhook` methods but
they're not yet part of the orchestrator's triggered flow. Needs a real
webhook source to design against.

---

## What changed and why

### The reframe (2026-07-23): Rust kernel as SSOT, WASM delivery

| Before | Now | Reason |
| ------ | --- | ------ |
| `diff-fusion-ts` was a hand-written full port of the Rust crate | `diff-fusion-ts` delegates three-way diff, idempotency, and built-in policy resolution to the Rust kernel via WASM; native TS bodies survive as `native*` functions pending a human-gated deletion | Zero-drift reuse beats re-implementation once kernel semantics are mostly frozen; three real canonicalization bugs (see above) had already crept into the native port unnoticed |
| No formal conformance contract across runtimes | `spec/vectors/` (82 vectors) + `spec/schema/` (3 boundary schemas) are the shared contract; every delivery target must pass the same vectors, no judgement calls | A target with red vectors is broken, full stop — the same discipline the contract test suite already applied to adapters |
| Full-library multi-language port was the implicit plan | Kernel scope is pure functions only (diff, resolve, canonical JSON, idempotency key); orchestrator/adapters/ports stay native per host | The actual goal is one Node integration plus a likely Go consumer, not a general multi-language library product |

Two native TS modules are deliberately *not* kernel-backed — `invariants.ts`
(not kernel-able: no wire shape for an arbitrary user predicate) and the
richer knobs on `SetByKey` beyond what `MergePolicyRef` declares. These are
scope boundaries, not omissions; see `sdk/typescript/CLAUDE.md` for the
current list. (`compare.ts` and `transform.ts` were kernel-v2 candidates
here too; both are now kernel-backed.)

### What changed from v0.1.0 and why

| v0.1.0 | Now | Reason |
| ------ | --- | ------ |
| "NOT a sync engine" | Two-way sync is in scope | The stated goal all along; the v0.1 scope was the current state, not the target |
| `ConflictStrategy` enum with `LastWriteWins` as a peer strategy | `policy::escape_hatch::LastWriteWins` with mandatory `reason`; Tier 1 strategies are `OwnedBy`, `Additive`, `Append`, `StateMachine`, `SetByKey` | LWW-by-default is a known anti-pattern at scale (Saito & Shapiro; CRDT motivation) — it's an escape hatch, not a first choice |
| Source-of-truth was a free-text metadata field | `policy::owned_by::OwnedBy` is a first-class strategy with runtime enforcement | Declarative ownership resolves ~80% of conflicts before they arise (App.md § 03) |
| No ancestor, no three-way diff | Three-way diff against a stored ancestor | Without an ancestor, "A changed" and "both changed" are indistinguishable and silent overwrites become possible |
| No per-push idempotency or OCC | Both required at the port level | Direct fix for the duplicate-record and silent-overwrite classes of bug |

The existing `DiffFusion` facade (from `core/src/facade.rs`) is unchanged.
Users doing detection-only can ignore the rest of the library.

---

## References

The design is not novel; it synthesizes well-researched components.

- Cockburn, A. *Hexagonal Architecture* (2005) — ports & adapters.
- Evans, E. *Domain-Driven Design* (2003) — anti-corruption layer /
  transformers.
- Terry, D. B. et al. *Bayou* (SOSP 1995) — three-way reconciliation with
  a stored ancestor.
- Saito, Y. & Shapiro, M. *Optimistic Replication* (ACM CSur 2005) — why
  LWW fails at scale.
- Shekow, M. *Syncpal* (DAIS 2019) — iterative reconciliation algorithm
  for file synchronizers; also a diff-first decomposition.
- Shapiro, M. et al. *Conflict-Free Replicated Data Types* (INRIA 2011)
  — the motivation for policy-based merges over timestamp-based ones.

See `App.md` § 06 for the full provenance matrix.
