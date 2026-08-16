---
layout: default
title: Writing an adapter
parent: Guides
nav_order: 2
---

# Writing an adapter

An adapter connects one external system to the reconciliation pipeline by implementing `SystemPort` (`src/src/ports/system.rs`). The orchestrator never branches on which system it's talking to — every per-system quirk (auth, pagination, native vs. faked optimistic concurrency) stays inside the adapter.

The trait:

```rust
#[async_trait]
pub trait SystemPort: Send + Sync {
    fn system_type(&self) -> &str;

    async fn fetch(
        &self,
        entity_type: &str,
        ext: &ExternalRef,
    ) -> Result<(Value, ExternalRef), SyncError>;

    async fn find_by_canonical_id(
        &self,
        entity_type: &str,
        canonical_id: &str,
    ) -> Result<Option<ExternalRef>, SyncError>;

    async fn upsert(
        &self,
        entity_type: &str,
        canonical_id: &str,
        canonical: &Value,
        expect_version: Option<&str>,
        idempotency_key: &[u8; 32],
    ) -> Result<ExternalRef, SyncError>;
}
```

`src/src/adapters/test_memory.rs`'s `TestMemoryAdapter` is the reference implementation — small enough to read in full, but it exercises every contract point below. It backs the two-way sync example in the [previous guide]({{ site.baseurl }}/guides/two-way-sync).

## The methods

- **`system_type`** — a stable label (`"erp"`, `"netsuite"`, …) used in logs, policy contexts (`OwnedBy` matches against it), and `ExternalRef::system`.
- **`fetch`** — given an `ExternalRef`, return the current canonical JSON view plus a fresh `ExternalRef` reflecting whatever version the system reports *now*. The trait deals exclusively in canonical values — mapping the external system's native shape to canonical JSON is the adapter's private concern, so the orchestrator stays format-agnostic.
- **`find_by_canonical_id`** — reverse lookup from your domain's stable id to this system's `ExternalRef`, or `None` if the entity doesn't exist there yet. This is the method whose absence causes most duplicate-record bugs: without it, a webhook firing mid-cycle can't be told apart from "entity doesn't exist yet", and the adapter creates a second record instead of finding the first.
- **`upsert`** — write canonical state and return the new `ExternalRef` (with the post-write version). Two obligations come with this call — see below.

## `ExternalRef`

```rust
pub struct ExternalRef {
    pub system: String,
    pub external_id: String,
    pub version: Option<String>,
}
```

`version` is whatever the external system uses to detect concurrent writes — a revision counter, an HTTP ETag, a commit hash. It's opaque to the orchestrator; only the adapter that produced it knows how to compare two of them. `TestMemoryAdapter` uses a monotonic decimal counter (`"1"`, `"2"`, …).

## Optimistic concurrency: honour `expect_version`

`upsert`'s `expect_version` is the orchestrator's assertion about the version it last saw. If the external system moved since then, the adapter **must** return `SyncError::StaleWrite` rather than silently overwriting — the orchestrator restarts the cycle on that error rather than clobbering a concurrent change. `TestMemoryAdapter` enforces this with a plain equality check against its stored version:

```rust
// OCC: if caller supplied an expected version, it must match.
if let Some(expect) = expect_version
    && expect != existing.version.to_string()
{
    return Err(SyncError::stale(
        &self.system_type,
        Some(existing.version.to_string()),
        format!(
            "version mismatch: caller expected {expect}, current {}",
            existing.version
        ),
    ));
} else if expect_version.is_some() {
    // Caller expected an existing record but none exists.
    return Err(SyncError::stale(
        &self.system_type,
        None,
        "caller supplied expect_version but no existing record",
    ));
}
```

If your target system has native OCC (an ETag, a `rev` field, `If-Match`), forward `expect_version` to it and translate its concurrency error into `SyncError::StaleWrite`. If it doesn't, fake it the way `TestMemoryAdapter` does: read the current version under a lock and compare before writing.

## Idempotency: dedup on `idempotency_key`

Every `upsert` carries a deterministic 32-byte key (`idempotency_key(canonical_id, operation, &canonical)`, from `crate::domain::idempotency`). A repeat call with the **same key** must not double-apply — the orchestrator relies on this to make retries safe after a crash or timeout between "push succeeded" and "orchestrator found out". `TestMemoryAdapter` tracks the last key it saw per entity and short-circuits before touching state:

```rust
// Idempotency: repeat upsert with the same key is a no-op.
if existing.last_idempotency_key.as_ref() == Some(idempotency_key) {
    return Ok(self.make_ref(&existing.external_id, existing.version));
}
```

If the target system has a native idempotency mechanism (Stripe-style `Idempotency-Key` header, a dedup table keyed by request id), forward the key to it. If not, maintain your own dedup table the way `TestMemoryAdapter` does — one key per entity is enough since keys are derived from `(canonical_id, operation, payload)` and change whenever the payload does.

Both checks matter together: idempotency is checked *before* the version check in `TestMemoryAdapter::upsert`, so a safe retry with a stale-looking `expect_version` (because the retry is replaying a write the caller already believes succeeded) still returns cleanly instead of erroring.

## Other ports you may want to implement

`SystemPort` is the only port every sync needs two of. Three more ports round out a deployment, each with a working in-memory or filesystem default so you only implement your own when you need durability or a real destination:

- **`AncestorStore`** (`src/src/ports/ancestor.rs`) — `get`/`put` for the last-synced canonical view, keyed by `(entity_type, canonical_id)`. `SyncEngine` defaults to an in-memory store; `src/src/adapters/filesystem_ancestor.rs`'s `FilesystemAncestorStore` ships as a durable example — one JSON file per entity under a root directory, written via temp-file-then-rename so a crash mid-write can't leave a half-written ancestor on disk.
- **`EscalationQueue`** (`src/src/ports/escalation.rs`) — `push`/`len` for conflicts that survive resolution. `SyncEngine` defaults to an in-memory queue; a real deployment usually wants this backed by something a reviewer can see (a database table, a ticket queue).
- **`Observer`** (`src/src/ports/observer.rs`) — a passive sink that receives a `Capture` (both sides' canonical views for one entity) for logging or visualization. It has no transport dependencies and doesn't participate in the merge pipeline — implement it to ship snapshots to your own tooling.
