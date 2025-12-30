mod queries;
mod state;
#[cfg(all(test, not(feature = "ts-rs")))]
mod tests;
mod triggers;

use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use log::LevelFilter;
use serde::Deserialize;
use tauri_plugin_log::fern;
use tokio::sync::oneshot;

use crate::config::{GGSettings, read_config};
use crate::messages::{
    AbandonRevisions, BackoutRevisions, CheckoutRevision, CopyChanges, CopyHunk, CreateRef,
    CreateRevision, CreateRevisionBetween, DeleteRef, DescribeRevision, DuplicateRevisions,
    GitFetch, GitPush, InsertRevision, MoveChanges, MoveHunk, MoveRef, MoveRevision, MoveSource,
    RenameBranch, TrackBranch, UndoOperation, UntrackBranch,
};
use crate::worker::{Mutation, Session, SessionEvent, WorkerSession};
use state::{AppState, Asset};

/// anyhow -> http 500 wrapper
struct ApiError(anyhow::Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.0.to_string()).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(err: E) -> Self {
        ApiError(err.into())
    }
}

#[tokio::main]
pub async fn run_web(options: super::RunOptions, port: Option<u16>) -> Result<()> {
    fern::Dispatch::new()
        .level(LevelFilter::Warn)
        .level_for(
            "gg",
            if options.debug {
                LevelFilter::Debug
            } else {
                LevelFilter::Info
            },
        )
        .chain(std::io::stderr())
        .apply()?;

    let (repo_settings, _) = read_config(options.workspace.as_deref())?;
    let port_setting = port.unwrap_or_else(|| repo_settings.web_default_port());

    let (app, shutdown_rx) = create_app(options)?;

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port_setting}")).await?;
    let addr = listener.local_addr()?;
    let url = format!("http://{addr}");
    log::info!("Listening on {url}");

    // open browser (best-effort)
    tokio::task::spawn_blocking(move || {
        let _ = webbrowser::open(&url);
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
            log::info!("Shutdown complete.");
        })
        .await?;

    Ok(())
}

pub(self) fn create_app(options: super::RunOptions) -> Result<(Router, oneshot::Receiver<()>)> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel(); // this one needs async
    let (worker_tx, worker_rx) = channel();

    thread::spawn(move || {
        tauri::async_runtime::block_on(async {
            log::debug!("start worker");
            let session = WorkerSession::new(options.workspace, options.settings);
            if let Err(err) = session.handle_events(&worker_rx).await {
                log::error!("worker: {err:#}");
            }
            log::debug!("end worker");
        });
    });

    let state = AppState::new(
        options.context,
        worker_tx,
        shutdown_tx,
        Duration::from_secs(600),
    );

    let app = Router::new()
        // static assets
        .route("/", get(serve_index))
        .route("/assets/{*path}", get(serve_asset))
        .fallback(get(serve_fallback))
        // API endpoints
        .nest("/api/query", queries::router())
        .nest("/api/trigger", triggers::router())
        .route("/api/mutate/{command}", post(handle_mutate))
        .with_state(state.clone());

    // shut down if we don't get a ping for ten minutes
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if state.is_dead() {
                break;
            }
        }
    });

    if options.is_child {
        println!("Startup complete.");
    }

    Ok((app, shutdown_rx))
}

async fn serve_index(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let asset = state.load_asset("/index.html").await?;

    // used to track open tabs; done here so that it works for both vite and static assets
    let client_id = uuid::Uuid::new_v4().to_string();
    let injected_script = format!(r#"<script>window.__GG_CLIENT_ID__="{client_id}";</script>"#);
    let asset_html = String::from_utf8_lossy(asset.data());
    let modified_html = asset_html.replace("</head>", &format!("{injected_script}</head>"));

    Ok((
        [(axum::http::header::CONTENT_TYPE, "text/html")],
        modified_html,
    ))
}

async fn serve_asset(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Asset, StatusCode> {
    state.load_asset(&format!("/assets/{path}")).await
}

async fn serve_fallback(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<Asset, StatusCode> {
    let uri = request.uri();
    let path_and_query = uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(uri.path());
    state.load_asset(path_and_query).await
}

async fn handle_mutate(
    State(state): State<AppState>,
    Path(command): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    log::debug!("mutation: {command}");

    match command.as_str() {
        "abandon_revisions" => execute_mutation::<AbandonRevisions>(&state, body),
        "backout_revisions" => execute_mutation::<BackoutRevisions>(&state, body),
        "checkout_revision" => execute_mutation::<CheckoutRevision>(&state, body),
        "create_revision" => execute_mutation::<CreateRevision>(&state, body),
        "create_revision_between" => execute_mutation::<CreateRevisionBetween>(&state, body),
        "insert_revision" => execute_mutation::<InsertRevision>(&state, body),
        "describe_revision" => execute_mutation::<DescribeRevision>(&state, body),
        "duplicate_revisions" => execute_mutation::<DuplicateRevisions>(&state, body),
        "move_revision" => execute_mutation::<MoveRevision>(&state, body),
        "move_source" => execute_mutation::<MoveSource>(&state, body),
        "move_changes" => execute_mutation::<MoveChanges>(&state, body),
        "copy_changes" => execute_mutation::<CopyChanges>(&state, body),
        "move_hunk" => execute_mutation::<MoveHunk>(&state, body),
        "copy_hunk" => execute_mutation::<CopyHunk>(&state, body),
        "track_branch" => execute_mutation::<TrackBranch>(&state, body),
        "untrack_branch" => execute_mutation::<UntrackBranch>(&state, body),
        "rename_branch" => execute_mutation::<RenameBranch>(&state, body),
        "create_ref" => execute_mutation::<CreateRef>(&state, body),
        "delete_ref" => execute_mutation::<DeleteRef>(&state, body),
        "move_ref" => execute_mutation::<MoveRef>(&state, body),
        "git_push" => execute_mutation::<GitPush>(&state, body),
        "git_fetch" => execute_mutation::<GitFetch>(&state, body),
        "undo_operation" => {
            let (tx, rx) = channel();
            state.worker_tx.send(SessionEvent::ExecuteMutation {
                tx,
                mutation: Box::new(UndoOperation),
            })?;
            let result = rx.recv()?;
            Ok(Json(serde_json::to_value(result)?))
        }

        _ => Err(ApiError(anyhow!("Unknown mutation: {}", command))),
    }
}

fn execute_mutation<T>(
    state: &AppState,
    body: serde_json::Value,
) -> Result<Json<serde_json::Value>, ApiError>
where
    T: Mutation + Send + Sync + 'static + serde::de::DeserializeOwned,
{
    #[derive(Deserialize)]
    struct MutationRequest<U> {
        mutation: U,
    }

    let wrapper: MutationRequest<T> = serde_json::from_value(body)?;
    let (tx, rx) = channel();
    state.worker_tx.send(SessionEvent::ExecuteMutation {
        tx,
        mutation: Box::new(wrapper.mutation),
    })?;
    let result = rx.recv()?;
    Ok(Json(serde_json::to_value(result)?))
}
