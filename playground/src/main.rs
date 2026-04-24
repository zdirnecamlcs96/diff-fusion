mod dto;
mod pipeline;
mod policies;

use axum::{Json, Router, routing::post};
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::services::ServeDir;

use dto::{SyncRequest, SyncResponse};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let web_dir: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");

    let static_service = ServeDir::new(&web_dir).append_index_html_on_directories(true);

    let app = Router::new()
        .route("/sync", post(run_sync))
        .fallback_service(static_service);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("playground listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn run_sync(Json(req): Json<SyncRequest>) -> Json<SyncResponse> {
    Json(pipeline::run(req).await)
}
