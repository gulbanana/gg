use std::time::Duration;
use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Router, routing::get};
use itertools::Itertools;
use jj_lib::settings::UserSettings;
use tauri_utils::mime_type::MimeType;

#[derive(Clone)]
struct AppState {
    context: Arc<tauri::Context<tauri::Wry>>,
}

impl AppState {
    fn load_asset(&self, path: &str) -> Asset {
        let data = self
            .context
            .assets
            .get(&path.into())
            .map(|data| data.iter().copied().collect_vec());
        Asset {
            mime_type: match data {
                Some(ref d) => MimeType::parse(d, path),
                None => MimeType::OctetStream.to_string(),
            },
            data,
        }
    }
}

struct Asset {
    mime_type: String,
    data: Option<Vec<u8>>,
}

impl IntoResponse for Asset {
    fn into_response(self) -> Response {
        match self.data {
            Some(data) => ([(header::CONTENT_TYPE, self.mime_type)], data).into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        }
    }
}

#[tokio::main]
pub async fn run_web(
    _workspace: Option<PathBuf>,
    _settings: UserSettings,
    context: tauri::Context<tauri::Wry>,
) -> Result<()> {
    let app = Router::new()
        .route("/", get(serve_index))
        .route("/assets/{*path}", get(serve_asset))
        .with_state(AppState {
            context: Arc::new(context),
        });

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

async fn serve_index(State(state): State<AppState>) -> Asset {
    state.load_asset("/index.html")
}

async fn serve_asset(State(state): State<AppState>, Path(path): Path<String>) -> Asset {
    state.load_asset(&format!("/assets/{path}"))
}
