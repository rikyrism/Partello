//! Liveness and readiness probes.

use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

use crate::AppState;

/// Liveness: the process is up and serving requests.
pub async fn health(State(app): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "live_rooms": app.hub.live_room_count(),
    }))
}

/// Readiness: dependencies (the database) are reachable.
pub async fn ready(State(app): State<AppState>) -> (StatusCode, Json<Value>) {
    match app.hub.store.ping().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "status": "ready" }))),
        Err(err) => {
            tracing::warn!(error = %err, "readiness check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable", "database": "unreachable" })),
            )
        }
    }
}
