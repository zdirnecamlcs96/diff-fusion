---
layout: default
title: Rust
parent: SDKs
nav_order: 1
---

# Rust

The kernel and the full library, in one crate at `core/`. Full generated API docs: [rustdoc](/diff-fusion/api/rust/).

## Install

Not on crates.io yet — depend on the git repo:

```toml
[dependencies]
diff-fusion = { git = "https://github.com/zdirnecamlcs96/diff-fusion.git" }
serde_json = "1.0"
```

## Layer map

From the crate's module doc (`core/src/lib.rs`):

- `domain` — pure computation: error categories, diffs, CIF types, idempotency keys
- `application` — use cases: orchestrator, policies, schema transformation
- `ports` — abstract interfaces: `SystemPort`, `AncestorStore`, `EscalationQueue`
- `adapters` — concrete implementations of the port traits
- `drivers` — user-facing entry points: `SyncEngine`, `DiffFusion`, CLI

The dependency rule points inward: `domain ← application ← drivers` and `domain ← ports ← adapters ← drivers`. Nothing in an inner ring imports from an outer ring.

## Two facades

### `DiffFusion` — Tier 0, detection only

`core/src/drivers/facade.rs`. Transform JSON to a common shape and compare two CIF values — no policies, no state, no I/O.

```rust
pub fn new(schema: Value) -> Self
pub fn transform(&self, source: &Value, format_id: &str) -> Result<Value, Box<dyn Error>>
pub fn compare(&self, old: &Value, new: &Value) -> ConflictReport
pub fn transform_and_compare(
    &self,
    source_a: &Value, format_a: &str,
    source_b: &Value, format_b: &str,
) -> Result<ConflictReport, Box<dyn Error>>
pub fn schema(&self) -> &Value
pub fn validate_cif(&self, value: &Value) -> Result<(), Vec<String>>
```

### `SyncEngine` — Tier 1, full reconciliation

`core/src/drivers/sync_engine.rs`. Builder-configured, hides the ancestor store / escalation queue / orchestrator wiring behind one chain of calls. See the [two-way sync guide]({{ site.baseurl }}/guides/two-way-sync) for a full walkthrough.

```rust
// SyncEngine
pub fn builder(side_a: A, side_b: B) -> SyncEngineBuilder<A, B>
pub async fn sync(&self, entity_type: &str, canonical_id: &str) -> Result<CycleOutcome, SyncError>
pub async fn preview(&self, entity_type: &str, canonical_id: &str) -> Result<ShadowReport, SyncError>
pub fn escalation_depth(&self) -> usize

// SyncEngineBuilder
pub fn policy(self, path: impl Into<String>, policy: Box<dyn MergePolicy>) -> Self
pub fn invariant(self, invariant: Box<dyn Invariant>) -> Self
pub fn one_way(self) -> Self
pub fn ancestor_store(self, store: Arc<dyn AncestorStore>) -> Self
pub fn escalation_queue(self, queue: Arc<dyn EscalationQueue>) -> Self
pub fn seed_ancestor(self, entity_type: impl Into<String>, canonical_id: impl Into<String>, canonical: Value) -> Self
pub fn validate_against_schema(self, schema: &Value) -> Result<Self, Vec<String>>
pub fn build(self) -> SyncEngine<A, B>
```

`one_way()` is a preset: `side_a` becomes the source of truth for any field not overridden by a subsequent `.policy(...)` call — target-side edits revert on the next cycle. `validate_against_schema` checks installed policies (e.g. `SetByKey` anchors, `nested` sub-policies) against a CIF schema before the first cycle runs, so misconfiguration fails at build time rather than at the first sync.

Both facades are re-exported at the crate root, so `diff_fusion::DiffFusion` and `diff_fusion::SyncEngine` work without the full module path.
