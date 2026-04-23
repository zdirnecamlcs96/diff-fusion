//! Adapters layer — concrete implementations of [`crate::ports`] traits.
//!
//! Every external integration (REST API, database, queue, etc.) lives
//! here as an adapter that implements one or more port traits.
//!
//! Depends on: `domain`, `ports`. Must not depend on `application` or
//! `drivers`.

pub mod filesystem_ancestor;
pub mod in_memory_ancestor;
pub mod in_memory_escalation;
pub mod test_memory;
