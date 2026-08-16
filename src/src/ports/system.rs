//! `SystemPort` — the seam between the orchestrator and any single external
//! system.
//!
//! Every system (ERP, internal service, third-party API) implements the same
//! trait. Code above the port never branches on system identity; per-system
//! quirks are hidden inside the adapter.
//!
//! # Canonical-only
//!
//! The trait deals exclusively in canonical JSON values. Bidirectional
//! transformation (external shape ↔ canonical) is the adapter's private
//! concern. This keeps the orchestrator format-agnostic.
//!
//! # Optimistic concurrency
//!
//! Every [`SystemPort::upsert`] takes an `expect_version`. If the external
//! system moved since the orchestrator last fetched, the adapter returns
//! [`SyncError::StaleWrite`] and the cycle restarts. Adapters against
//! systems without native OCC fake it with a read-before-write check.
//!
//! # Idempotency
//!
//! Every upsert takes a deterministic 32-byte idempotency key
//! (see [`crate::idempotency`]). Adapters must forward it to the external
//! system's idempotency mechanism when supported, or maintain their own
//! dedup table when not.

use crate::domain::error::SyncError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Identifies an entity in an external system.
///
/// `version` is whatever the external system uses to detect concurrent
/// modifications — a revision number, an HTTP ETag, a commit hash. It's
/// opaque to the orchestrator; only the adapter knows how to compare them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExternalRef {
    pub system: String,
    pub external_id: String,
    pub version: Option<String>,
}

impl ExternalRef {
    pub fn new(
        system: impl Into<String>,
        external_id: impl Into<String>,
        version: Option<String>,
    ) -> Self {
        Self {
            system: system.into(),
            external_id: external_id.into(),
            version,
        }
    }
}

/// The adapter trait. Implementors are expected to be cheap to clone
/// (usually `Arc<Inner>`) so the orchestrator can hold one across cycles.
#[async_trait]
pub trait SystemPort: Send + Sync {
    /// Stable identifier for this system — used in logs, policy contexts,
    /// and `ExternalRef::system`.
    fn system_type(&self) -> &str;

    /// Fetch the current canonical view for the given external ref.
    async fn fetch(
        &self,
        entity_type: &str,
        ext: &ExternalRef,
    ) -> Result<(Value, ExternalRef), SyncError>;

    /// Reverse lookup: find the external ref for a canonical id.
    ///
    /// This is the "findByCanonicalId" method whose absence causes most of
    /// the duplicate-record class of bugs — when a webhook fires mid-cycle
    /// and the adapter can't tell whether the entity already exists
    /// externally, it creates a second record.
    async fn find_by_canonical_id(
        &self,
        entity_type: &str,
        canonical_id: &str,
    ) -> Result<Option<ExternalRef>, SyncError>;

    /// Upsert canonical state. Returns the new [`ExternalRef`] (with the
    /// post-write version). The `idempotency_key` is supplied by the
    /// orchestrator via [`crate::domain::idempotency::idempotency_key`].
    ///
    /// `expect_version` is the orchestrator's assertion about the version it
    /// last saw. When it mismatches, adapters MUST return
    /// [`SyncError::StaleWrite`] rather than silently overwriting.
    async fn upsert(
        &self,
        entity_type: &str,
        canonical_id: &str,
        canonical: &Value,
        expect_version: Option<&str>,
        idempotency_key: &[u8; 32],
    ) -> Result<ExternalRef, SyncError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_ref_basics() {
        let r = ExternalRef::new("netsuite", "PO-42", Some("v7".into()));
        assert_eq!(r.system, "netsuite");
        assert_eq!(r.external_id, "PO-42");
        assert_eq!(r.version.as_deref(), Some("v7"));
    }
}
