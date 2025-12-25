//! Typed handlers for web mode triggers

use super::state::AppState;
use axum::{Router, extract::State, http::StatusCode, routing::post};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/heartbeat", post(heartbeat))
        .route("/begin_shutdown", post(begin_shutdown))
        .route("/end_shutdown", post(end_shutdown))
}

async fn heartbeat(State(state): State<AppState>) -> StatusCode {
    state.keep_alive();
    StatusCode::OK
}

async fn begin_shutdown(State(state): State<AppState>) -> StatusCode {
    state.cancel_shutdown();
    state.request_shutdown();
    StatusCode::OK
}

async fn end_shutdown(State(state): State<AppState>) -> StatusCode {
    state.cancel_shutdown();
    state.keep_alive();
    StatusCode::OK
}
