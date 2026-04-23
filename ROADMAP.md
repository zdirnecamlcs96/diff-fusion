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

## Current state

The library is built up in layers. Each layer compiles and tests on its
own — this was deliberate, bottom-up decomposition (the diff primitive is
the kernel; everything else grows outward from it). The ordering mirrors
Syncpal (Shekow, DAIS 2019) and Bayou (Terry et al., SOSP 1995): diff
first, then reconcile.

| Layer | Module | Status |
| ----- | ------ | ------ |
| Two-way diff | `compare` | ✅ shipped |
| Canonical transformation | `transform`, `cif_trait` | ✅ shipped |
| Three-way diff with provenance | `diff::three_way` | ✅ shipped |
| Ancestor store (trait + in-memory) | `ancestor` | ✅ shipped |
| Idempotency keys | `idempotency` | ✅ shipped |
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
| Layered directory structure (domain / application / ports / adapters / drivers) | `src/` tree | ✅ shipped |

### Verified by tests

- Property: three-way diff of `(anc, anc, anc)` is empty; swapping A and B
  inverts `source` without changing the set of entries.
- Property: `Additive` is commutative under A/B swap.
- End-to-end: ancestor advances only after both pushes confirm; a seeded
  conflict lands in the escalation queue and neither side is mutated.
- Contract suite: an adapter is "done" when it passes every test in
  `run_contract_suite`. No judgement calls.

---

## Not yet built

In priority order. Each item has a clear driver — if the driver doesn't
apply yet, the work waits.

### Durable persistence

- ✅ **`FilesystemAncestorStore`** — JSON-per-entity, atomic writes via
  tempfile+rename, hashed filenames for arbitrary canonical_id strings.
  Suitable for single-process deployments.
- ⏳ SQLite / Postgres ancestor store — for multi-process concurrency.
- ⏳ Durable escalation queue (SQS / Postgres / similar) — the in-memory
  queue still loses items on restart.

Driver for the remaining items: first real integration with a live system.

### First real adapter

Pick one authentic integration pair and build one adapter end-to-end.
Until this happens, the port trait and capability flags are under-pressured
by synthetic tests alone.

Candidates (not chosen yet): Shopify + Amazon, NetSuite + internal
service, custom REST-pair. The contract test suite makes "is this adapter
done?" a test question, not a judgement call.

### Schema-carried policy declarations

`policy::declaration::MergePolicyRef` is serde-ready but not yet wired into
the existing JSON schema parser in `transform.rs`. The bridge function
lets users declare policies in their schema JSON and build a runtime
`PolicyMap` from it. Deferring until a real use case demands it.

### Observability

Metrics (cycles run, conflicts escalated, stale-write restarts), tracing
spans per cycle, and structured logging. Deferred until deployed.

### Webhook ingestion path

The port trait declares `parse_webhook` / `verify_webhook` methods but
they're not yet part of the orchestrator's triggered flow. Needs a real
webhook source to design against.

---

## What changed from v0.1.0 and why

| v0.1.0 | Now | Reason |
| ------ | --- | ------ |
| "NOT a sync engine" | Two-way sync is in scope | The stated goal all along; the v0.1 scope was the current state, not the target |
| `ConflictStrategy` enum with `LastWriteWins` as a peer strategy | `policy::escape_hatch::LastWriteWins` with mandatory `reason`; Tier 1 strategies are `OwnedBy`, `Additive`, `Append`, `StateMachine`, `SetByKey` | LWW-by-default is a known anti-pattern at scale (Saito & Shapiro; CRDT motivation) — it's an escape hatch, not a first choice |
| Source-of-truth was a free-text metadata field | `policy::owned_by::OwnedBy` is a first-class strategy with runtime enforcement | Declarative ownership resolves ~80% of conflicts before they arise (App.md § 03) |
| No ancestor, no three-way diff | Three-way diff against a stored ancestor | Without an ancestor, "A changed" and "both changed" are indistinguishable and silent overwrites become possible |
| No per-push idempotency or OCC | Both required at the port level | Direct fix for the duplicate-record and silent-overwrite classes of bug |

The existing `DiffFusion` facade (from `src/facade.rs`) is unchanged.
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
