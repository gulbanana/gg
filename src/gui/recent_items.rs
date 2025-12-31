//! macos "recent items" integration

use super::try_create_window;
use tauri::{
    RunEvent, Wry,
    plugin::{Builder, TauriPlugin},
};

pub fn init() -> TauriPlugin<Wry> {
    Builder::new("gg-recent-items")
        .on_event(|app, event| {
            if let RunEvent::Opened { urls } = event {
                for url in urls {
                    if url.scheme() == "file" {
                        if let Ok(path) = url.to_file_path() {
                            log::debug!("open recent item: {:?}", path);

                            // avoid main-thread deadlock
                            let app = app.clone();
                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = try_create_window(&app, Some(path.clone())) {
                                    log::error!("Failed to open workspace {:?}: {}", path, e);
                                }
                            });
                        }
                    }
                }
            }
        })
        .build()
}
