//! Diff primitives.
//!
//! Two-way diff lives in [`crate::compare`] (the existing `git diff` style
//! comparator). Three-way diff is the reconciliation-grade primitive that
//! also records *which side changed* — the missing signal that lets the
//! orchestrator distinguish "A moved" from "both moved" without falling back
//! to wall-clock timestamps.

pub mod three_way;

pub use three_way::{ChangeSource, Changelog, FieldChange, three_way_diff};
