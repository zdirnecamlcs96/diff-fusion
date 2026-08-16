---
layout: default
title: Concepts
nav_order: 3
---

# Concepts

The domain model, end to end — what each term means and where it lives in
the code.

## Vocabulary

These words are used consistently in code, tests, logs, and docs. Prefer
them over synonyms when talking about the library.

| Word | What it means | Not |
|---|---|---|
| **System** | An external source-of-truth (ERP, internal service) | "provider", "backend" |
| **Port** | The `SystemPort` trait (or `AncestorStore`, `EscalationQueue`) | "connector", "driver" |
| **Adapter** | A per-system implementation of the port | |
| **Canonical** | The library's internal representation — CIF | "normalized", "common", "unified" |
| **Ancestor** | The last-synced canonical view, stored in the ancestor store | "baseline", "snapshot" |
| **Changelog** | The list of per-field changes after a three-way diff | "delta", "diff" |
| **Resolver** | `policy::resolve` — applies policies to a changelog | "merger" |
| **Escalation** | Routing unresolved conflicts to human review | "manual intervention", "fallback" |
| **Cycle** | One pass of pull → diff → resolve → push → commit-ancestor | |
| **Shadow mode** | Running a cycle without pushing or advancing the ancestor | "dry run" |

## CIF

Integrating *n* systems pairwise needs n(n-1)/2 transformers, and every
new system means touching every existing one. Transforming each system to
a Common Intermediate Format (CIF) instead needs only *n* transformers —
one per system, in and out — and adding a system is one new transformer,
not a wave of edits across every pair. The CIF also isolates the merge
engine from source-system quirks entirely: nothing above the transform
step sees a system's native shape.

## Three-way diff

Every cycle compares three states, not two: the stored ancestor
(last-known-reconciled), side A's current view, and side B's current
view. `three_way_diff(ancestor, a, b)` (`core/src/domain/diff/three_way.rs`)
produces a `Changelog` where every entry is a `FieldChange` carrying a
`source: ChangeSource::A | B | Both`.

`source: Both` does **not** mean conflict — it means both sides moved from
the ancestor since the last cycle. The resolver decides whether the two
new values happen to agree or actually need a policy to reconcile. Without
the ancestor, there is no way to tell "A changed" from "both changed", and
reconciliation degrades to timestamp tie-breaking.

## Ancestor store

The `AncestorStore` port (`core/src/ports/ancestor.rs`) holds the last
known reconciled canonical state per `(entity_type, canonical_id)`. A
cycle advances it only *after* every push confirms — never before. Two
shipped implementations: `InMemoryAncestorStore` (tests, ephemeral) and
`FilesystemAncestorStore` (durable, atomic JSON-file writes,
`adapters::filesystem_ancestor`).

## Merge policies

Per-field strategies, dispatched by path from a `Changelog` entry. Most
fields never need anything past Tier 1.

- **Tier 1 (~85% of fields)** — declarative strategies: `OwnedBy` (one
  side is authoritative), `Additive` (numeric deltas accumulate),
  `Append` (array concatenation), `StateMachine` (enum transitions,
  illegal moves rejected).
- **Tier 2 (~10%)** — invariants: post-merge predicates that validate or
  transform a Tier-1 candidate (e.g. "a closed PO can't accept more
  receipts").
- **Tier 3 (~3%)** — structural: `SetByKey` merges collections by a
  stable per-side identity key, with `OnAdded` / `OnRemoved` /
  `OnBothChanged` hooks.
- **Tier 4 (~2%)** — custom: implement `MergePolicy` directly for a field
  that doesn't fit the above. If more than 5% of fields land here, that's
  a signal a new Tier 1 strategy should be promoted instead.

**Escape hatch:** `LastWriteWins` (`application/policy/escape_hatch.rs`)
is not a default and is not free to construct — it requires a written
`reason` at construction, because timestamp-based resolution fails at
scale (clock skew, batch windows) and the justification needs to be
visible in logs and conflict reports.

`suggest_policies` (`application/policy/suggest.rs`) walks a CIF schema
and proposes a starting policy map — a heuristic, not a substitute for
reviewing each field.

## Idempotency and optimistic concurrency

From `core/src/domain/idempotency.rs`:

> `idempotency_key(canonical_id, operation, payload)` is a pure function
> of its inputs — no timestamps, no random IDs. When an adapter retries a
> push after a transient failure, the external system sees the same key
> and treats the second attempt as a no-op instead of a duplicate record.
>
> The payload is serialized via `serde_json::to_string`. `serde_json::Map`
> is `BTreeMap`-backed by default (no `preserve_order` feature enabled), so
> object keys are always sorted — two semantically-equal `Value`s hash
> identically.
>
> Each field is length-prefixed before hashing so `("a", "bc", ...)` and
> `("ab", "c", ...)` never collide.

The key is a 32-byte BLAKE3 hash. Every `SystemPort::upsert` also takes an
`expect_version: Option<&str>` (`core/src/ports/system.rs`) — the
orchestrator's assertion about the version it last observed. A mismatch
signals another actor moved first.

## Escalation queue

Anything a policy can't decide — or explicitly rejects — routes to the
`EscalationQueue` port with full provenance: the path, the reason, the
underlying change, and a `class` tag so a caller can branch disposition
per category:

| Class | Trigger |
|---|---|
| `NoPolicy` | No merge policy declared for the path — caller misconfiguration. |
| `PolicyConflict` | A Tier-1 policy ran and returned `MergeOutcome::Conflict` (illegal state transition, divergent `SetByKey` element, non-numeric `Additive` input). |
| `InvariantViolation` | Tier-1 produced a candidate, but a Tier-2 invariant rejected it. |

## Error categories

From `core/src/domain/error.rs`:

> Every error that flows through the orchestrator falls into exactly one
> of three categories. Each category drives a different recovery path:
>
> - `SyncError::Transient` — retry with backoff (network, rate limit, 5xx).
> - `SyncError::StaleWrite` — restart the cycle; another actor moved first.
> - `SyncError::Conflict` — the resolver cannot decide; route to escalation.
>
> The categories are the interface. Never construct a bare error — pick one.

## Cycle ordering

The non-negotiable sequence for one pass of `Orchestrator::run_cycle_at`:

1. Pull from **both** sides before diffing.
2. Diff before resolving — no guessing at values without provenance.
3. Resolve before pushing — no pushing a partially-merged state.
4. Push both stale sides before touching the ancestor.
5. Commit the ancestor *only* after every push confirmed.

If step 5 happens before step 4 completes, the next cycle observes the
"new" ancestor but has no record that a push was missed — **silent
drift**. The next cycle believes state converged when it did not. This is
the single most load-bearing ordering constraint in the whole library.
{: .warning }
