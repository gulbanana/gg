//! Typed handlers for web mode triggers

use std::collections::HashMap;

use super::state::AppState;
use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use jj_lib::config::ConfigSource;
use serde::Deserialize;

use crate::worker::SessionEvent;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/heartbeat", post(heartbeat))
        .route("/begin_shutdown", post(begin_shutdown))
        .route("/end_shutdown", post(end_shutdown))
        .route("/write_config_table", post(write_config_table))
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

#[derive(Deserialize)]
struct WriteConfigTableRequest {
    #[allow(dead_code)]
    client_id: String,
    scope: String,
    key: Vec<String>,
    values: HashMap<String, String>,
}

async fn write_config_table(
    State(state): State<AppState>,
    Json(body): Json<WriteConfigTableRequest>,
) -> StatusCode {
    let config_scope = match body.scope.as_str() {
        "user" => ConfigSource::User,
        "repo" => ConfigSource::Repo,
        _ => return StatusCode::BAD_REQUEST,
    };
    match state.worker_tx.send(SessionEvent::WriteConfigTable {
        scope: config_scope,
        key: body.key,
        values: body.values,
    }) {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
