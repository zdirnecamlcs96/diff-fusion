---
layout: default
title: Getting started
nav_order: 2
---

# Getting started

## Install

Nothing is published to crates.io or npm yet. Rust and TypeScript both
install straight from the git repository; Go modules resolve from git
directly, no separate publish step needed there.

### Rust

```toml
[dependencies]
diff-fusion = { git = "https://github.com/zdirnecamlcs96/diff-fusion.git" }
serde_json = "1.0"
```

### TypeScript

```bash
npm install github:zdirnecamlcs96/diff-fusion
```

Requires Node ≥ 20, ESM-only, `"module": "NodeNext"` (or bundler
equivalent) in `tsconfig.json`.

### Go

```bash
go get github.com/zdirnecamlcs96/diff-fusion/sdk/golang
```

This pulls the kernel package only — see
[Three deliveries, one kernel]({{ site.baseurl }}/#three-deliveries-one-kernel)
for what that means.

## First diff

The Tier-0 `DiffFusion` facade transforms and compares — no reconciliation
machinery, no state.

A schema has two parts: `cif_schema` declares the canonical fields, and
`transformations` maps each source format's raw shape onto them, keyed by
format id. Each field mapping is a `source_path` (dotted paths like
`variants.0.price` reach into nested objects and arrays) plus a `type` for
automatic conversion.

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

`transform_and_compare` runs both transformations and diffs the results;
`report.conflicts` is the list of fields that differ, each with a `path`,
`old_value`, and `new_value`. No policies, no ancestor, no push — just
"what changed".

## First reconciliation

The Tier-1 `SyncEngine` adds the rest: a stored ancestor, per-field merge
policies, and pushes back to both sides. From
`src/src/drivers/sync_engine.rs`:

```rust
let a = TestMemoryAdapter::new("erp");
let b = TestMemoryAdapter::new("inv");
a.seed("e", "1", json!({"q": 11}));
b.seed("e", "1", json!({"q": 12}));

let engine = SyncEngine::builder(a, b)
    .policy("q", Box::new(Additive))
    .seed_ancestor("e", "1", json!({"q": 10}))
    .build();

let out = engine.sync("e", "1").await.unwrap();
assert!(matches!(out, CycleOutcome::Synced { .. }));
```

`SyncEngine::builder(side_a, side_b)` takes two `SystemPort` adapters.
`.policy(path, Box::new(Additive))` declares a merge policy per canonical
field — here, `Additive` accumulates both sides' deltas on `"q"`.
`.seed_ancestor(entity_type, canonical_id, value)` primes the ancestor
store for a fresh entity (in real use, the store fills in from prior
cycles). `.build()` assembles the orchestrator.

`.sync(entity_type, canonical_id).await` runs one full cycle — pull both
sides, diff against the ancestor, resolve policies, push stale sides,
commit the ancestor — and returns a `CycleOutcome`: `Synced { pushed_to }`
on success, `NoOp` if nothing changed, or an escalation if a conflict
couldn't be resolved.

## Preview before you push

Call `.preview()` instead of `.sync()` to run the same pull-diff-resolve
pipeline without pushing or advancing the ancestor — shadow mode. Use it
to validate a new adapter against real systems before letting it write.
{: .note }

## Next steps

- [Concepts]({{ site.baseurl }}/concepts) — the vocabulary, the three-way diff, merge policy tiers, idempotency.
- [Two-way sync walkthrough]({{ site.baseurl }}/guides/two-way-sync) — the full example end to end, in Rust and TypeScript.
- [Writing an adapter]({{ site.baseurl }}/guides/writing-an-adapter) — implementing `SystemPort` against a real system.
- [Architecture]({{ site.baseurl }}/reference/architecture) — the full layer map and cycle ordering rules.
