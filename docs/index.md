---
layout: home
title: Home
nav_order: 1
permalink: /
---

# diff-fusion
{: .fs-9 .fw-700 }

A Rust library for two-way reconciliation between authoritative systems.
{: .fs-6 .fw-300 }

[Get started]({{ site.baseurl }}/getting-started){: .btn .btn-primary .mr-2 }
[View on GitHub](https://github.com/zdirnecamlcs96/diff-fusion){: .btn }

---

## The problem it solves

Integrating *n* systems pairwise needs n(n-1)/2 transformers — every new
system means updating every other transformer, and breaking changes
cascade across all of them. Transforming each system to a common
intermediate format (CIF) instead needs only *n* transformers: one per
system, in and out.

That solves the "different shapes" half of the problem. The harder half
shows up once both sides can change: a last-write-wins overwrite silently
destroys whichever edit lost the race. diff-fusion instead computes a
three-way diff against a stored ancestor, so it knows which side moved —
one, the other, or both — and can resolve that per field instead of
guessing.

## How it works

```
System A ─▶ Canonical ─┐
                       ├─ Three-way diff (A, B, ancestor)
System B ─▶ Canonical ─┘          │
                                  ▼
                              Resolve per-field policies
                                  │
                                  ├── clean: push stale side(s), commit ancestor LAST
                                  └── conflicts: route to escalation queue
```

## See it

The detection-only Tier-0 facade, `DiffFusion`, needs no reconciliation
machinery — define a schema once, transform two source shapes into it, and
diff them:

```rust
use diff_fusion::DiffFusion;
use serde_json::json;

let schema = json!({
    "cif_schema": {
        "product_id": {"type": "string", "required": true},
        "price": {"type": "number", "required": true}
    },
    "transformations": {
        "salesforce": {
            "product_id": {"source_path": "Id", "type": "string"},
            "price": {"source_path": "Price__c", "type": "number"}
        },
        "shopify": {
            "product_id": {"source_path": "id", "type": "string"},
            "price": {"source_path": "variants.0.price", "type": "number"}
        }
    }
});

let diff_fusion = DiffFusion::new(schema);
let salesforce_data = json!({"Id": "SF-001", "Price__c": 29.99});
let shopify_data = json!({"id": "SH-001", "variants": [{"price": 34.99}]});

let report = diff_fusion.transform_and_compare(
    &salesforce_data, "salesforce",
    &shopify_data, "shopify"
).unwrap();

for conflict in report.conflicts {
    println!("{} differs: {} vs {}",
        conflict.path, conflict.old_value, conflict.new_value);
}
```

This prints one line per field that differs between the two CIF-mapped
records — here, both `product_id` (`SF-001` vs `SH-001`) and `price`
(`29.99` vs `34.99`).

## What you get

- Three-way diff with `source: A | B | Both` per field
- Declarative merge policies (`OwnedBy`, `Additive`, `Append`, `StateMachine`, `SetByKey`)
- Optimistic concurrency + idempotency keys on every push
- Escalation queue for the ~5% of conflicts that require human judgement
- Shadow mode — diff without pushing, for validating new adapters
- Conflict taxonomy (`NoPolicy` / `PolicyConflict` / `InvariantViolation`) so callers can branch disposition per class
- Durable filesystem-backed ancestor store (`adapters::filesystem_ancestor`)
- `SyncEngine` facade — one builder, no `Arc<dyn …>` ceremony; full layered module tree (`domain / application / ports / adapters / drivers`)
- The detection-only facade (`DiffFusion`) stays available as a Tier-0 entry point
- Passive `Observer` hook (`ports::observer`) for streaming pipeline events to any capture endpoint

## Three deliveries, one kernel

| Package | Install | What you get |
|---|---|---|
| Rust (`diff-fusion`) | `diff-fusion = { git = "https://github.com/zdirnecamlcs96/diff-fusion.git" }` | Full library — all five layers (`domain / application / ports / adapters / drivers`) |
| TypeScript (`diff-fusion-ts`) | `npm install github:zdirnecamlcs96/diff-fusion` | Full library; kernel computed via WASM (the Rust crate compiled with wasm-bindgen) |
| Go (`sdk/golang`) | `go get github.com/zdirnecamlcs96/diff-fusion/sdk/golang` | Kernel only — four functions (`ThreeWayDiff`, `MergeField`, `CanonicalJSON`, `IdempotencyKeyHex`); the app layer is native per host |

## What this is not

- **Not a workflow engine** — it reconciles state, it doesn't orchestrate multi-step business processes.
- **Not a real-time event bus** — it batches in convergence windows, not sub-second propagation.
- **Not a generic ETL/integration platform** — one-way sync and data pipelines aren't goals.
- **Not a CRDT** — merge semantics are policy-based; genuine conflicts escalate to a human instead of resolving automatically.

## Next

- [Getting started]({{ site.baseurl }}/getting-started)
- [Concepts]({{ site.baseurl }}/concepts)
- [Guides]({{ site.baseurl }}/guides) — two-way sync walkthrough, writing an adapter
- [SDKs]({{ site.baseurl }}/sdks) — Rust, TypeScript, Go
- [Rust API](/diff-fusion/api/rust/)
