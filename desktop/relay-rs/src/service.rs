//! The private side: the Node relay asks for the heavy reads here, over loopback, with a secret.
use crate::core;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};

type Secret = Arc<String>;

async fn guard(State(s): State<Secret>, h: HeaderMap, req: Request, next: Next) -> Response {
    if h.get("x-hearth-core").and_then(|v| v.to_str().ok()) != Some(s.as_str()) { return StatusCode::UNAUTHORIZED.into_response(); }
    next.run(req).await
}

async fn blocking<F: FnOnce() -> Value + Send + 'static>(f: F) -> Json<Value> {
    Json(tokio::task::spawn_blocking(f).await.unwrap_or_else(|_| json!({"error": "failed"})))
}

pub fn router(secret: String) -> Router {
    let s: Secret = Arc::new(secret);
    Router::new()
        .route("/summaries", post(|| async { blocking(core::summaries).await }))
        .route("/messages", post(|Json(b): Json<Value>| async move {
            let file = PathBuf::from(b.get("file").and_then(|x| x.as_str()).unwrap_or(""));
            let tail = b.get("tail").and_then(|x| x.as_u64()).unwrap_or(80) as usize;
            blocking(move || core::messages(&file, tail)).await
        }))
        .route("/search", post(|Json(b): Json<Value>| async move {
            let q = b.get("q").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let limit = b.get("limit").and_then(|x| x.as_u64()).unwrap_or(25) as usize;
            blocking(move || core::search(&q, limit)).await
        }))
        .route("/usage", post(|Json(b): Json<Value>| async move {
            let files: Vec<PathBuf> = b.get("files").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).map(PathBuf::from).collect()).unwrap_or_default();
            blocking(move || {
                use rayon::prelude::*;
                let rows: Vec<Value> = files.par_iter().map(|f| core::usage(f)).collect();
                Value::Array(rows)
            }).await
        }))
        .layer(middleware::from_fn_with_state(s, guard))
}
