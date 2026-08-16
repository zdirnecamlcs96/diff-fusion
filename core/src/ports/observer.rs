//! `Observer` — capture sink for both sides of an entity.
//!
//! An observer receives a single [`Capture`] payload that snapshots the
//! canonical view of `side_a` and `side_b` for a given entity. The trait
//! has **no transport dependencies** — implement it in-process to log,
//! ship over HTTP, write to disk, etc.
//!
//! Observers are passive: they don't run the merge / diff / push pipeline
//! and don't decide what to do with the data. The reconciliation pipeline
//! is invoked separately (interactively via a capture UI, or
//! programmatically by `Orchestrator::run_cycle_at`).
//!
//! Observers MUST NOT block on I/O on the calling task; queue and return.
//!
//! # Wire format
//!
//! [`Capture`] derives `Serialize` / `Deserialize` so a remote sink (e.g.
//! `diff_fusion_observe::HttpObserver`) can ship the snapshot as JSON.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Sink that receives one [`Capture`] per snapshot.
pub trait Observer: Send + Sync {
    fn on_capture(&self, c: &Capture);
}

/// Snapshot of an entity on both sides at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capture {
    pub entity_type: String,
    pub canonical_id: String,
    pub side_a: SideCapture,
    pub side_b: SideCapture,
}

/// One side of a [`Capture`] — system name plus the canonical view fetched
/// from that system's adapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SideCapture {
    /// `SystemPort::system_type()` of the source.
    pub system: String,
    /// CIF view returned by `SystemPort::fetch`.
    pub canonical_view: Value,
    /// `ExternalRef::version` returned by the adapter, if any.
    pub version: Option<String>,
}
