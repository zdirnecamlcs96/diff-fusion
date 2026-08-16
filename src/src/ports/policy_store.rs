//! `PolicyStore` port — trait + error type.
//!
//! A host-backed alternative to the hand-coded `PolicyMap` an
//! [`crate::application::orchestrator::Orchestrator`] is built with:
//! [`PolicyStore::load`] returns a fresh [`PolicyMap`] once per cycle,
//! built from a [`crate::application::policy::PolicyDocument`] the host
//! read out of its own database. Storage-neutral by construction — no row
//! id, table/collection name, column, cursor, or pagination in the
//! signature.
//!
//! This module defines the interface only. Concrete stores live in
//! [`crate::adapters`] — an in-memory reference impl is available at
//! [`crate::adapters::in_memory_policy_store::InMemoryPolicyStore`].

use crate::application::policy::PolicyMap;

/// Errors the store may raise. Kept narrow on purpose — real persistence
/// adapters can wrap I/O or deserialize failures as
/// [`crate::domain::error::SyncError::Transient`].
#[derive(Debug, thiserror::Error)]
pub enum PolicyStoreError {
    #[error("policy store backend failure: {0}")]
    Backend(String),
}

/// Load interface for policy documents. Synchronous — persistence
/// adapters that need I/O should either use blocking calls or wrap an
/// async runtime, matching [`crate::ports::ancestor::AncestorStore`].
pub trait PolicyStore: Send + Sync {
    /// Load the whole policy map for one entity type. No document for
    /// `entity_type` is not an error — return `Ok(PolicyMap::default())`
    /// so every changed field escalates as `NoPolicy` rather than guessing.
    fn load(&self, entity_type: &str) -> Result<PolicyMap, PolicyStoreError>;
}
