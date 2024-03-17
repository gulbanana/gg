//! This module is a temporary compromise allowing coupling from the worker back to the UI.
//! It's not possible to abstract over jj_lib::git::RemoteCallbacks at the moment.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{mpsc::channel, OnceLock},
};

use jj_lib::git::RemoteCallbacks;
use tauri::{Manager, WebviewWindow};

use crate::{messages::InputRequest, AppState};

pub static UI_WINDOW: OnceLock<WebviewWindow> = OnceLock::new();

pub fn with_git<T>(f: impl FnOnce(RemoteCallbacks<'_>) -> T) -> T {
    let mut callbacks = RemoteCallbacks::default();

    let get_ssh_keys = &mut get_ssh_keys;
    callbacks.get_ssh_keys = Some(get_ssh_keys);

    let get_password = &mut |url: &str, username: &str| {
        request_input(
            format!("Please enter a password for {} at {}", username, url),
            ["Password".into()],
        )
        .and_then(|mut fields| fields.remove("Password"))
    };
    callbacks.get_password = Some(get_password);

    let get_username_password = &mut |url: &str| {
        request_input(
            format!("Please enter a username and password for {}", url),
            ["Username".into(), "Password".into()],
        )
        .and_then(|mut fields| {
            fields.remove("Username").and_then(|username| {
                fields
                    .remove("Password")
                    .map(|password| (username, password))
            })
        })
    };
    callbacks.get_username_password = Some(get_username_password);

    f(callbacks)
}

fn request_input<T: IntoIterator<Item = String>>(
    detail: String,
    fields: T,
) -> Option<HashMap<String, String>> {
    log::debug!("request input");

    // get the global [note: :-(] user-input window
    let window = match UI_WINDOW.get() {
        Some(window) => window.clone(),
        None => {
            log::error!("input request failed: UI_WINDOW not set");
            return None;
        }
    };

    // initialise a channel to receive responses
    let (tx, rx) = channel();
    window.state::<AppState>().set_input(window.label(), tx);

    // send the request
    match window.emit(
        "gg://input",
        InputRequest {
            title: String::from("Git login"),
            detail,
            fields: fields.into_iter().collect(),
        },
    ) {
        Ok(_) => (),
        Err(err) => {
            log::error!("input request failed: emit failed: {err}");
            return None;
        }
    }

    // wait for the response
    match rx.recv() {
        Ok(response) => {
            if response.cancel {
                log::error!("input request failed: input cancelled");
                None
            } else {
                Some(response.fields)
            }
        }
        Err(err) => {
            log::error!("input request failed: {err}");
            None
        }
    }
}

// simplistic, but it's the same as the version in jj_cli::git_util
fn get_ssh_keys(_username: &str) -> Vec<PathBuf> {
    let mut paths = vec![];
    if let Some(home_dir) = dirs::home_dir() {
        let ssh_dir = Path::new(&home_dir).join(".ssh");
        for filename in ["id_ed25519_sk", "id_ed25519", "id_rsa"] {
            let key_path = ssh_dir.join(filename);
            if key_path.is_file() {
                log::info!("found ssh key {key_path:?}");
                paths.push(key_path);
            }
        }
    }
    if paths.is_empty() {
        log::info!("no ssh key found");
    }
    paths
}
