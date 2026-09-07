use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use interprocess::local_socket::{
    GenericFilePath, ListenerOptions, Stream, ToFsName,
    traits::{ListenerExt, Stream as _},
};
use tauri::AppHandle;

fn socket_path() -> PathBuf {
    std::env::temp_dir().join("gg-instance.sock")
}

/// Try to forward a workspace open request to an already-running GG instance.
/// Returns true if another instance accepted the request (caller should exit).
pub fn try_forward(workspace: Option<&PathBuf>) -> bool {
    let path = socket_path();
    let name = match path.to_fs_name::<GenericFilePath>() {
        Ok(n) => n,
        Err(_) => return false,
    };

    let stream = match Stream::connect(name) {
        Ok(s) => s,
        Err(_) => return false,
    };

    // resolve None to cwd here so the server opens the right directory
    let resolved = workspace.cloned().or_else(|| std::env::current_dir().ok());
    let line = match resolved {
        Some(p) => format!("{}\n", p.display()),
        None => "\n".to_string(),
    };

    let mut writer = &stream;
    writer.write_all(line.as_bytes()).ok();
    writer.flush().ok();
    true
}

/// Start listening for forwarded open requests from subsequent GG launches.
/// Silently returns if the socket cannot be bound (e.g. race with another instance).
pub fn listen(app_handle: AppHandle) {
    let path = socket_path();
    let _ = std::fs::remove_file(&path); // clean up any stale socket

    let name = match path.to_fs_name::<GenericFilePath>() {
        Ok(n) => n,
        Err(_) => return,
    };

    let listener = match ListenerOptions::new().name(name).create_sync() {
        Ok(l) => l,
        Err(e) => {
            log::warn!("single-instance listener failed to bind: {e}");
            return;
        }
    };

    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let stream = match conn {
                Ok(s) => s,
                Err(_) => break,
            };

            let app = app_handle.clone();
            std::thread::spawn(move || {
                let mut line = String::new();
                BufReader::new(stream).read_line(&mut line).ok();
                let workspace = {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(trimmed))
                    }
                };
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = super::try_create_window(&app, workspace.clone()) {
                        log::error!("single-instance open {:?}: {e}", workspace);
                    }
                });
            });
        }
    });
}
