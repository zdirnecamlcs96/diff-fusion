//! `diff_fusion_observe` — HTTP sink for `diff_fusion::ports::observer::Observer`.
//!
//! Wraps an [`Observer`] implementation around a non-blocking channel +
//! background `tokio` task that POSTs each [`Capture`] as JSON to the
//! playground's ingestion endpoint:
//!
//!   `POST {endpoint}/api/captures/{capture_id}`
//!
//! The channel is bounded; if the background task can't keep up, captures
//! are **dropped** rather than backpressuring the caller. The capture
//! payload itself is the request body — the path id is authoritative.
//!
//! # Example
//!
//! ```no_run
//! use std::sync::Arc;
//! use diff_fusion_observe::HttpObserver;
//!
//! let observer = Arc::new(HttpObserver::new("http://localhost:3000", "demo-1"));
//! // diff_fusion::application::capture::capture(&a, &b, "po", "PO-1", &*observer).await?;
//! ```

use diff_fusion::ports::observer::{Capture, Observer};
use tokio::sync::mpsc;

/// Default capacity of the internal mpsc buffer. Captures are typically
/// shipped one at a time; a small buffer is plenty.
const DEFAULT_CAPACITY: usize = 4;

/// Observer that ships captures to a playground over HTTP.
///
/// Cheap to clone via `Arc`. Send + Sync — wrap in `Arc<dyn Observer>`
/// before passing to `capture()`.
pub struct HttpObserver {
    tx: mpsc::Sender<Capture>,
    /// Kept alive for the lifetime of the observer; aborts when the
    /// observer is dropped.
    _task: tokio::task::JoinHandle<()>,
}

impl HttpObserver {
    /// Build an observer pointed at `endpoint` (e.g. `http://localhost:3000`),
    /// tagging every shipped capture with `capture_id` (used as the path
    /// segment, not the body).
    pub fn new(endpoint: impl Into<String>, capture_id: impl Into<String>) -> Self {
        Self::with_capacity(endpoint, capture_id, DEFAULT_CAPACITY)
    }

    pub fn with_capacity(
        endpoint: impl Into<String>,
        capture_id: impl Into<String>,
        capacity: usize,
    ) -> Self {
        let endpoint = endpoint.into();
        let capture_id = capture_id.into();
        let (tx, mut rx) = mpsc::channel::<Capture>(capacity);
        let url = format!(
            "{}/api/captures/{}",
            endpoint.trim_end_matches('/'),
            capture_id
        );
        let task = tokio::spawn(async move {
            let client = reqwest::Client::new();
            while let Some(cap) = rx.recv().await {
                // Best-effort: a failed POST never reaches the caller.
                let _ = client.post(&url).json(&cap).send().await;
            }
        });
        Self { tx, _task: task }
    }
}

impl Observer for HttpObserver {
    fn on_capture(&self, c: &Capture) {
        // Drop on backpressure (try_send returns Err on full / closed).
        let _ = self.tx.try_send(c.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::extract::{Path, State};
    use axum::routing::post;
    use diff_fusion::ports::observer::{Capture, SideCapture};
    use serde_json::{Value, json};
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;

    type Captured = Arc<Mutex<Vec<(String, Value)>>>;

    async fn ingest(
        State(captured): State<Captured>,
        Path(capture_id): Path<String>,
        body: String,
    ) -> &'static str {
        let parsed: Value = serde_json::from_str(&body).unwrap();
        captured.lock().await.push((capture_id, parsed));
        "ok"
    }

    fn build_server() -> (Captured, SocketAddr, tokio::task::JoinHandle<()>) {
        let captured: Captured = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/api/captures/:capture_id", post(ingest))
            .with_state(captured.clone());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (captured, addr, handle)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ships_capture_to_endpoint_with_id_in_path() {
        let (captured, addr, _server) = build_server();
        let observer = HttpObserver::new(format!("http://{addr}"), "cap-xyz");

        let cap = Capture {
            entity_type: "purchase_order".into(),
            canonical_id: "PO-1".into(),
            side_a: SideCapture {
                system: "erp".into(),
                canonical_view: json!({"price": 18}),
                version: Some("1".into()),
            },
            side_b: SideCapture {
                system: "warehouse".into(),
                canonical_view: json!({"price": 12}),
                version: Some("1".into()),
            },
        };
        observer.on_capture(&cap);

        for _ in 0..50 {
            if !captured.lock().await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let captured = captured.lock().await;
        assert_eq!(captured.len(), 1, "expected 1 capture; got {captured:?}");
        let (capture_id, body) = &captured[0];
        assert_eq!(capture_id, "cap-xyz");
        assert_eq!(body["entity_type"], "purchase_order");
        assert_eq!(body["canonical_id"], "PO-1");
        assert_eq!(body["side_a"]["system"], "erp");
        assert_eq!(body["side_a"]["canonical_view"], json!({"price": 18}));
        assert_eq!(body["side_b"]["system"], "warehouse");
    }
}
