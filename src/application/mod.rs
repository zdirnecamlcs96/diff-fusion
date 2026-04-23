//! Application layer — use cases that orchestrate domain primitives
//! across ports.
//!
//! Depends on: `domain`, `ports`. Must not depend on `adapters` or
//! `drivers`.

pub mod orchestrator;
pub mod policy;
pub mod presets;
pub mod transform;
