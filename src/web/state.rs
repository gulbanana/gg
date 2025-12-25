use std::{
    sync::{Arc, Mutex, mpsc::Sender},
    time::{Duration, Instant},
};

use axum::{
    body::Bytes,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use tauri_plugin_http::reqwest;
use tauri_utils::mime_type::MimeType;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::worker::SessionEvent;

const TAURI_DEV: bool = cfg!(not(feature = "custom-protocol"));

#[derive(Clone)]
pub struct AppState {
    context: Arc<tauri::Context<tauri::Wry>>,
    http_client: reqwest::Client,
    pub worker_tx: Sender<SessionEvent>,
    shutdown_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    last_heartbeat: Arc<Mutex<Instant>>,
    pending_shutdown: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl AppState {
    pub fn new(
        context: tauri::Context<tauri::Wry>,
        worker_tx: Sender<SessionEvent>,
        shutdown_tx: oneshot::Sender<()>,
    ) -> Self {
        Self {
            context: Arc::new(context),
            http_client: reqwest::Client::new(),
            worker_tx,
            shutdown_tx: Arc::new(Mutex::new(Some(shutdown_tx))),
            last_heartbeat: Arc::new(Mutex::new(Instant::now())),
            pending_shutdown: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn load_asset(&self, path: &str) -> Result<Asset, StatusCode> {
        if TAURI_DEV {
            self.load_proxy(path).await
        } else {
            self.load_embedded(path)
        }
    }

    async fn load_proxy(&self, path_and_query: &str) -> Result<Asset, StatusCode> {
        let dev_url = format!("http://localhost:6973{}", path_and_query);
        let resp = self
            .http_client
            .get(&dev_url)
            .send()
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;

        if !resp.status().is_success() {
            return Err(StatusCode::from_u16(resp.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR));
        }

        let mime_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let data = resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

        Ok(Asset { mime_type, data })
    }

    fn load_embedded(&self, path: &str) -> Result<Asset, StatusCode> {
        let data: Bytes = self
            .context
            .assets
            .get(&path.into())
            .map(|data| Bytes::copy_from_slice(&data))
            .ok_or(StatusCode::NOT_FOUND)?;
        let mime_type = MimeType::parse(&data, path);
        Ok(Asset { mime_type, data })
    }

    pub fn keep_alive(&self) {
        *self.last_heartbeat.lock().unwrap() = Instant::now();
    }

    pub fn is_dead(&self) -> bool {
        let elapsed: Duration = self.last_heartbeat.lock().unwrap().elapsed();
        if elapsed > Duration::from_secs(600) {
            log::debug!("no heartbeat");
            if let Some(tx) = self.shutdown_tx.lock().unwrap().take() {
                let _ = tx.send(());
            }
            true
        } else {
            false
        }
    }

    pub fn request_shutdown(&self) {
        let shutdown_tx = self.shutdown_tx.clone();
        let handle = tokio::spawn(async move {
            // grace period to allow reloads
            tokio::time::sleep(Duration::from_secs(1)).await;
            if let Some(tx) = shutdown_tx.lock().unwrap().take() {
                let _ = tx.send(());
            }
        });

        *self.pending_shutdown.lock().unwrap() = Some(handle);
        log::debug!("shutdown requested, waiting...");
    }

    pub fn cancel_shutdown(&self) {
        if let Some(handle) = self.pending_shutdown.lock().unwrap().take() {
            handle.abort();
            log::debug!("shutdown cancelled");
        }
    }
}

pub struct Asset {
    mime_type: String,
    data: Bytes,
}

impl IntoResponse for Asset {
    fn into_response(self) -> Response {
        ([(header::CONTENT_TYPE, self.mime_type)], self.data).into_response()
    }
}
