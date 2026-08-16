//! End-to-end smoke test for capture mode.
//!
//! Snapshots the canonical view of an entity from two `TestMemoryAdapter`
//! sides and ships the resulting `Capture` to the playground over HTTP.
//! The reconciliation pipeline is **not** run here — the playground stores
//! the capture, and the user clicks a saved capture in the browser to
//! load it into the demo form and run sync interactively.
//!
//! Usage:
//!   1. In one terminal:    cargo run -p playground
//!   2. Open browser:       http://localhost:3000
//!   3. In another terminal: cargo run --example observe_demo
//!   4. Click the capture id that appears under "Captures".
//!
//! Override the playground endpoint with `PLAYGROUND_URL` and the capture
//! identifier with `OBSERVE_CAPTURE_ID`.

use diff_fusion::adapters::test_memory::TestMemoryAdapter;
use diff_fusion::application::capture::capture;
use diff_fusion::application::orchestrator::now_ms;
use diff_fusion::ports::observer::Observer;
use diff_fusion_observe::HttpObserver;
use serde_json::json;
use std::sync::Arc;

const ENTITY: &str = "purchase_order";
const ID: &str = "PO-42";

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::var("PLAYGROUND_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());
    let capture_id = std::env::var("OBSERVE_CAPTURE_ID")
        .unwrap_or_else(|_| format!("demo-{}", now_ms()));

    println!("posting capture to {endpoint} as id {capture_id}");

    let side_a = TestMemoryAdapter::new("erp");
    let side_b = TestMemoryAdapter::new("warehouse");
    side_a.seed(ENTITY, ID, json!({"price": 18, "qty_received": 0}));
    side_b.seed(ENTITY, ID, json!({"price": 12, "qty_received": 3}));

    let observer: Arc<dyn Observer> = Arc::new(HttpObserver::new(endpoint, &capture_id));
    let cap = capture(&side_a, &side_b, ENTITY, ID, &*observer).await?;
    println!(
        "captured {}/{}: side_a={} side_b={}",
        cap.entity_type, cap.canonical_id, cap.side_a.canonical_view, cap.side_b.canonical_view
    );

    // Give the background HTTP task a moment to flush before tear-down.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Ok(())
}
