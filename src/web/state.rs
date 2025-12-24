use std::sync::Arc;

use axum::{
    body::Bytes,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use tauri_plugin_http::reqwest;
use tauri_utils::mime_type::MimeType;

const TAURI_DEV: bool = cfg!(not(feature = "custom-protocol"));

#[derive(Clone)]
pub struct AppState {
    context: Arc<tauri::Context<tauri::Wry>>,
    http_client: reqwest::Client,
}

impl AppState {
    pub fn new(context: tauri::Context<tauri::Wry>) -> Self {
        Self {
            context: Arc::new(context),
            http_client: reqwest::Client::new(),
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
