use std::{
    collections::HashMap,
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
    clients: Arc<Mutex<HashMap<String, Instant>>>,
    has_ever_connected: Arc<Mutex<bool>>,
    pending_disconnects: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    client_timeout: Duration,
}

impl AppState {
    pub fn new(
        context: tauri::Context<tauri::Wry>,
        worker_tx: Sender<SessionEvent>,
        shutdown_tx: oneshot::Sender<()>,
        client_timeout: Duration,
    ) -> Self {
        Self {
            context: Arc::new(context),
            http_client: reqwest::Client::new(),
            worker_tx,
            shutdown_tx: Arc::new(Mutex::new(Some(shutdown_tx))),
            clients: Arc::new(Mutex::new(HashMap::new())),
            has_ever_connected: Arc::new(Mutex::new(false)),
            pending_disconnects: Arc::new(Mutex::new(HashMap::new())),
            client_timeout,
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

    // a client just checked in. cancel pending disconnects and make sure they're tracked
    pub fn keep_alive(&self, client_id: String) {
        if let Some(handle) = self.pending_disconnects.lock().unwrap().remove(&client_id) {
            handle.abort(); // cancels the last_rites 
        }

        let is_new = self
            .clients
            .lock()
            .unwrap()
            .insert(client_id.clone(), Instant::now())
            .is_none();

        if is_new {
            log::debug!("client connected: {client_id}");
        }

        // begin reference counting
        *self.has_ever_connected.lock().unwrap() = true;
    }

    // a client is checking out. give them a moment to change their mind and then stop tracking
    pub fn last_rites(&self, client_id: String) {
        let clients = self.clients.clone();
        let shutdown_tx = self.shutdown_tx.clone();
        let pending_disconnects = self.pending_disconnects.clone();
        let client_id_for_task = client_id.clone();

        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await; // grace period to allow F5

            pending_disconnects
                .lock()
                .unwrap()
                .remove(&client_id_for_task);

            clients.lock().unwrap().remove(&client_id_for_task);
            log::debug!("client disconnected: {client_id_for_task}");

            // everyone else may also be dead
            if clients.lock().unwrap().is_empty() {
                log::debug!("no clients remaining, shutting down");
                if let Some(tx) = shutdown_tx.lock().unwrap().take() {
                    let _ = tx.send(());
                }
            }
        });

        self.pending_disconnects
            .lock()
            .unwrap()
            .insert(client_id, handle);
    }

    // poll-based death check - once the heartbeat starts, it must not stop
    pub fn is_dead(&self) -> bool {
        let has_ever_connected = *self.has_ever_connected.lock().unwrap();
        if !has_ever_connected {
            return false;
        }

        // Remove stale clients (no heartbeat for 10 minutes)
        let timeout = Duration::from_secs(600);
        let mut clients = self.clients.lock().unwrap();
        let stale: Vec<String> = clients
            .iter()
            .filter(|(_, last_seen)| last_seen.elapsed() > timeout)
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale {
            log::debug!("client timed out: {id}");
            clients.remove(&id);
        }

        if clients.is_empty() {
            log::debug!("no clients remaining");
            if let Some(tx) = self.shutdown_tx.lock().unwrap().take() {
                let _ = tx.send(());
            }
            true
        } else {
            false
        }
    }
}

pub struct Asset {
    mime_type: String,
    data: Bytes,
}

impl Asset {
    pub fn data(&self) -> &Bytes {
        &self.data
    }
}

impl IntoResponse for Asset {
    fn into_response(self) -> Response {
        ([(header::CONTENT_TYPE, self.mime_type)], self.data).into_response()
    }
}
