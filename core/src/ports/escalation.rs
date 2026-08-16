//! `EscalationQueue` port — where unresolved conflicts go for human review.
//!
//! App.md § 03 is explicit: ~5% of conflicts cannot be auto-resolved, and
//! throwing them away (or silently picking a winner) is worse than not
//! reconciling at all. The orchestrator routes these to an
//! [`EscalationQueue`] with full provenance so a reviewer can see
//! *exactly* what each side claimed and choose.
//!
//! This module defines the interface only. Concrete queues live in
//! [`crate::adapters`] — an in-memory reference impl is available at
//! [`crate::adapters::in_memory_escalation::InMemoryEscalationQueue`].

use crate::application::policy::UnresolvedConflict;

/// One pending escalation. Carries the full [`UnresolvedConflict`] list so
/// the reviewer has provenance for every disputed field.
#[derive(Debug, Clone, PartialEq)]
pub struct EscalationItem {
    pub entity_type: String,
    pub canonical_id: String,
    pub conflicts: Vec<UnresolvedConflict>,
    pub created_at_ms: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum EscalationError {
    #[error("escalation queue backend failure: {0}")]
    Backend(String),
}

pub trait EscalationQueue: Send + Sync {
    fn push(&self, item: EscalationItem) -> Result<(), EscalationError>;
    fn len(&self) -> usize;
}
