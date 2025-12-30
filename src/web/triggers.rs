//! Typed handlers for web mode triggers

use super::state::AppState;
use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use serde::Deserialize;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/heartbeat", post(heartbeat))
        .route("/begin_shutdown", post(begin_shutdown))
        .route("/end_shutdown", post(end_shutdown))
}

#[derive(Deserialize)]
struct Beacon {
    client_id: String,
}

async fn heartbeat(State(state): State<AppState>, Json(body): Json<Beacon>) -> StatusCode {
    state.keep_alive(body.client_id);
    StatusCode::OK
}

async fn begin_shutdown(State(state): State<AppState>, Json(body): Json<Beacon>) -> StatusCode {
    state.last_rites(body.client_id);
    StatusCode::OK
}

async fn end_shutdown(State(state): State<AppState>, Json(body): Json<Beacon>) -> StatusCode {
    state.keep_alive(body.client_id);
    StatusCode::OK
}
