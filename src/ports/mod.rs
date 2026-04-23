//! Ports layer — abstract interfaces at the boundary between the
//! application and concrete external systems or persistence backends.
//!
//! Depends on: `domain` only. Concrete implementations live in
//! [`crate::adapters`].

pub mod ancestor;
pub mod escalation;
pub mod system;
