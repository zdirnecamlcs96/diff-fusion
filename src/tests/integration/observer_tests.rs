//! Verifies the simplified observer contract: `capture()` snapshots both
//! sides' canonical views and hands them to the observer once. The
//! reconciliation pipeline is *not* run here — that's invoked separately
//! (e.g. interactively via a capture UI).

use diff_fusion::adapters::test_memory::TestMemoryAdapter;
use diff_fusion::application::capture::capture;
use diff_fusion::ports::observer::{Capture, Observer};
use serde_json::json;
use std::sync::Mutex;

const ENTITY: &str = "purchase_order";
const ID: &str = "PO-1";

#[derive(Default)]
struct CollectingObserver {
    captured: Mutex<Vec<Capture>>,
}

impl Observer for CollectingObserver {
    fn on_capture(&self, c: &Capture) {
        self.captured.lock().unwrap().push(c.clone());
    }
}

#[tokio::test]
async fn capture_snapshots_both_sides() {
    let side_a = TestMemoryAdapter::new("sys_a");
    let side_b = TestMemoryAdapter::new("sys_b");
    side_a.seed(ENTITY, ID, json!({"price": 15}));
    side_b.seed(ENTITY, ID, json!({"price": 10}));

    let obs = CollectingObserver::default();
    let cap = capture(&side_a, &side_b, ENTITY, ID, &obs).await.unwrap();

    assert_eq!(cap.entity_type, ENTITY);
    assert_eq!(cap.canonical_id, ID);
    assert_eq!(cap.side_a.system, "sys_a");
    assert_eq!(cap.side_a.canonical_view, json!({"price": 15}));
    assert_eq!(cap.side_b.system, "sys_b");
    assert_eq!(cap.side_b.canonical_view, json!({"price": 10}));
    assert!(cap.side_a.version.is_some());
    assert!(cap.side_b.version.is_some());

    // Observer received the same payload exactly once.
    let captured = obs.captured.lock().unwrap();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0], cap);
}

#[tokio::test]
async fn capture_errors_when_entity_missing_on_one_side() {
    let side_a = TestMemoryAdapter::new("sys_a");
    let side_b = TestMemoryAdapter::new("sys_b");
    side_a.seed(ENTITY, ID, json!({"price": 15}));
    // side_b intentionally not seeded.

    let obs = CollectingObserver::default();
    let err = capture(&side_a, &side_b, ENTITY, ID, &obs)
        .await
        .expect_err("expected missing-entity error");
    let msg = err.to_string();
    assert!(msg.contains("sys_b"), "error should name the missing side: {msg}");
    assert!(obs.captured.lock().unwrap().is_empty());
}
