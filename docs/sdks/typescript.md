---
layout: default
title: TypeScript
parent: SDKs
nav_order: 2
---

# TypeScript

The full library — same layers as Rust (`domain / application / ports / adapters / drivers`), same file-for-file module tree — with the kernel delivered as vendored WASM instead of a hand-written TS port. Full generated API docs: [typedoc](/diff-fusion/api/ts/).

## Install

Not published to npm yet — install from the git repo:

```bash
npm install github:zdirnecamlcs96/diff-fusion
```

**Requirements:** Node ≥ 20, ESM-only. Your `tsconfig.json` needs `"module": "NodeNext"` (or a bundler equivalent) — the package ships ESM with `.js` import specifiers.

## Kernel is vendored — no Rust toolchain needed

Core semantics (three-way diff, canonical JSON, idempotency keys, built-in policy resolution) are computed by the Rust `diff-fusion` crate, compiled to WASM and vendored at `sdk/typescript/wasm/` (`diff_fusion_bg.wasm`). Consuming the package needs nothing beyond Node — no Rust toolchain, no build step. Only `src/kernel.ts` inside the package touches the wasm module; you never call it directly.

Kernel output is checked against the golden vectors in `spec/vectors/` — `kernel-vectors.json` for three-way diff and merge-field resolution, `idempotency-vectors.json` for canonical JSON and idempotency keys — and the TS test suite asserts byte-exact agreement with Rust for all four kernel functions, including error strings.

## Barrel exports

The published package root re-exports only the two facade surfaces:

- **Tier 0**: `DiffFusion`, `Conflict`, `ConflictReport`, `CompareResult`.
- **Tier 1**: `SyncEngine`, `SyncEngineBuilder`, `SyncOutcome`, `FacadeConflict`, `FacadePreview`.

```ts
import { DiffFusion, SyncEngine } from "diff-fusion";
```

That's enough for the [getting-started]({{ site.baseurl }}/getting-started) walkthroughs and the [two-way sync guide]({{ site.baseurl }}/guides/two-way-sync) — both facades cover policies, adapters, and outcomes through their own method signatures without needing a deeper import.

## Alpha caveat: deeper layers aren't on the public barrel yet

Policies (`Additive`, `OwnedBy`, `StateMachine`, …) and adapters (`TestMemoryAdapter`, …) live in the package's source tree but are **not** exported from the package root, and the package's `exports` field in `package.json` only declares `"."` — so a deep import against the installed package (e.g. `diff-fusion/dist/application/policy/additive.js`) is blocked by Node's `exports` resolution, not just undocumented.
{: .warning }

Per the package README: *"For now, clone the repo and import via relative paths, as the shipped `examples/` do."* Concretely, `sdk/typescript/examples/twoWaySync.ts` imports straight from source:

```ts
import { Additive } from "../src/application/policy/additive.js";
import { OwnedBy } from "../src/application/policy/ownedBy.js";
import {
  StateMachine,
  StateTransition,
} from "../src/application/policy/stateMachine.js";
import { TestMemoryAdapter } from "../src/adapters/testMemory.js";
```

Until a later phase expands the barrel, working with policies or adapters means either cloning `sdk/typescript/` and importing its `src/` directly (as above), or waiting for the export surface to widen. The full list of what lives where:

- **Policies**: `src/application/policy/{additive,append,ownedBy,stateMachine,escapeHatch,structural,declaration,invariants}.ts`
- **Ports**: `src/ports/{system,ancestor,escalation}.ts`
- **Adapters**: `src/adapters/{testMemory,inMemoryAncestor,inMemoryEscalation,filesystemAncestor}.ts`
- **Orchestrator**: `src/application/orchestrator.ts`
- **Transform / compare primitives**: `src/application/transform.ts`, `src/domain/compare.ts`
- **Domain types**: `src/domain/types.ts`
- **Idempotency**: `src/domain/idempotency.ts`

All paths above are relative to `sdk/typescript/`.

## `set_by_key` policy declarations can't reach `Union`/`PreferA`/`PreferB`

The JSON shape `declaration.ts` accepts for `set_by_key` (used by schema-driven `.policy()` declarations, mirroring the Rust wire constructor) only carries `identity`, `a_anchor`, and `b_anchor`. The `SetByKey` class itself also has an `onBothChanged` field (`"Escalate" | "PreferA" | "PreferB" | "Union"`, default `"Escalate"`), but nothing in the JSON declaration path can set it to anything but the default. To reach the other variants, construct `SetByKey` directly and set `onBothChanged` on the instance before passing it to `.policy(...)` — the JSON declaration path can't express it. This is current behavior, not a guarantee — an open design question, not yet decided.
{: .warning }
