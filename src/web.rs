use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use axum::{Router, routing::get};
use jj_lib::settings::UserSettings;

#[tokio::main]
pub async fn run_web(_workspace: Option<PathBuf>, _settings: UserSettings) -> Result<()> {
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));

    // bind to random port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;

    let addr = listener.local_addr()?;
    let url = format!("http://{}", addr);
    println!("Listening on {}", url);

    tokio::task::spawn_blocking(move || {
        let _ = webbrowser::open(&url); // best-effort
    });

    let server = axum::serve(listener, app).with_graceful_shutdown(async {
        tokio::time::sleep(Duration::from_mins(10)).await;
        println!("Shutting down after 10 minutes");
    });

    server.await?;
    Ok(())
}
