//! Three playground state stores:
//!
//! - `CaptureStore` — saved [`Capture`] snapshots posted by external
//!   programs (typically `diff_fusion_observe::HttpObserver`). Looked up
//!   by id and listed under `/api/captures`.
//! - `TestStore` — wizard-authored test definitions (verbatim textareas
//!   from the New Test stepper plus the most recent run's outcome).
//!   In-memory only; cleared on server restart.
//! - `Registry<T>` / `SyncRegistry` — broadcast + ring-buffer fan-out for
//!   short-lived demo-form sync progress, surfaced over SSE at
//!   `/api/sync/:sync_id/stream`.
//!
//! Captures and tests don't need a broadcast channel — they're saved once
//! and read later when the user picks one in the UI. Only the demo's
//! progress events need live fan-out, so the generic `Registry` stays for that.

use crate::dto::ProgressEvent;
use diff_fusion::ports::observer::Capture;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

/// How many recent events to keep for replay on connect.
const REPLAY_BUFFER: usize = 128;

/// Default broadcast channel capacity. Slow subscribers drop frames.
const BROADCAST_CAPACITY: usize = 256;

/// Default eviction window: an entry with no new events for this long is
/// removed from the registry / store.
const DEFAULT_IDLE_TTL_MS: u64 = 10 * 60 * 1000;

/* ----- CaptureStore ---------------------------------------------------- */

/// One saved capture plus the wall-clock time it was posted.
#[derive(Clone)]
struct StoredCapture {
    capture: Capture,
    saved_at_ms: u64,
}

/// In-memory store of captures keyed by capture id. Posts are last-write
/// wins; entries idle past `idle_ttl_ms` are evicted.
#[derive(Clone)]
pub struct CaptureStore {
    inner: Arc<RwLock<HashMap<String, StoredCapture>>>,
    idle_ttl_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct CaptureSummary {
    pub capture_id: String,
    pub entity_type: String,
    pub canonical_id: String,
    pub saved_at_ms: u64,
}

impl Default for CaptureStore {
    fn default() -> Self {
        Self::new(DEFAULT_IDLE_TTL_MS)
    }
}

impl CaptureStore {
    pub fn new(idle_ttl_ms: u64) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            idle_ttl_ms,
        }
    }

    pub fn put(&self, capture_id: &str, capture: Capture) {
        self.evict_stale();
        let mut map = self.inner.write().expect("poisoned");
        map.insert(
            capture_id.to_string(),
            StoredCapture {
                capture,
                saved_at_ms: now_ms(),
            },
        );
    }

    pub fn get(&self, capture_id: &str) -> Option<Capture> {
        self.evict_stale();
        let map = self.inner.read().expect("poisoned");
        map.get(capture_id).map(|e| e.capture.clone())
    }

    pub fn list(&self) -> Vec<CaptureSummary> {
        self.evict_stale();
        let map = self.inner.read().expect("poisoned");
        let mut out: Vec<CaptureSummary> = map
            .iter()
            .map(|(id, entry)| CaptureSummary {
                capture_id: id.clone(),
                entity_type: entry.capture.entity_type.clone(),
                canonical_id: entry.capture.canonical_id.clone(),
                saved_at_ms: entry.saved_at_ms,
            })
            .collect();
        out.sort_by(|a, b| b.saved_at_ms.cmp(&a.saved_at_ms));
        out
    }

    fn evict_stale(&self) {
        let now = now_ms();
        let mut map = self.inner.write().expect("poisoned");
        map.retain(|_, entry| now.saturating_sub(entry.saved_at_ms) < self.idle_ttl_ms);
    }
}

/* ----- TestStore ------------------------------------------------------- */

/// A wizard-authored test, stored verbatim as the user typed it. The
/// textareas are kept as raw strings so reloading round-trips exactly
/// (no JSON re-formatting drift).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TestRecord {
    pub name: String,
    pub cif_schema: String,
    pub policy: String,
    pub transformer_a: String,
    pub transformer_b: String,
    pub system_a: String,
    pub system_b: String,
    pub ancestor: String,
    pub system_a_name: String,
    pub system_b_name: String,
    /// The outcome kind from the most recent run, if any: "Synced",
    /// "Escalated", "NoOp", or "Error". `None` means never run.
    #[serde(default)]
    pub last_outcome: Option<String>,
}

#[derive(Clone, Debug)]
struct StoredTest {
    record: TestRecord,
    saved_at_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct TestSummary {
    pub test_id: String,
    pub name: String,
    pub system_a_name: String,
    pub system_b_name: String,
    pub last_outcome: Option<String>,
    pub saved_at_ms: u64,
}

/// In-memory store of wizard-authored tests keyed by id. Last-write wins;
/// no eviction (tests are intentionally sticky for the session).
#[derive(Clone, Default)]
pub struct TestStore {
    inner: Arc<RwLock<HashMap<String, StoredTest>>>,
}

impl TestStore {
    pub fn put(&self, test_id: &str, record: TestRecord) {
        let mut map = self.inner.write().expect("poisoned");
        map.insert(
            test_id.to_string(),
            StoredTest {
                record,
                saved_at_ms: now_ms(),
            },
        );
    }

    pub fn get(&self, test_id: &str) -> Option<TestRecord> {
        let map = self.inner.read().expect("poisoned");
        map.get(test_id).map(|e| e.record.clone())
    }

    pub fn list(&self) -> Vec<TestSummary> {
        let map = self.inner.read().expect("poisoned");
        let mut out: Vec<TestSummary> = map
            .iter()
            .map(|(id, entry)| TestSummary {
                test_id: id.clone(),
                name: entry.record.name.clone(),
                system_a_name: entry.record.system_a_name.clone(),
                system_b_name: entry.record.system_b_name.clone(),
                last_outcome: entry.record.last_outcome.clone(),
                saved_at_ms: entry.saved_at_ms,
            })
            .collect();
        out.sort_by(|a, b| b.saved_at_ms.cmp(&a.saved_at_ms));
        out
    }
}

/* ----- ObserverStore --------------------------------------------------- */

/// Configured inbound producer label. Each entry tells the playground
/// "expect captures with this `capture_id` to come from a producer named
/// `name`". The playground itself is the sink (capture endpoint at
/// `POST /api/captures/:capture_id`); this struct is just metadata so
/// the UI can label / track who's sending data in.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ObserverConfig {
    pub name: String,
    pub capture_id: String,
}

#[derive(Clone, Debug)]
struct StoredObserver {
    config: ObserverConfig,
    saved_at_ms: u64,
    /// Wall-clock of the last `POST /api/captures/{capture_id}` whose
    /// path matched this observer's `capture_id`. `None` until a
    /// capture has actually arrived.
    last_seen_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ObserverSummary {
    pub observer_id: String,
    pub name: String,
    pub capture_id: String,
    pub saved_at_ms: u64,
    pub last_seen_ms: Option<u64>,
}

/// In-memory store of observer configs keyed by id. No eviction; cleared
/// on server restart. Last-write wins.
#[derive(Clone, Default)]
pub struct ObserverStore {
    inner: Arc<RwLock<HashMap<String, StoredObserver>>>,
}

impl ObserverStore {
    pub fn put(&self, observer_id: &str, config: ObserverConfig) {
        let mut map = self.inner.write().expect("poisoned");
        // Preserve last_seen across re-saves of the same id (e.g.
        // renaming) — only re-init when the entry is new.
        let last_seen_ms = map.get(observer_id).and_then(|e| e.last_seen_ms);
        map.insert(
            observer_id.to_string(),
            StoredObserver {
                config,
                saved_at_ms: now_ms(),
                last_seen_ms,
            },
        );
    }

    pub fn get(&self, observer_id: &str) -> Option<ObserverConfig> {
        let map = self.inner.read().expect("poisoned");
        map.get(observer_id).map(|e| e.config.clone())
    }

    pub fn remove(&self, observer_id: &str) -> bool {
        let mut map = self.inner.write().expect("poisoned");
        map.remove(observer_id).is_some()
    }

    pub fn list(&self) -> Vec<ObserverSummary> {
        let map = self.inner.read().expect("poisoned");
        let mut out: Vec<ObserverSummary> = map
            .iter()
            .map(|(id, entry)| ObserverSummary {
                observer_id: id.clone(),
                name: entry.config.name.clone(),
                capture_id: entry.config.capture_id.clone(),
                saved_at_ms: entry.saved_at_ms,
                last_seen_ms: entry.last_seen_ms,
            })
            .collect();
        out.sort_by(|a, b| b.saved_at_ms.cmp(&a.saved_at_ms));
        out
    }

    /// Bump `last_seen_ms` on every observer whose `capture_id` matches.
    /// Returns the count touched. Called when a capture POSTs to
    /// `/api/captures/:capture_id` so the Observers UI can show a
    /// "last seen N seconds ago" badge.
    pub fn touch_by_capture_id(&self, capture_id: &str) -> usize {
        let mut map = self.inner.write().expect("poisoned");
        let now = now_ms();
        let mut n = 0;
        for entry in map.values_mut() {
            if entry.config.capture_id == capture_id {
                entry.last_seen_ms = Some(now);
                n += 1;
            }
        }
        n
    }
}

/* ----- Registry (live fan-out, used by SyncRegistry only) -------------- */

#[derive(Clone)]
pub struct Registry<T: Clone + Send + 'static> {
    inner: Arc<RwLock<HashMap<String, Entry<T>>>>,
    idle_ttl_ms: u64,
}

struct Entry<T: Clone + Send + 'static> {
    tx: broadcast::Sender<T>,
    buffer: Vec<T>,
    last_seen_ms: u64,
}

pub struct Subscription<T: Clone + Send + 'static> {
    pub replay: Vec<T>,
    pub rx: broadcast::Receiver<T>,
}

impl<T: Clone + Send + 'static> Default for Registry<T> {
    fn default() -> Self {
        Self::new(DEFAULT_IDLE_TTL_MS)
    }
}

impl<T: Clone + Send + 'static> Registry<T> {
    pub fn new(idle_ttl_ms: u64) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            idle_ttl_ms,
        }
    }

    /// Append an event to the named entry, creating the entry if absent.
    pub fn push(&self, run_id: &str, ev: T) {
        self.evict_stale();
        let mut map = self.inner.write().expect("poisoned");
        let entry = map.entry(run_id.to_string()).or_insert_with(|| Entry {
            tx: broadcast::channel(BROADCAST_CAPACITY).0,
            buffer: Vec::with_capacity(REPLAY_BUFFER),
            last_seen_ms: now_ms(),
        });
        entry.last_seen_ms = now_ms();
        if entry.buffer.len() == REPLAY_BUFFER {
            entry.buffer.remove(0);
        }
        entry.buffer.push(ev.clone());
        let _ = entry.tx.send(ev);
    }

    pub fn subscribe(&self, run_id: &str) -> Subscription<T> {
        let mut map = self.inner.write().expect("poisoned");
        let entry = map.entry(run_id.to_string()).or_insert_with(|| Entry {
            tx: broadcast::channel(BROADCAST_CAPACITY).0,
            buffer: Vec::with_capacity(REPLAY_BUFFER),
            last_seen_ms: now_ms(),
        });
        Subscription {
            replay: entry.buffer.clone(),
            rx: entry.tx.subscribe(),
        }
    }

    fn evict_stale(&self) {
        let now = now_ms();
        let mut map = self.inner.write().expect("poisoned");
        map.retain(|_, entry| now.saturating_sub(entry.last_seen_ms) < self.idle_ttl_ms);
    }
}

/// Short-lived sync-progress fan-out for the demo form. Surfaced under
/// `/api/sync/:sync_id/stream`.
pub type SyncRegistry = Registry<ProgressEvent>;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use diff_fusion::ports::observer::SideCapture;
    use serde_json::json;

    fn cap(canonical_id: &str) -> Capture {
        Capture {
            entity_type: "po".into(),
            canonical_id: canonical_id.into(),
            side_a: SideCapture {
                system: "a".into(),
                canonical_view: json!({}),
                version: None,
            },
            side_b: SideCapture {
                system: "b".into(),
                canonical_view: json!({}),
                version: None,
            },
        }
    }

    #[test]
    fn put_then_get_returns_capture() {
        let store = CaptureStore::default();
        store.put("cap-1", cap("PO-1"));
        let got = store.get("cap-1").expect("present");
        assert_eq!(got.canonical_id, "PO-1");
    }

    #[test]
    fn list_orders_by_saved_at_descending() {
        let store = CaptureStore::default();
        store.put("cap-old", cap("X"));
        std::thread::sleep(std::time::Duration::from_millis(5));
        store.put("cap-new", cap("X"));
        let entries = store.list();
        assert_eq!(entries[0].capture_id, "cap-new");
        assert_eq!(entries[1].capture_id, "cap-old");
    }

    #[test]
    fn evicts_captures_past_ttl() {
        let store = CaptureStore::new(0);
        store.put("cap-1", cap("X"));
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert!(store.list().is_empty());
        assert!(store.get("cap-1").is_none());
    }

    #[test]
    fn put_overwrites_existing_capture() {
        let store = CaptureStore::default();
        store.put("cap-1", cap("PO-1"));
        store.put("cap-1", cap("PO-2"));
        assert_eq!(store.get("cap-1").unwrap().canonical_id, "PO-2");
    }

    fn test_record(name: &str) -> TestRecord {
        TestRecord {
            name: name.into(),
            cif_schema: "{}".into(),
            policy: "{}".into(),
            transformer_a: "{}".into(),
            transformer_b: "{}".into(),
            system_a: "{}".into(),
            system_b: "{}".into(),
            ancestor: String::new(),
            system_a_name: "a".into(),
            system_b_name: "b".into(),
            last_outcome: None,
        }
    }

    #[test]
    fn test_store_put_then_get_roundtrip() {
        let store = TestStore::default();
        store.put("t-1", test_record("First"));
        let got = store.get("t-1").expect("present");
        assert_eq!(got.name, "First");
    }

    #[test]
    fn test_store_list_orders_by_saved_at_descending() {
        let store = TestStore::default();
        store.put("t-old", test_record("old"));
        std::thread::sleep(std::time::Duration::from_millis(5));
        store.put("t-new", test_record("new"));
        let entries = store.list();
        assert_eq!(entries[0].test_id, "t-new");
        assert_eq!(entries[1].test_id, "t-old");
    }

    #[test]
    fn test_store_put_overwrites_existing() {
        let store = TestStore::default();
        let mut r = test_record("First");
        store.put("t-1", r.clone());
        r.last_outcome = Some("Synced".into());
        store.put("t-1", r);
        assert_eq!(store.get("t-1").unwrap().last_outcome.as_deref(), Some("Synced"));
    }

    fn obs(name: &str, capture_id: &str) -> ObserverConfig {
        ObserverConfig {
            name: name.into(),
            capture_id: capture_id.into(),
        }
    }

    #[test]
    fn observer_store_put_then_get_roundtrip() {
        let store = ObserverStore::default();
        store.put("o-1", obs("Local", "demo-1"));
        let got = store.get("o-1").expect("present");
        assert_eq!(got.name, "Local");
        assert_eq!(got.capture_id, "demo-1");
    }

    #[test]
    fn observer_store_remove_deletes() {
        let store = ObserverStore::default();
        store.put("o-1", obs("X", "demo-1"));
        assert!(store.remove("o-1"));
        assert!(store.get("o-1").is_none());
        assert!(!store.remove("o-1"));
    }

    #[test]
    fn observer_store_list_orders_by_saved_at_descending() {
        let store = ObserverStore::default();
        store.put("o-old", obs("old", "a"));
        std::thread::sleep(std::time::Duration::from_millis(5));
        store.put("o-new", obs("new", "b"));
        let entries = store.list();
        assert_eq!(entries[0].observer_id, "o-new");
        assert_eq!(entries[1].observer_id, "o-old");
    }

    #[test]
    fn observer_store_touch_bumps_matching_capture_id() {
        let store = ObserverStore::default();
        store.put("o-prod", obs("Production", "prod-cycle"));
        store.put("o-stg", obs("Staging", "staging-cycle"));
        // Initial state: no last_seen.
        let before = store.list();
        assert!(before.iter().all(|s| s.last_seen_ms.is_none()));
        // Touch prod-cycle; only the matching observer should bump.
        let n = store.touch_by_capture_id("prod-cycle");
        assert_eq!(n, 1);
        let after = store.list();
        let prod = after.iter().find(|s| s.observer_id == "o-prod").unwrap();
        let stg = after.iter().find(|s| s.observer_id == "o-stg").unwrap();
        assert!(prod.last_seen_ms.is_some());
        assert!(stg.last_seen_ms.is_none());
    }

    #[test]
    fn observer_store_re_put_preserves_last_seen() {
        let store = ObserverStore::default();
        store.put("o-1", obs("v1", "cap-x"));
        store.touch_by_capture_id("cap-x");
        let seen_before = store.list().into_iter().next().unwrap().last_seen_ms;
        assert!(seen_before.is_some());
        // Rename via re-put — last_seen should survive.
        store.put("o-1", obs("v2", "cap-x"));
        let seen_after = store.list().into_iter().next().unwrap().last_seen_ms;
        assert_eq!(seen_after, seen_before);
    }
}
