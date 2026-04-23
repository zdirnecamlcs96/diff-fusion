//! `AncestorStore` port — trait + shared types.
//!
//! The ancestor is the last-synced canonical view of an entity. Every
//! completed sync cycle advances it *after* all pushes confirm — never
//! before. Without it, three-way diff cannot distinguish "A changed"
//! from "both changed" and silent overwrites become possible.
//!
//! This module defines the interface only. Concrete stores live in
//! [`crate::adapters`] — an in-memory reference impl is available at
//! [`crate::adapters::in_memory_ancestor::InMemoryAncestorStore`].

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Composite key for an ancestor entry.
///
/// `entity_type` keeps different canonical shapes in the same store from
/// colliding (e.g. a `PurchaseOrder` with the same id as an `InventoryItem`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AncestorKey {
    pub entity_type: String,
    pub canonical_id: String,
}

impl AncestorKey {
    pub fn new(entity_type: impl Into<String>, canonical_id: impl Into<String>) -> Self {
        Self {
            entity_type: entity_type.into(),
            canonical_id: canonical_id.into(),
        }
    }
}

/// One stored ancestor — the canonical view last confirmed on both sides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AncestorEntry {
    /// The canonical JSON at last sync. This is what three-way diff compares
    /// both current views against.
    pub canonical: Value,

    /// Milliseconds since unix epoch. The caller supplies this so tests and
    /// deterministic replays can seed fixed timestamps.
    pub updated_at_ms: u64,
}

impl AncestorEntry {
    pub fn new(canonical: Value, updated_at_ms: u64) -> Self {
        Self {
            canonical,
            updated_at_ms,
        }
    }
}

/// Errors the store may raise. Kept narrow on purpose — real persistence
/// adapters can wrap I/O failures as
/// [`crate::domain::error::SyncError::Transient`].
#[derive(Debug, thiserror::Error)]
pub enum AncestorStoreError {
    #[error("ancestor store backend failure: {0}")]
    Backend(String),
}

/// Read/write interface for ancestors. Synchronous — persistence adapters
/// that need I/O should either use blocking calls or wrap an async runtime.
pub trait AncestorStore: Send + Sync {
    fn get(&self, key: &AncestorKey) -> Result<Option<AncestorEntry>, AncestorStoreError>;

    fn put(&self, key: AncestorKey, entry: AncestorEntry) -> Result<(), AncestorStoreError>;

    fn delete(&self, key: &AncestorKey) -> Result<(), AncestorStoreError>;
}
