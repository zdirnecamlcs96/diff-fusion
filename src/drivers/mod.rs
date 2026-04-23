//! Drivers layer — user-facing entry points.
//!
//! The outermost ring: CLI, programmatic facades, anything that wires
//! together application use cases and adapters for end-user consumption.
//! Depends on every inner layer.

pub mod cli;
pub mod facade;
pub mod sync_engine;
