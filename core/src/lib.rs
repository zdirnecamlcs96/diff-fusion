//! # diff-fusion
//!
//! **A library for two-way reconciliation between authoritative systems.**
//!
//! Transform each system's JSON to a canonical format (CIF), compute a
//! three-way diff against a stored ancestor, resolve per-field merge
//! policies, and push the result back — with optimistic concurrency and
//! deterministic idempotency keys. Unresolvable conflicts route to an
//! escalation queue for human review.
//!
//! See `ARCHITECTURE.md` for the layered module layout, `App.md` for the
//! design rationale, and `New claude.md` for the rules of the road when
//! working in the code.
//!
//! ## Layer map
//!
//! - [`domain`]      — pure computation: error categories, diffs, CIF types, idempotency keys
//! - [`application`] — use cases: orchestrator, policies, schema transformation
//! - [`ports`]       — abstract interfaces: `SystemPort`, `AncestorStore`, `EscalationQueue`
//! - [`adapters`]    — concrete implementations of the port traits
//! - [`drivers`]     — user-facing entry points: `SyncEngine`, `DiffFusion`, CLI
//!
//! The dependency rule points inward: `domain ← application ← drivers`
//! and `domain ← ports ← adapters ← drivers`. Nothing in an inner ring
//! imports from an outer ring.
//!
//! ## Quickstart (detection-only, Tier-0 facade)
//!
//! ```
//! use diff_fusion::DiffFusion;
//! use serde_json::json;
//!
//! let schema = json!({
//!     "cif_schema": {
//!         "product_id": {"type": "string", "required": true},
//!         "price": {"type": "number", "required": true}
//!     },
//!     "transformations": {
//!         "salesforce": {
//!             "product_id": {"source_path": "Id", "type": "string"},
//!             "price": {"source_path": "Price__c", "type": "number"}
//!         },
//!         "shopify": {
//!             "product_id": {"source_path": "id", "type": "string"},
//!             "price": {"source_path": "variants.0.price", "type": "number"}
//!         }
//!     }
//! });
//!
//! let diff_fusion = DiffFusion::new(schema);
//! let salesforce_data = json!({"Id": "SF-001", "Price__c": 29.99});
//! let shopify_data = json!({"id": "SH-001", "variants": [{"price": 34.99}]});
//!
//! let report = diff_fusion.transform_and_compare(
//!     &salesforce_data, "salesforce",
//!     &shopify_data, "shopify"
//! ).unwrap();
//!
//! for conflict in report.conflicts {
//!     println!("{} differs: {} vs {}",
//!         conflict.path, conflict.old_value, conflict.new_value);
//! }
//! ```
//!
//! ## Quickstart (two-way reconciliation, Tier-1 facade)
//!
//! Build a [`SyncEngine`](drivers::sync_engine::SyncEngine) from two
//! adapters and a policy chain; call `.sync(entity_type, id)`. See
//! `examples/two_way_sync.rs` for a runnable walkthrough.

// ================= layer declarations =================
pub mod adapters;
pub mod application;
pub mod domain;
pub mod drivers;
pub mod ports;

// ================= crate-root convenience re-exports =================
// The most-used items, re-exported here so callers can write
// `diff_fusion::SyncEngine` instead of `diff_fusion::drivers::sync_engine::SyncEngine`.
pub use application::orchestrator::{CycleOutcome, ShadowReport};
pub use application::policy::UnresolvedConflict;
pub use drivers::facade::DiffFusion;
pub use drivers::sync_engine::SyncEngine;

// Domain types that leak into public APIs.
pub use domain::compare::compare_json;
pub use domain::error::SyncError;

// Application types commonly constructed by users (for building policies).
pub use application::transform::transform_to_cif;

use serde::{Deserialize, Serialize};

// ================= legacy free-function helpers =================

/// Conflict detected between two JSON values (detection-only API).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Conflict {
    pub path: String,
    pub old_value: String,
    pub new_value: String,
}

/// Summary of conflict detection results (detection-only API).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConflictReport {
    pub conflicts: Vec<Conflict>,
    pub has_conflicts: bool,
    pub total_conflicts: usize,
}
