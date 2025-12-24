mod state;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{Request, StatusCode};
use axum::{Router, routing::get};
use jj_lib::settings::UserSettings;

use state::{AppState, Asset};

#[tokio::main]
pub async fn run_web(
    _workspace: Option<PathBuf>,
    _settings: UserSettings,
    context: tauri::Context<tauri::Wry>,
) -> Result<()> {
    let state = AppState::new(context);

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/assets/{*path}", get(serve_asset))
        .fallback(get(serve_fallback))
        .with_state(state);

    // bind to random port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;

    let addr = listener.local_addr()?;
    let url = format!("http://{}", addr);
    println!("Listening on {}", url);

    tokio::task::spawn_blocking(move || {
        let _ = webbrowser::open(&url); // best-effort
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::time::sleep(Duration::from_mins(10)).await;
            println!("Shutting down after 10 minutes");
        })
        .await?;

    Ok(())
}

async fn serve_index(State(state): State<AppState>) -> Result<Asset, StatusCode> {
    state.load_asset("/index.html").await
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
