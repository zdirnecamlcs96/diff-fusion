mod dto;
mod pipeline;
mod runs;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{Json, Router, routing::get, routing::post};
use diff_fusion::application::policy::suggest_policies;
use diff_fusion::ports::observer::Capture;
use futures::stream::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::services::ServeDir;

use dto::{ProgressEvent, SyncRequest, SyncResponse};
use runs::{
    CaptureStore, CaptureSummary, ObserverConfig, ObserverStore, ObserverSummary, Subscription,
    SyncRegistry, TestRecord, TestStore, TestSummary,
};

#[derive(Clone)]
struct AppState {
    /// Saved captures from external programs (typically `HttpObserver`).
    captures: CaptureStore,
    /// Wizard-authored tests saved by the New Test stepper. In-memory
    /// only; cleared on server restart.
    tests: TestStore,
    /// Configured observer endpoints the wizard can target. In-memory.
    observers: ObserverStore,
    /// Short-lived per-sync progress fan-out for the demo dialog.
    sync: SyncRegistry,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Vite build output. Run `npm install && npm run build` in
    // playground/web/ before launching the server.
    let web_dir: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web/dist");

    let static_service = ServeDir::new(&web_dir).append_index_html_on_directories(true);

    let state = AppState {
        captures: CaptureStore::default(),
        tests: TestStore::default(),
        observers: ObserverStore::default(),
        sync: SyncRegistry::default(),
    };

    let app = Router::new()
        .route("/sync", post(run_sync))
        .route("/api/suggest", post(run_suggest))
        .route("/api/captures", get(list_captures))
        .route(
            "/api/captures/:capture_id",
            post(put_capture).get(get_capture),
        )
        .route("/api/tests", get(list_tests))
        .route("/api/tests/:test_id", post(put_test).get(get_test))
        .route("/api/observers", get(list_observers))
        .route(
            "/api/observers/:observer_id",
            post(put_observer).get(get_observer).delete(delete_observer),
        )
        .route("/api/sync/:sync_id/stream", get(stream_sync))
        .with_state(state)
        .fallback_service(static_service);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("playground listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn run_sync(
    State(state): State<AppState>,
    Json(req): Json<SyncRequest>,
) -> Json<SyncResponse> {
    let run_id = req.run_id.clone();
    let progress = run_id.as_deref().map(|id| pipeline::Progress {
        registry: &state.sync,
        run_id: id,
    });
    Json(pipeline::run(req, progress).await)
}

/// Request body for `POST /api/suggest`. Accepts either a raw schema JSON
/// or `{ "schema": {...} }` so the page doesn't have to care whether its
/// textarea already has the wrapper shape.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SuggestRequest {
    Wrapped { schema: Value },
    Bare(Value),
}

/// Turn a CIF schema into a draft `per_field` declaration. Thin wrapper
/// around `diff_fusion::application::policy::suggest_policies` — all
/// heuristics live in core so other frontends (CLI, WASM, etc.) consume
/// the same rules.
async fn run_suggest(Json(req): Json<SuggestRequest>) -> Json<Value> {
    let schema = match req {
        SuggestRequest::Wrapped { schema } => schema,
        SuggestRequest::Bare(v) => v,
    };
    let per_field = suggest_policies(&schema);
    Json(serde_json::json!({ "per_field": per_field }))
}

/// `POST /api/captures/:capture_id` — save a capture posted by an external
/// program (typically a `diff_fusion_observe::HttpObserver`). Body is the
/// `Capture` JSON itself; the path id is authoritative. As a side effect,
/// any registered observer whose `capture_id` matches gets its
/// `last_seen_ms` bumped so the Observers UI can show producer activity.
async fn put_capture(
    State(state): State<AppState>,
    Path(capture_id): Path<String>,
    Json(capture): Json<Capture>,
) -> &'static str {
    state.captures.put(&capture_id, capture);
    state.observers.touch_by_capture_id(&capture_id);
    "ok"
}

/// `GET /api/captures` — list captures the store currently knows about,
/// most recently saved first.
async fn list_captures(State(state): State<AppState>) -> Json<Vec<CaptureSummary>> {
    Json(state.captures.list())
}

/// `GET /api/captures/:capture_id` — fetch one capture by id.
async fn get_capture(
    State(state): State<AppState>,
    Path(capture_id): Path<String>,
) -> Result<Json<Capture>, StatusCode> {
    state
        .captures
        .get(&capture_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// `POST /api/tests/:test_id` — save (or overwrite) a wizard-authored test.
/// Body is the full `TestRecord`; the path id is authoritative. The
/// stepper calls this twice per run: once before /sync (last_outcome=null)
/// so the test shows up in the dashboard immediately, then again after
/// /sync completes with the outcome filled in.
async fn put_test(
    State(state): State<AppState>,
    Path(test_id): Path<String>,
    Json(record): Json<TestRecord>,
) -> &'static str {
    state.tests.put(&test_id, record);
    "ok"
}

/// `GET /api/tests` — list saved tests, most recently saved first.
async fn list_tests(State(state): State<AppState>) -> Json<Vec<TestSummary>> {
    Json(state.tests.list())
}

/// `GET /api/tests/:test_id` — fetch one test's full record (textareas
/// verbatim, ready to drop back into the wizard).
async fn get_test(
    State(state): State<AppState>,
    Path(test_id): Path<String>,
) -> Result<Json<TestRecord>, StatusCode> {
    state
        .tests
        .get(&test_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// `POST /api/observers/:observer_id` — register or overwrite an observer
/// endpoint. Body is the `ObserverConfig` (`name`, `endpoint`); the path id
/// is authoritative.
async fn put_observer(
    State(state): State<AppState>,
    Path(observer_id): Path<String>,
    Json(config): Json<ObserverConfig>,
) -> &'static str {
    state.observers.put(&observer_id, config);
    "ok"
}

/// `GET /api/observers` — list configured observers, most-recent-saved first.
async fn list_observers(State(state): State<AppState>) -> Json<Vec<ObserverSummary>> {
    Json(state.observers.list())
}

/// `GET /api/observers/:observer_id` — fetch one config.
async fn get_observer(
    State(state): State<AppState>,
    Path(observer_id): Path<String>,
) -> Result<Json<ObserverConfig>, StatusCode> {
    state
        .observers
        .get(&observer_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// `DELETE /api/observers/:observer_id` — remove a configured observer.
async fn delete_observer(
    State(state): State<AppState>,
    Path(observer_id): Path<String>,
) -> Result<&'static str, StatusCode> {
    if state.observers.remove(&observer_id) {
        Ok("ok")
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

fn to_progress_sse(ev: &ProgressEvent) -> Event {
    Event::default()
        .event("progress")
        .json_data(ev)
        .unwrap_or_else(|_| Event::default().data("{}"))
}

/// `GET /api/sync/:sync_id/stream` — SSE stream of `ProgressEvent`s for a
/// single demo-form sync. Replays the ring buffer on connect so a dialog
/// opened mid-cycle still catches earlier stages.
async fn stream_sync(
    State(state): State<AppState>,
    Path(sync_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let sub: Subscription<ProgressEvent> = state.sync.subscribe(&sync_id);
    let replay = futures::stream::iter(sub.replay.into_iter().map(|ev| Ok(to_progress_sse(&ev))));
    let live = tokio_stream::wrappers::BroadcastStream::new(sub.rx)
        .filter_map(|res| async move { res.ok() })
        .map(|ev| Ok(to_progress_sse(&ev)));
    Sse::new(replay.chain(live)).keep_alive(KeepAlive::default())
}
