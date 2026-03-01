//! Web mode: an Axum HTTP server that serves the GG frontend and exposes a
//! JSON API.
//!
//! The main entry point for library consumers is [`create_app`], which returns
//! an [`axum::Router`] and a shutdown receiver. The router serves:
//!
//! - Static assets (the embedded Svelte frontend) at `/` and `/assets/*`
//! - Query endpoints at `/api/query/*`
//! - Mutation endpoints at `/api/mutate/{command}` (POST, JSON body)
//! - Trigger endpoints at `/api/trigger/*`
//! - Server-Sent Events at `/api/events` for push updates (config changes,
//!   progress, etc.)

mod queries;
mod sink;
mod state;
#[cfg(all(test, not(feature = "ts-rs")))]
mod tests;
mod triggers;

use std::convert::Infallible;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures_util::stream::{self, Stream};
use log::LevelFilter;
use serde::Deserialize;
use tauri_plugin_log::fern;
use tokio::sync::{broadcast, oneshot};

use crate::config::{GGSettings, read_config};
use crate::messages::mutations::{
    AbandonRevisions, AdoptRevision, BackoutRevisions, CheckoutRevision, CopyChanges, CopyHunk,
    CreateRef, CreateRevision, CreateRevisionBetween, DeleteRef, DescribeRevision,
    DuplicateRevisions, ExternalDiff, ExternalResolve, GitFetch, GitPush, InsertRevisions,
    MoveChanges, MoveHunk, MoveRef, MoveRevisions, MutationOptions, RenameBookmark, TrackBookmark,
    UndoOperation, UntrackBookmark,
};
use crate::worker::{Mutation, Session, SessionEvent, WorkerSession};
use sink::{SseEvent, SseSink};
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

/// Options specific to the `gg web` CLI subcommand.
///
/// These control how [`run_web`] binds and launches the server. They are not
/// needed when calling [`create_app`] directly.
#[derive(Default)]
pub struct WebOptions {
    /// TCP port to bind to. When `None`, uses the value from
    /// `gg.web.default-port` in jj config (default 2178).
    pub port: Option<u16>,
    /// Force-open the browser regardless of config.
    pub launch: bool,
    /// Suppress browser launch regardless of config.
    pub no_launch: bool,
}

#[doc(hidden)]
#[tokio::main]
pub async fn run_web(options: super::RunOptions, web_options: WebOptions) -> Result<()> {
    let gg_level = if options.debug {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    fern::Dispatch::new()
        .level(LevelFilter::Warn)
        .level_for("gg", gg_level)
        .level_for("gg_lib", gg_level)
        .chain(std::io::stderr())
        .apply()?;

    let (repo_settings, _, _) = read_config(options.workspace.as_deref())?;
    let client_timeout = repo_settings.web_client_timeout();
    let (app, shutdown_rx) = create_app(options, Some(client_timeout))?;

    // bind to selected or random port
    let port = web_options
        .port
        .unwrap_or_else(|| repo_settings.web_default_port());
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    let addr = listener.local_addr()?;
    let url = format!("http://{addr}");
    log::info!("Listening on {url}");

    // open browser (best-effort)
    let launch_browser = if web_options.launch {
        true
    } else if web_options.no_launch {
        false
    } else {
        repo_settings.web_launch_browser()
    };

    if launch_browser {
        tokio::task::spawn_blocking(move || {
            let _ = webbrowser::open(&url);
        });
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
            log::info!("Shutdown complete.");
        })
        .await?;

    Ok(())
}

/// Build an Axum [`Router`] that serves the GG frontend and JSON API.
///
/// This is the primary integration point for embedding GG in another service.
/// It spawns a background worker thread (via [`WorkerSession`]) and wires up
/// all routes. The returned [`oneshot::Receiver`] fires when the server should
/// shut down (e.g. the last SSE client disconnected after `client_timeout`).
///
/// # Arguments
///
/// - `options` — workspace path, settings, and flags (see [`RunOptions`](super::RunOptions))
/// - `client_timeout` — when `Some`, the server shuts itself down after all
///   SSE clients have been disconnected for this duration
///
/// # Example
///
/// ```no_run
/// use std::path::PathBuf;
/// use gg_lib::{RunOptions, web};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let options = RunOptions::new(PathBuf::from("."));
///     let (app, shutdown_rx) = web::create_app(options, None)?;
///
///     let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
///     axum::serve(listener, app)
///         .with_graceful_shutdown(async { let _ = shutdown_rx.await; })
///         .await?;
///     Ok(())
/// }
/// ```
pub fn create_app(
    options: super::RunOptions,
    client_timeout: Option<Duration>,
) -> Result<(Router, oneshot::Receiver<()>)> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel(); // this one needs async
    let (worker_tx, worker_rx) = channel();
    let (progress_tx, _progress_rx) = broadcast::channel::<SseEvent>(16);

    let progress_sender = SseSink::new(progress_tx.clone());

    thread::spawn(move || {
        tauri::async_runtime::block_on(async {
            log::debug!("start worker");
            let session = WorkerSession::new(
                progress_sender,
                options.workspace,
                options.settings,
                options.ignore_immutable,
                options.enable_askpass,
            );
            if let Err(err) = session.handle_events(&worker_rx).await {
                log::error!("worker: {err:#}");
            }
            log::debug!("end worker");
        });
    });

    let state = AppState::new(
        options.context,
        worker_tx,
        progress_tx,
        shutdown_tx,
        client_timeout,
    );

    let app = Router::new()
        // static assets
        .route("/", get(serve_index))
        .route("/log", get(serve_index))
        .route("/revision", get(serve_index))
        .route("/assets/{*path}", get(serve_asset))
        .fallback(get(serve_fallback))
        // API endpoints
        .nest("/api/query", queries::router())
        .nest("/api/trigger", triggers::router())
        .route("/api/mutate/{command}", post(handle_mutate))
        .route("/api/events", get(stream_events))
        .with_state(state.clone());

    if client_timeout.is_some() {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                if state.is_dead() {
                    break;
                }
            }
        });
    }

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

async fn stream_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.progress_tx.subscribe();

    let stream = stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Ok((event_name, payload)) => {
                let sse_event = Event::default()
                    .event(event_name)
                    .data(serde_json::to_string(&payload).unwrap_or_default());
                Some((Ok(sse_event), rx))
            }
            Err(broadcast::error::RecvError::Closed) => None,
            Err(broadcast::error::RecvError::Lagged(_)) => {
                Some((Ok(Event::default().comment("lagged")), rx))
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
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
        "insert_revisions" => execute_mutation::<InsertRevisions>(&state, body),
        "describe_revision" => execute_mutation::<DescribeRevision>(&state, body),
        "duplicate_revisions" => execute_mutation::<DuplicateRevisions>(&state, body),
        "move_revisions" => execute_mutation::<MoveRevisions>(&state, body),
        "adopt_revision" => execute_mutation::<AdoptRevision>(&state, body),
        "move_changes" => execute_mutation::<MoveChanges>(&state, body),
        "copy_changes" => execute_mutation::<CopyChanges>(&state, body),
        "move_hunk" => execute_mutation::<MoveHunk>(&state, body),
        "copy_hunk" => execute_mutation::<CopyHunk>(&state, body),
        "track_bookmark" => execute_mutation::<TrackBookmark>(&state, body),
        "untrack_bookmark" => execute_mutation::<UntrackBookmark>(&state, body),
        "rename_bookmark" => execute_mutation::<RenameBookmark>(&state, body),
        "create_ref" => execute_mutation::<CreateRef>(&state, body),
        "delete_ref" => execute_mutation::<DeleteRef>(&state, body),
        "move_ref" => execute_mutation::<MoveRef>(&state, body),
        "git_push" => execute_mutation::<GitPush>(&state, body),
        "git_fetch" => execute_mutation::<GitFetch>(&state, body),
        "external_diff" => execute_mutation::<ExternalDiff>(&state, body),
        "external_resolve" => execute_mutation::<ExternalResolve>(&state, body),
        "undo_operation" => {
            let (tx, rx) = channel();
            state.worker_tx.send(SessionEvent::ExecuteMutation {
                tx,
                mutation: Box::new(UndoOperation),
                options: MutationOptions {
                    ignore_immutable: false,
                },
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
        options: MutationOptions,
    }

    let wrapper: MutationRequest<T> = serde_json::from_value(body)?;
    let (tx, rx) = channel();
    state.worker_tx.send(SessionEvent::ExecuteMutation {
        tx,
        mutation: Box::new(wrapper.mutation),
        options: wrapper.options,
    })?;
    let result = rx.recv()?;
    Ok(Json(serde_json::to_value(result)?))
}
