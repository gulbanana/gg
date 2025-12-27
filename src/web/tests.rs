use super::create_app;
use crate::{RunOptions, config::tests::settings_with_gg_defaults};
use anyhow::Result;
use axum::{body::Body, http};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn integration_test() -> Result<()> {
    let (app, _shutdown_rx) = create_app(RunOptions {
        context: tauri::generate_context!(),
        settings: settings_with_gg_defaults(),
        workspace: None,
        debug: false,
        is_child: false,
    })?;

    let request = http::Request::builder()
        .uri("/")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await?;
    assert_eq!(response.status(), 200);

    let body_bytes = response.into_body().collect().await?.to_bytes().to_vec();
    let body_str = String::from_utf8(body_bytes)?;
    assert_eq!(&body_str[..15], "<!doctype html>");

    Ok(())
}
