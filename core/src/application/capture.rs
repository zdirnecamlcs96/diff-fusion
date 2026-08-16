//! `capture` — fetch both sides' canonical views and ship them to an
//! [`Observer`] without running the merge / diff / push pipeline.
//!
//! The observer is purely a snapshot sink. The reconciliation pipeline is
//! invoked separately — for example, a capture UI could store captures and
//! let the user pick one to feed into the demo pipeline.

use crate::domain::error::SyncError;
use crate::ports::observer::{Capture, Observer, SideCapture};
use crate::ports::system::SystemPort;

/// Snapshot the canonical view of `(entity_type, canonical_id)` from both
/// sides and hand it to `observer`. Returns the [`Capture`] for callers
/// that want it locally; the observer receives the same payload.
///
/// Errors transparently if either side cannot resolve the canonical id or
/// fails the canonical fetch.
pub async fn capture<A: SystemPort, B: SystemPort>(
    side_a: &A,
    side_b: &B,
    entity_type: &str,
    canonical_id: &str,
    observer: &dyn Observer,
) -> Result<Capture, SyncError> {
    let side_a_cap = capture_side(side_a, entity_type, canonical_id).await?;
    let side_b_cap = capture_side(side_b, entity_type, canonical_id).await?;
    let cap = Capture {
        entity_type: entity_type.to_string(),
        canonical_id: canonical_id.to_string(),
        side_a: side_a_cap,
        side_b: side_b_cap,
    };
    observer.on_capture(&cap);
    Ok(cap)
}

async fn capture_side<P: SystemPort>(
    side: &P,
    entity_type: &str,
    canonical_id: &str,
) -> Result<SideCapture, SyncError> {
    let ext = side
        .find_by_canonical_id(entity_type, canonical_id)
        .await?
        .ok_or_else(|| {
            SyncError::transient(format!(
                "entity {canonical_id} not found on {}",
                side.system_type()
            ))
        })?;
    let (canonical_view, fresh_ref) = side.fetch(entity_type, &ext).await?;
    Ok(SideCapture {
        system: side.system_type().to_string(),
        canonical_view,
        version: fresh_ref.version,
    })
}
