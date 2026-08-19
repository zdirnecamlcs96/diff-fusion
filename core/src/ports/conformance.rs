//! Conformance harness for [`SystemPort`] implementations.
//!
//! `core/tests/integration/contract_tests.rs` already exercises the
//! reference adapter, but a `tests/` file isn't part of the published
//! crate — an out-of-tree adapter crate can't call into it. This module is
//! library code so any `SystemPort` implementor can assert the same
//! contract from its own test suite.
//!
//! The trait speaks CIF only (see the [`crate::ports::system`] module
//! docs' "Canonical-only" section): `upsert` receives a document
//! containing just the fields the two systems share. Fields the CIF
//! mapping never sees are per-system local state, and `upsert` must
//! read-modify-write the shared paths onto the existing native record
//! rather than replace it wholesale — otherwise a push silently deletes
//! native-only data. Checking that needs a peek at the adapter's native
//! storage that the port itself never exposes, hence [`RawAccess`]: a tiny
//! side door the adapter's own test supplies.

use crate::domain::error::SyncError;
use crate::domain::idempotency::idempotency_key;
use crate::ports::system::{ExternalRef, SystemPort};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Adapter-supplied side door into native storage, bypassing the CIF-only
/// `SystemPort` surface. Only needed for the preservation check (C1) —
/// the other checks drive the port directly.
#[async_trait]
pub trait RawAccess {
    /// Seed a raw native record directly in the adapter's backing store,
    /// bypassing `upsert`. `native` may contain fields the CIF mapping
    /// never touches — the harness uses that to prove a push doesn't
    /// clobber them.
    async fn seed_raw(&self, entity_type: &str, canonical_id: &str, native: Value) -> ExternalRef;

    /// Read the native record back, bypassing whatever CIF projection the
    /// port applies on `fetch`.
    async fn read_raw(&self, entity_type: &str, canonical_id: &str) -> Value;
}

/// Assert a [`SystemPort`] implementation honors the trait's documented
/// contract. Panics (with a message naming the violated rule) on the first
/// failing check.
pub async fn assert_system_port_contract<P: SystemPort, R: RawAccess>(port: &P, raw: &R) {
    assert_c1_preservation(port, raw).await;
    assert_c2_version_guard(port).await;
    assert_c3_idempotency(port).await;
}

/// C1 — preservation. `upsert` must update the shared CIF paths on the
/// existing native record and leave everything else alone (read-modify-
/// write, not replace) — see this module's doc comment.
async fn assert_c1_preservation<P: SystemPort, R: RawAccess>(port: &P, raw: &R) {
    let entity_type = "conformance_entity";
    let canonical_id = "conformance-c1";
    let seeded = raw
        .seed_raw(
            entity_type,
            canonical_id,
            json!({"shared": "old", "native_only": "keep-me"}),
        )
        .await;

    let cif = json!({"shared": "new"});
    let ik = idempotency_key(canonical_id, "upsert", &cif);
    port.upsert(entity_type, canonical_id, &cif, seeded.version.as_deref(), &ik)
        .await
        .expect("C1 preservation: upsert of a shared-only CIF document must succeed");

    let after = raw.read_raw(entity_type, canonical_id).await;
    assert_eq!(
        after.get("shared"),
        Some(&json!("new")),
        "C1 preservation violated: upsert must update the shared path it was given"
    );
    assert_eq!(
        after.get("native_only"),
        Some(&json!("keep-me")),
        "C1 preservation violated: upsert must read-modify-write the native record, \
         not replace it — a native-only field vanished after a push"
    );
}

/// C2 — version guard. The trait's "Optimistic concurrency" docs promise a
/// mismatched `expect_version` returns [`SyncError::StaleWrite`] rather
/// than silently overwriting, so every `SystemPort` must honor it — this
/// check always runs (no skip condition).
async fn assert_c2_version_guard<P: SystemPort>(port: &P) {
    let entity_type = "conformance_entity";
    let canonical_id = "conformance-c2";
    let cif = json!({"shared": "v1"});
    let ik1 = idempotency_key(canonical_id, "upsert", &cif);
    let created = port
        .upsert(entity_type, canonical_id, &cif, None, &ik1)
        .await
        .expect("C2 version guard: initial upsert must succeed");

    let stale = format!("stale-{}", created.version.as_deref().unwrap_or("none"));
    let cif2 = json!({"shared": "v2"});
    let ik2 = idempotency_key(canonical_id, "upsert", &cif2);
    let err = port
        .upsert(entity_type, canonical_id, &cif2, Some(&stale), &ik2)
        .await
        .expect_err(
            "C2 version guard violated: upsert with a mismatched expect_version \
             must be rejected, not silently applied",
        );
    assert!(
        matches!(err, SyncError::StaleWrite { .. }),
        "C2 version guard violated: expected SyncError::StaleWrite on version mismatch, got {err:?}"
    );
}

/// C3 — idempotency. The trait's "Idempotency" docs require adapters to
/// dedup a repeated `idempotency_key` (via the external system's mechanism
/// or a table of their own), so every `SystemPort` must honor it — this
/// check always runs (no skip condition).
async fn assert_c3_idempotency<P: SystemPort>(port: &P) {
    let entity_type = "conformance_entity";
    let canonical_id = "conformance-c3";
    let cif = json!({"shared": "same"});
    let ik = idempotency_key(canonical_id, "upsert", &cif);

    let r1 = port
        .upsert(entity_type, canonical_id, &cif, None, &ik)
        .await
        .expect("C3 idempotency: first upsert must succeed");
    let r2 = port
        .upsert(entity_type, canonical_id, &cif, None, &ik)
        .await
        .expect("C3 idempotency: repeat upsert with same idempotency key must succeed");

    assert_eq!(
        r1, r2,
        "C3 idempotency violated: a repeat upsert with the same idempotency key \
         must be a no-op, not create a new write"
    );
}
