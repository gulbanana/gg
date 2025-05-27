//! Sometimes callbacks are buried deep in library code, requiring user input.
//! This module offers an overcomplicated and fragile solution.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc::channel,
};

use anyhow::Result;
use jj_lib::{git::RemoteCallbacks, repo::MutableRepo};
use tauri::{Emitter, Manager, Window};

use crate::{
    AppState,
    messages::{InputField, InputRequest},
    worker::WorkerCallbacks,
};

pub struct FrontendCallbacks(pub Window);

impl WorkerCallbacks for FrontendCallbacks {
    fn with_git(
        &self,
        repo: &mut MutableRepo,
        f: &dyn Fn(&mut MutableRepo, RemoteCallbacks<'_>) -> Result<()>,
    ) -> Result<()> {
        let mut cb = RemoteCallbacks::default();

        let get_ssh_keys = &mut get_ssh_keys;
        cb.get_ssh_keys = Some(get_ssh_keys);

        let get_password = &mut |url: &str, username: &str| {
            self.request_input(
                format!("Please enter a password for {} at {}", username, url),
                ["Password"],
            )
            .and_then(|mut fields| fields.remove("Password"))
        };
        cb.get_password = Some(get_password);

        let get_username_password = &mut |url: &str| {
            self.request_input(
                format!("Please enter a username and password for {}", url),
                ["Username", "Password"],
            )
            .and_then(|mut fields| {
                fields.remove("Username").and_then(|username| {
                    fields
                        .remove("Password")
                        .map(|password| (username, password))
                })
            })
        };
        cb.get_username_password = Some(get_username_password);

        f(repo, cb)
    }
}

impl FrontendCallbacks {
    fn request_input<T: IntoIterator<Item = U>, U: Into<InputField>>(
        &self,
        detail: String,
        fields: T,
    ) -> Option<HashMap<String, String>> {
        log::debug!("request input");

        // initialise a channel to receive responses
        let (tx, rx) = channel();
        self.0.state::<AppState>().set_input(self.0.label(), tx);

        // send the request
        match self.0.emit(
            "gg://input",
            InputRequest {
                title: String::from("Git Login"),
                detail,
                fields: fields.into_iter().map(|field| field.into()).collect(),
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
