//! conceptually similar to jj_cli::git_util, except non-blocking
//!
//! gg acts as server (started by a user) and client (started by git) to implement the askpass
//! protocol. its own ipc protocol is trivial, yet underspecified: the client sends newline-delimited
//! prompts to the server, which responds with either OK:<existing InputResponse> or NO if it needs
//! to provision an InputRequest first.

use crate::messages::{InputField, InputRequest, InputResponse, MultilineString, MutationResult};
use crate::worker::EventSinkExt;
use anyhow::{Context, Result, anyhow};
use interprocess::local_socket::{
    GenericFilePath, ListenerOptions, Stream, ToFsName,
    traits::{Listener as _, Stream as _},
};
use jj_lib::git::{Progress, RemoteCallbacks};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use uuid::Uuid;

static CRITICAL_SECTION: Mutex<()> = Mutex::new(());

pub struct AuthContext {
    input: Option<InputResponse>,
    prompts: Arc<Mutex<Vec<String>>>,
}

impl AuthContext {
    pub fn new(input: Option<InputResponse>) -> Self {
        Self {
            input,
            prompts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Run a callback with git authentication and optional progress reporting.
    pub fn with_callbacks<T>(
        &mut self,
        sink: Option<Arc<dyn crate::worker::EventSink>>,
        f: impl FnOnce(RemoteCallbacks) -> T,
    ) -> T {
        let _env_guard: MutexGuard<'_, ()> =
            CRITICAL_SECTION.lock().expect("critical section poisoned");

        let stop_flag = Arc::new(AtomicBool::new(false));

        // socket for the askpass process to call back into this process
        let socket_name = format!("gg-askpass-{}", Uuid::new_v4());
        let socket_path = if cfg!(windows) {
            PathBuf::from(format!(r"\\.\pipe\{}", socket_name))
        } else {
            std::env::temp_dir().join(format!("{}.sock", socket_name))
        };

        // try to set up IPC, but allow failure - most people don't actually need it
        let handle = match socket_path.clone().to_fs_name::<GenericFilePath>() {
            Ok(name) => match ListenerOptions::new().name(name).create_sync() {
                Ok(listener) => {
                    let response = self.input.clone();
                    let prompts_clone = Arc::clone(&self.prompts);
                    let stop_flag_clone = Arc::clone(&stop_flag);
                    #[cfg(unix)]
                    let socket_path_clone = socket_path.clone();

                    Some(thread::spawn(move || {
                        if let Err(err) =
                            askpass_server(listener, response, prompts_clone, stop_flag_clone)
                        {
                            log::error!("askpass_server: {:#?}", err);
                        }
                        #[cfg(unix)]
                        let _ = std::fs::remove_file(&socket_path_clone);
                    }))
                }
                Err(e) => {
                    log::warn!("Failed to start askpass server: {}", e);
                    None
                }
            },
            Err(e) => {
                log::warn!("Invalid askpass socket path: {}", e);
                None
            }
        };

        // if we did manage to start a server, specify an environment variable which causes gg to run as a client
        let env_set = if handle.is_some()
            && let Ok(exe_path) = std::env::current_exe()
        {
            // SAFETY: called in a critical section, doesn't do any FFI
            // even so, we should remove this as soon as https://github.com/jj-vcs/jj/pull/8428 is merged
            unsafe {
                std::env::set_var("GIT_ASKPASS", &exe_path);
                std::env::set_var("GIT_TERMINAL_PROMPT", "0");
                std::env::set_var("SSH_ASKPASS", &exe_path);
                std::env::set_var("SSH_ASKPASS_REQUIRE", "force");
                std::env::set_var("GG_ASKPASS_SOCKET", &socket_path);
            }
            true
        } else {
            false
        };

        // legacy libgit2 callbacks, no longer used by jj-lib but kept here until support is formally removed
        let mut callbacks = RemoteCallbacks::default();

        let mut get_ssh_keys = Self::get_ssh_keys;
        callbacks.get_ssh_keys = Some(&mut get_ssh_keys);

        let mut get_password = |url: &str, username: &str| self.get_password(url, username);
        callbacks.get_password = Some(&mut get_password);

        let mut get_username_password = |url: &str| self.get_username_password(url);
        callbacks.get_username_password = Some(&mut get_username_password);

        // progress callbacks, delegating to a mode-specific event sink
        let mut progress_cb;
        let mut sideband_cb;
        if let Some(sink1) = sink {
            let sink2 = sink1.clone();

            progress_cb = Some(move |progress: &Progress| {
                sink2.send_typed(
                    "gg://progress",
                    &crate::messages::ProgressEvent::Progress {
                        overall_percent: (progress.overall * 100.0) as u32,
                        bytes_downloaded: progress.bytes_downloaded,
                    },
                );
            });
            callbacks.progress = progress_cb
                .as_mut()
                .map(|cb| cb as &mut dyn FnMut(&Progress));

            sideband_cb = Some(move |message: &[u8]| {
                if let Ok(text) = std::str::from_utf8(message) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        sink1.send_typed(
                            "gg://progress",
                            &crate::messages::ProgressEvent::Message {
                                text: trimmed.to_string(),
                            },
                        );
                    }
                }
            });
            callbacks.sideband_progress =
                sideband_cb.as_mut().map(|cb| cb as &mut dyn FnMut(&[u8]));
        }

        self.run_with_callbacks(callbacks, f, stop_flag, handle, env_set)
    }

    fn run_with_callbacks<T>(
        &self,
        callbacks: RemoteCallbacks,
        f: impl FnOnce(RemoteCallbacks) -> T,
        stop_flag: Arc<AtomicBool>,
        handle: Option<thread::JoinHandle<()>>,
        env_set: bool,
    ) -> T {
        let result = f(callbacks);

        // shut down the server
        stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = handle {
            let _ = handle.join();
        }

        // clean up the environment before exiting the global lock
        if env_set {
            // SAFETY: called in a critical section, doesn't do any FFI
            unsafe {
                std::env::remove_var("GIT_ASKPASS");
                std::env::remove_var("GIT_TERMINAL_PROMPT");
                std::env::remove_var("SSH_ASKPASS");
                std::env::remove_var("SSH_ASKPASS_REQUIRE");
                std::env::remove_var("GG_ASKPASS_SOCKET");
            }
        }

        result
    }

    // simplistic, but it's the same as the version in jj_cli::git_util
    fn get_ssh_keys(_username: &str) -> Vec<PathBuf> {
        let mut paths = vec![];
        if let Some(home_dir) = dirs::home_dir() {
            let ssh_dir = Path::new(&home_dir).join(".ssh");
            for filename in ["id_ed25519_sk", "id_ed25519", "id_rsa"] {
                let key_path = ssh_dir.join(filename);
                if key_path.is_file() {
                    log::debug!("found ssh key {key_path:?}");
                    paths.push(key_path);
                }
            }
        }
        if paths.is_empty() {
            log::warn!("No SSH key found.");
        }
        paths
    }

    // return input if available, record requirement otherwise
    fn get_password(&self, url: &str, username: &str) -> Option<String> {
        let password_prompt = format!("Password for '{username}@{url}':");
        if let Some(input) = &self.input
            && let Some(password) = input.fields.get(&password_prompt)
        {
            Some(password.clone())
        } else {
            self.prompts.lock().unwrap().push(password_prompt);
            None
        }
    }

    // return inputs if available, record requirement otherwise
    fn get_username_password(&self, url: &str) -> Option<(String, String)> {
        let username_prompt = format!("Username for '{url}':");
        let password_prompt = format!("Password for '{url}':");
        if let Some(input) = &self.input
            && let (Some(username), Some(password)) = (
                input.fields.get(&username_prompt),
                input.fields.get(&password_prompt),
            )
        {
            Some((username.clone(), password.clone()))
        } else {
            let mut prompts = self.prompts.lock().unwrap();
            prompts.push(username_prompt);
            prompts.push(password_prompt);
            None
        }
    }

    pub fn into_result(self, err: anyhow::Error) -> MutationResult {
        let prompts = self.prompts.lock().unwrap();
        match prompts.len() {
            0 => MutationResult::InternalError {
                message: MultilineString::from(err.to_string().as_str()),
            },
            _ => MutationResult::InputRequired {
                request: InputRequest {
                    title: String::from("Git Authentication"),
                    detail: String::new(),
                    fields: prompts
                        .iter()
                        .map(|prompt| InputField {
                            label: prompt.clone(),
                            choices: vec![],
                        })
                        .collect(),
                },
            },
        }
    }
}

pub fn run_askpass() -> Option<Result<()>> {
    if let Ok(socket_path) = std::env::var("GG_ASKPASS_SOCKET") {
        Some(askpass_client(socket_path).context("askpass_client"))
    } else {
        None
    }
}

fn askpass_client(socket_path: String) -> Result<()> {
    // the prompt is supplied to askpass as our first argument
    let prompt = std::env::args().nth(1).unwrap_or_default();

    // send it to the server over the socket we've been given
    let name = socket_path
        .to_fs_name::<GenericFilePath>()
        .map_err(|e| anyhow!("invalid socket path: {}", e))?;

    let stream =
        Stream::connect(name).map_err(|e| anyhow!("failed to connect to askpass socket: {}", e))?;

    let mut writer = &stream;
    writeln!(writer, "{}", prompt)?;
    writer.flush()?;

    // the server responds OK or NO
    let mut response = String::new();
    BufReader::new(&stream).read_line(&mut response)?;
    let response = response.trim();

    // write to stdout or exit non-zero
    if let Some(credential) = response.strip_prefix("OK:") {
        println!("{credential}");
        Ok(())
    } else {
        Err(anyhow!("credential unavailable"))
    }
}

fn askpass_server(
    listener: interprocess::local_socket::Listener,
    response: Option<InputResponse>,
    prompts: Arc<Mutex<Vec<String>>>,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    // so that we can check stop_flag
    listener
        .set_nonblocking(interprocess::local_socket::ListenerNonblockingMode::Both)
        .context("set_nonblocking")?;

    while !stop_flag.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok(stream) => {
                handle_askpass_request(stream, &response, &prompts)
                    .context("handle_askpass_request")?;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // spin until the client starts
                thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }

    Ok(())
}

// write either OK:<credential> or NO
fn handle_askpass_request(
    mut stream: Stream,
    response: &Option<InputResponse>,
    prompts: &Mutex<Vec<String>>,
) -> std::io::Result<()> {
    stream.set_nonblocking(false)?;

    let mut reader = BufReader::new(&stream);
    let mut prompt = String::new();
    reader.read_line(&mut prompt)?;

    let prompt = prompt.trim();
    log::debug!("askpass prompt: {}", prompt);

    let credential = if let Some(input) = response
        && let Some(field) = input.fields.get(prompt)
    {
        format!("OK:{}", field)
    } else {
        prompts.lock().unwrap().push(prompt.to_owned());
        "NO".to_string()
    };
    writeln!(stream, "{credential}")?;

    stream.flush()?;
    log::debug!("askpass credential: {}", credential);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::collections::HashMap;

    #[test]
    fn test_context_with_password() {
        let mut fields = HashMap::new();
        fields.insert(
            "Password for 'user@https://github.com':".to_string(),
            "secret".to_string(),
        );
        let input = Some(InputResponse { fields });
        let ctx = AuthContext::new(input);

        let result = ctx.get_password("https://github.com", "user");

        assert_eq!(result, Some("secret".to_string()));
        assert!(ctx.prompts.lock().unwrap().is_empty());
    }

    #[test]
    fn test_context_without_password() {
        let ctx = AuthContext::new(None);

        let result = ctx.get_password("https://github.com", "user");

        assert_eq!(result, None);
        let prompts = ctx.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 1);
        assert_eq!(
            prompts[0],
            "Password for 'user@https://github.com':".to_string()
        );
    }

    #[test]
    fn test_context_with_username_password() {
        let mut fields = HashMap::new();
        fields.insert(
            "Username for 'https://github.com':".to_string(),
            "user".to_string(),
        );
        fields.insert(
            "Password for 'https://github.com':".to_string(),
            "secret".to_string(),
        );
        let input = Some(InputResponse { fields });
        let ctx = AuthContext::new(input);

        let result = ctx.get_username_password("https://github.com");

        assert_eq!(result, Some(("user".to_string(), "secret".to_string())));
    }

    #[test]
    fn test_context_without_username_password() {
        let ctx = AuthContext::new(None);

        let result = ctx.get_username_password("https://github.com");

        assert_eq!(result, None);
        let prompts = ctx.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 2);
        assert_eq!(prompts[0], "Username for 'https://github.com':".to_string());
        assert_eq!(prompts[1], "Password for 'https://github.com':".to_string());
    }

    #[test]
    fn test_into_result_with_auth_required() {
        let ctx = AuthContext::new(None);
        ctx.get_password("https://github.com", "user"); // records auth requirement

        let result = ctx.into_result(anyhow::anyhow!("some error"));

        assert_matches!(result, MutationResult::InputRequired { .. });
    }

    #[test]
    fn test_into_result_without_auth_required() {
        let ctx = AuthContext::new(None);

        let result = ctx.into_result(anyhow::anyhow!("network error"));

        assert_matches!(result, MutationResult::InternalError { .. });
    }

    #[test]
    fn test_askpass_ipc_with_credentials() {
        let mut fields = HashMap::new();
        fields.insert("Password".to_string(), "secret123".to_string());
        let input = Some(InputResponse { fields });
        let mut ctx = AuthContext::new(input);

        // with_callbacks sets up the askpass server, so we test IPC inside it
        ctx.with_callbacks(None, |_cb| {
            let socket_path = std::env::var("GG_ASKPASS_SOCKET").expect("socket env set");
            let name = socket_path
                .to_fs_name::<GenericFilePath>()
                .expect("valid path");
            let stream = Stream::connect(name).expect("connect");

            let mut writer = &stream;
            writeln!(writer, "Password").expect("write");
            writer.flush().expect("flush");

            let mut response = String::new();
            BufReader::new(&stream)
                .read_line(&mut response)
                .expect("read");

            assert_eq!(response.trim(), "OK:secret123");
        });
    }

    #[test]
    fn test_askpass_ipc_without_credentials() {
        let mut ctx = AuthContext::new(None);

        ctx.with_callbacks(None, |_cb| {
            let socket_path = std::env::var("GG_ASKPASS_SOCKET").expect("socket env set");
            let name = socket_path
                .to_fs_name::<GenericFilePath>()
                .expect("valid path");
            let stream = Stream::connect(name).expect("connect");

            let mut writer = &stream;
            writeln!(writer, "Username for 'https://github.com':").expect("write");
            writer.flush().expect("flush");

            let mut response = String::new();
            BufReader::new(&stream)
                .read_line(&mut response)
                .expect("read");

            assert_eq!(response.trim(), "NO");
        });

        // Prompts are collected after with_callbacks returns
        let prompts = ctx.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0], "Username for 'https://github.com':");
    }
}
