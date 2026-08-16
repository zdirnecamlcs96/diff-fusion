//! Error categories for the sync cycle.
//!
//! Every error that flows through the orchestrator falls into exactly one of
//! three categories. Each category drives a different recovery path:
//!
//! - [`SyncError::Transient`] — retry with backoff (network, rate limit, 5xx).
//! - [`SyncError::StaleWrite`] — restart the cycle; another actor moved first.
//! - [`SyncError::Conflict`] — the resolver cannot decide; route to escalation.
//!
//! The categories are the interface. Never construct a bare error — pick one.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncError {
    /// A recoverable I/O or remote-side failure. Retry with backoff.
    #[error("transient failure: {0}")]
    Transient(String),

    /// Optimistic concurrency check failed on push — the external version we
    /// asserted no longer matches. Go back to step 1 of the cycle and re-diff
    /// against fresh state.
    #[error("stale write: {message} (system={system}, expected_version={expected:?})")]
    StaleWrite {
        system: String,
        expected: Option<String>,
        message: String,
    },

    /// The resolver could not decide a merge outcome for one or more fields.
    /// The orchestrator routes these to the escalation queue; do not throw.
    #[error("unresolved conflict(s): {paths:?}")]
    Conflict { paths: Vec<String> },
}

impl SyncError {
    pub fn transient(msg: impl Into<String>) -> Self {
        Self::Transient(msg.into())
    }

    pub fn stale(system: impl Into<String>, expected: Option<String>, msg: impl Into<String>) -> Self {
        Self::StaleWrite {
            system: system.into(),
            expected,
            message: msg.into(),
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_context() {
        let e = SyncError::stale("sap", Some("v3".into()), "version moved");
        let s = format!("{e}");
        assert!(s.contains("sap"));
        assert!(s.contains("v3"));
    }
}
