//! Typed handlers for web mode queries

use std::path::PathBuf;
use std::sync::mpsc::channel;

use axum::{Json, Router, extract::State, routing::post};
use serde::Deserialize;

use crate::messages::{self, LogPage, RepoConfig, RevSet, RevsResult};
use crate::worker::SessionEvent;

use super::ApiError;
use super::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/query_workspace", post(query_workspace))
        .route("/query_log", post(query_log))
        .route("/query_log_next_page", post(query_log_next_page))
        .route("/query_revisions", post(query_revisions))
        .route("/query_remotes", post(query_remotes))
        .route("/query_recent_workspaces", post(query_recent_workspaces))
        .route("/query_snapshot", post(query_snapshot))
}

#[derive(Deserialize)]
pub struct QueryWorkspace {
    path: Option<String>,
}

async fn query_workspace(
    State(state): State<AppState>,
    Json(req): Json<QueryWorkspace>,
) -> Result<Json<RepoConfig>, ApiError> {
    let path = req.path.map(PathBuf::from);
    let (tx, rx) = channel();
    state.worker_tx.send(SessionEvent::OpenWorkspace {
        tx,
        wd: path.clone(),
    })?;

    let config = match rx.recv()? {
        Ok(config) => {
            log::debug!("load workspace succeeded");
            config
        }
        Err(err) => {
            log::warn!("load workspace failed: {err}");
            RepoConfig::LoadError {
                absolute_path: path.unwrap_or_default().into(),
                message: format!("{:#}", err),
            }
        }
    };

    Ok(Json(config))
}

#[derive(Deserialize)]
pub struct QueryLog {
    revset: String,
}

async fn query_log(
    State(state): State<AppState>,
    Json(req): Json<QueryLog>,
) -> Result<Json<LogPage>, ApiError> {
    let (tx, rx) = channel();
    state.worker_tx.send(SessionEvent::QueryLog {
        tx,
        query: req.revset,
    })?;
    let result = rx.recv()??;
    Ok(Json(result))
}

async fn query_log_next_page(State(state): State<AppState>) -> Result<Json<LogPage>, ApiError> {
    let (tx, rx) = channel();
    state
        .worker_tx
        .send(SessionEvent::QueryLogNextPage { tx })?;
    let result = rx.recv()??;
    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct QueryRevisions {
    set: RevSet,
}

async fn query_revisions(
    State(state): State<AppState>,
    Json(req): Json<QueryRevisions>,
) -> Result<Json<RevsResult>, ApiError> {
    let (tx, rx) = channel();
    state
        .worker_tx
        .send(SessionEvent::QueryRevisions { tx, set: req.set })?;
    let result = rx.recv()??;
    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct QueryRemotes {
    tracking_bookmark: Option<String>,
}

async fn query_remotes(
    State(state): State<AppState>,
    Json(req): Json<QueryRemotes>,
) -> Result<Json<Vec<String>>, ApiError> {
    let (tx, rx) = channel();
    state.worker_tx.send(SessionEvent::QueryRemotes {
        tx,
        tracking_bookmark: req.tracking_bookmark,
    })?;
    let result = rx.recv()??;
    Ok(Json(result))
}

async fn query_recent_workspaces(
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, ApiError> {
    let (tx, rx) = channel();
    state.worker_tx.send(SessionEvent::ReadConfigArray {
        tx,
        key: vec!["gg".into(), "ui".into(), "recent-workspaces".into()],
    })?;
    let result = rx.recv()?.unwrap_or_default();
    Ok(Json(result))
}

async fn query_snapshot(
    State(state): State<AppState>,
) -> Result<Json<Option<messages::RepoStatus>>, ApiError> {
    let (tx, rx) = channel();
    state.worker_tx.send(SessionEvent::ExecuteSnapshot { tx })?;
    let result = rx.recv()?;
    Ok(Json(result))
}
