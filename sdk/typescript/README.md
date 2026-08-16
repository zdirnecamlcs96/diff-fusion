# diff-fusion

**A TypeScript library for two-way reconciliation between authoritative systems.**

Transform each system's JSON to a canonical format (CIF), compute a three-way diff against a stored ancestor, resolve per-field merge policies, and push the result back — with optimistic concurrency and deterministic idempotency keys. Unresolvable conflicts route to a review queue with full provenance.

This package is a behaviour-equivalent port of the [Rust diff-fusion crate](../../core/). Idempotency keys and filesystem ancestor filenames are byte-identical across runtimes, so you can ingest with one runtime and reconcile with the other.

> **Name:** "fusion" = fusing different formats into CIF for comparison — not merging data blindly.

## Status

**Alpha.** API surface is stable within a layer but may shift across minor versions until 1.0. Not yet published to npm; install from the git repo (see below).

## Install

```bash
npm install diff-fusion
```

While pre-publish, pin from git:

```bash
npm install github:zdirnecamlcs96/diff-fusion
```

**Requirements:** Node ≥ 20, ESM-only, NodeNext module resolution. Your `tsconfig.json` should use `"module": "NodeNext"` (or bundler equivalent) — the package ships ESM with `.js` import specifiers.

## Two tiers, one package

- **Tier 0 — `DiffFusion`.** Detection only. Transform source JSON to CIF and compare two CIF values. No policies, no state, no I/O. Good for ad-hoc diffing, schema validation, and data-quality tooling.
- **Tier 1 — `SyncEngine`.** Full reconciliation: three-way diff, per-field policies, optimistic concurrency, ancestor store, escalation queue. One fluent builder hides the moving parts.

Start at Tier 0 when you just need "what changed". Move to Tier 1 when you need "reconcile these two systems, tell me what to push, and queue anything I can't decide".

## Quick start — Tier 0 (`DiffFusion`)

```ts
import { DiffFusion } from "diff-fusion";

const schema = {
  cif_schema: {
    product_id: { type: "string", required: true },
    product_name: { type: "string", required: true },
    price: { type: "number", required: true },
  },
  transformations: {
    salesforce: {
      product_id: { source_path: "Id", type: "string" },
      product_name: { source_path: "Name", type: "string" },
      price: { source_path: "Price__c", type: "number" },
    },
    shopify: {
      product_id: { source_path: "id", type: "string" },
      product_name: { source_path: "title", type: "string" },
      price: { source_path: "variants.0.price", type: "number" },
    },
  },
};

const engine = new DiffFusion(schema);

const salesforce = { Id: "SF-1", Name: "Widget", Price__c: 29.99 };
const shopify = { id: "SH-1", title: "Widget", variants: [{ price: 34.99 }] };

const result = engine.transformAndCompare(salesforce, "salesforce", shopify, "shopify");
if (result.ok) {
  console.log(`conflicts: ${result.report.totalConflicts}`);
  for (const c of result.report.conflicts) {
    console.log(`  ${c.path}: ${c.oldValue} → ${c.newValue}`);
  }
}
```

Run the full example:

```bash
npx tsx examples/facadeUsage.ts
```

## Quick start — Tier 1 (`SyncEngine`)

> **Alpha caveat.** The package root currently re-exports only the two facade surfaces (`DiffFusion`, `SyncEngine`, their builders, and their result types). Policies and adapters live in the repo but aren't on the public barrel yet — plan Phase 10 will expand the exports. For now, clone the repo and import via relative paths, as the shipped `examples/` do. The snippet below uses the eventual published shape so it stays correct across the barrel expansion.

```ts
import {
  SyncEngine,
  // Published once the barrel expands:
  //   Additive, OwnedBy, StateMachine, StateTransition, TestMemoryAdapter
} from "diff-fusion";

const erp = new TestMemoryAdapter("erp");
const inv = new TestMemoryAdapter("inv");

// Seed both systems with the same ancestor state, then let them drift.
const starting = { price: 100, qty_recv: 5, status: "open" };
erp.seed("purchase_order", "PO-42", starting);
inv.seed("purchase_order", "PO-42", starting);

const engine = SyncEngine.builder(erp, inv)
  .policy("price", new OwnedBy("erp"))
  .policy("qty_recv", new Additive())
  .policy(
    "status",
    new StateMachine([
      new StateTransition("open", "closed"),
      new StateTransition("open", "cancelled"),
    ]),
  )
  .seedAncestor("purchase_order", "PO-42", starting)
  .build();

// Shadow run — compute the merge without writing.
const preview = await engine.preview("purchase_order", "PO-42");

// Real cycle — push the merged value to whichever side is stale, then
// advance the ancestor last.
const outcome = await engine.sync("purchase_order", "PO-42");
switch (outcome.kind) {
  case "NoOp":     /* nothing to do */ break;
  case "Synced":   console.log(`pushed to ${outcome.pushedTo.join(", ")}`); break;
  case "Escalated": console.log(`${outcome.conflicts.length} conflict(s) queued`); break;
}
```

Run the full example:

```bash
npx tsx examples/twoWaySync.ts
```

## Kernel

Core semantics — three-way diff, canonical JSON, idempotency keys, and
built-in policy resolution — are computed by the Rust `diff-fusion` crate,
compiled to WASM and vendored at `wasm/` (`diff_fusion_bg.wasm`, wasm-bindgen
0.2.100, `--target nodejs`). The Rust crate is the single source of truth;
this package's public API is unchanged, but internals delegate to that WASM
kernel instead of re-implementing the logic in TypeScript. Only
[`src/kernel.ts`](./src/kernel.ts) touches the wasm module — every other file
calls its typed wrapper functions.

**Why:** a hand-written TS port drifts from the Rust semantics silently. A
one-time differential fuzz of the (now-deleted) native TS kernel against
the Rust kernel caught three real bugs the native port had — `__proto__` key
drop, UTF-16-vs-code-point key sort order, and integer-like-key enumeration
order — now pinned as golden vectors. The kernel is the frozen reference;
TypeScript stops re-deriving it.

**Wire contract.** The kernel speaks JSON strings over the wasm boundary
using Rust's wire shape: `snake_case` field names, and for `FieldChange` in
particular, an *absent* key means "that side didn't touch the field" while a
*present* `null` means "the field was cleared to null". This distinction
matters — collapsing it (e.g. `?? null`) loses the "untouched" case. The
camelCase-and-`undefined` ↔ wire translation happens only inside
`kernel.ts`; nothing else in this codebase should serialize kernel wire
shapes directly.

**Rebuild the artifact** (from the repo root):

```bash
./scripts/build-wasm.sh
```

This requires the rustup-managed `cargo` (with the `wasm32-unknown-unknown`
target installed) first in `PATH` — Homebrew's `cargo` lacks that target and
fails with `error[E0463]: can't find crate for core`.

**Status:** the native TypeScript kernel bodies (the hand-written twins of
three-way diff, canonical JSON, idempotency key hex, and each policy's
TS-side merge) have been retired now that the differential fuzz and
benchmark gates passed (git history preserves them). A handful of modules
are deliberately *not* kernel-backed — see [`CLAUDE.md`](./CLAUDE.md) for
the current list and why.

## Core concepts

**CIF (Common Intermediate Format).** The canonical JSON shape both sides are transformed into. Integrating *n* systems needs *n* transformers (one per system to CIF), not *n(n-1)/2* pairwise ones. CIF also cleanly decouples the merge engine from source-system quirks.

**Three-way diff.** Every cycle compares three states: the stored ancestor (last-known-good), side A's current value, side B's current value. Each field carries a `source: "a" | "b" | "both"` tag so policies can decide "one side moved" vs "both sides moved". The ancestor is what makes a merge — not a diff — possible.

**Per-field merge policies (Tier 1).** Declarative rules that run on one `FieldChange`. Built-in:

| Policy         | Use case                                              |
|----------------|-------------------------------------------------------|
| `OwnedBy`      | One side owns the field; other side's writes ignored. |
| `Additive`     | Counters; both sides' deltas accumulate.              |
| `Append`       | Arrays; union both sides' additions.                  |
| `StateMachine` | Enum transitions; reject illegal moves.               |
| `SetByKey`     | Cross-system arrays keyed by per-side stable anchors. |
| `LastWriteWins`| Escape hatch — requires a written justification.      |

You can also declare policies as JSON via `MergePolicyRef` and build them with `build(ref)`.

**Escalation queue.** Anything a policy can't decide (or that a policy explicitly rejects) routes to a queue with full provenance: the path, the reason, the change, and a `class` tag (`NoPolicy` / `PolicyConflict` / `InvariantViolation`) so your UI can branch disposition per category.

**Ancestor store.** Durable record of "last known reconciled state", keyed by `(entityType, canonicalId)`. The cycle advances it **last**, after the push succeeds, so a crash between push and ancestor-commit means a retry re-does the same safe merge instead of losing history.

**Idempotency keys.** Every push carries a deterministic 32-byte BLAKE3 key derived from `(canonicalId, operation, canonical-JSON payload)`. Retries produce the same key, so adapters that honour it get de-duped at the source. The key is **byte-identical to the Rust implementation** — a cross-runtime retry replays safely.

## API reference (key exports)

From `diff-fusion`:

- **Tier 0 facade**: `DiffFusion`, `Conflict`, `ConflictReport`, `CompareResult`.
- **Tier 1 facade**: `SyncEngine`, `SyncEngineBuilder`, `SyncOutcome`, `FacadeConflict`, `FacadePreview`.

Deeper layers — import from sub-paths. Only reach for these when the facades don't cover your case:

- **Policies**: `src/application/policy/{additive,append,ownedBy,stateMachine,escapeHatch,structural,declaration,invariants}.ts`.
- **Ports**: `src/ports/{system,ancestor,escalation}.ts` — interfaces each adapter implements.
- **Adapters**: `src/adapters/{testMemory,inMemoryAncestor,inMemoryEscalation,filesystemAncestor}.ts`.
- **Orchestrator**: `src/application/orchestrator.ts` — the 7-step cycle the `SyncEngine` facade wraps.
- **Transform / compare primitives**: `src/application/transform.ts`, `src/domain/compare.ts`.
- **Domain types**: `src/domain/types.ts` (`JsonValue`, `CifFieldDefinition`, `cifFieldDefinition`, `SchemaFields`, `toJsonSchema`, `validateSchema`).
- **Idempotency**: `src/domain/idempotency.ts` (`idempotencyKey`, `idempotencyKeyHex`, `canonicalStringify`).

## Examples

Every example under `examples/` runs standalone via `npx tsx`:

```bash
npx tsx examples/facadeUsage.ts   # Tier-0 DiffFusion walkthrough
npx tsx examples/twoWaySync.ts    # Tier-1 SyncEngine end-to-end
npx tsx examples/libraryUsage.ts  # Direct-library API: transform, compare, schema builders
npx tsx examples/sourceOfTruth.ts # Owner-based conflict resolution
```

## CLI

The package ships a `diff-fusion` binary for quick two-way diffs from the shell:

```bash
npx diff-fusion diff path/to/a.json path/to/b.json \
  --schema path/to/schema.json \
  --format-a format_a \
  --format-b format_b
```

Matches the Rust CLI's subcommand shape. Run `npx diff-fusion --help` for the full option list.

## Development

```bash
cd ts
npm install
npm test            # vitest run (unit + integration + contract)
npm run typecheck   # tsc --noEmit
npm run build       # emit dist/
npm run lint        # biome check
```

Port-local conventions live in [`CLAUDE.md`](./CLAUDE.md) — `.js` import specifiers under NodeNext, discriminated unions with literal `kind` tags, `undefined` not `null`, `Map<K,V>` over POJOs when keys aren't fixed.

## Cross-runtime guarantees

The TS port commits to byte-identical output with Rust for:

- `idempotencyKeyHex(canonicalId, operation, payload)` — the BLAKE3 digest of a length-prefixed, canonical-JSON framing.
- `threeWayDiff` and `mergeField` — delegate to the same Rust-compiled WASM kernel every runtime shares, now checked against golden vectors on both success and error output, not just asserted by construction.
- Filesystem ancestor filenames — the on-disk layout produced by `adapters/filesystemAncestor.ts`.

Shared golden vectors live in `../../spec/vectors/` (`idempotency-vectors.json` for idempotency keys and canonical JSON, `kernel-vectors.json` for three-way diff and merge-field resolution), generated from the Rust side and round-tripped by the TS tests. If you need strict cross-runtime replay safety (queue a retry in Rust, finish it in TS or vice versa), these are the contract surfaces to trust.

See also the [Rust crate README](../README.md) for the original design notes.

## License

MIT — see [LICENSE](../LICENSE).
