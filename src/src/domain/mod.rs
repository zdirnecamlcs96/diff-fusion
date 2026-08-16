//! Domain layer — pure computation, no I/O, no async.
//!
//! Every other layer may depend on this one; this one depends on none of
//! them. Contents: error categories, idempotency key computation, two-way
//! and three-way diff primitives.

pub mod compare;
pub mod diff;
pub mod error;
pub mod idempotency;
pub mod json_path;
