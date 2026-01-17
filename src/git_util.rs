//! conceptually similar to jj_cli::git_util, except non-blocking
//!
//! gg acts as server (started by a user) and client (started by git) to implement the askpass
//! protocol. its own ipc protocol is trivial, yet underspecified: the client sends newline-delimited
//! prompts to the server, which responds with either OK:<existing InputResponse> or NO if it needs
//! to provision an InputRequest first.

use std::{
    collections::HashMap,
    env,
    ffi::OsString,
    io::{self, BufRead, BufReader, ErrorKind, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use interprocess::local_socket::{
    GenericFilePath, ListenerOptions, Stream, ToFsName,
    traits::{Listener as _, Stream as _},
};
use jj_lib::git::{GitProgress, GitSidebandLineTerminator, GitSubprocessCallback};
use uuid::Uuid;

use crate::{
    messages::{InputField, InputRequest, InputResponse, MultilineString, MutationResult},
    worker::EventSinkExt,
};

pub struct AuthContext {
    input: Option<InputResponse>,
    prompts: Arc<Mutex<Vec<String>>>,
}

pub struct SinkSubprogressCallback(Option<Arc<dyn crate::worker::EventSink>>);

impl GitSubprocessCallback for SinkSubprogressCallback {
    fn needs_progress(&self) -> bool {
        self.0.is_some()
    }

    fn progress(&mut self, progress: &GitProgress) -> io::Result<()> {
        if let Some(sink) = &self.0 {
            sink.send_typed(
                "gg://progress",
                &crate::messages::ProgressEvent::Progress {
                    overall_percent: (progress.overall() * 100.0) as u32,
                },
            );
        }
        Ok(())
    }

    fn local_sideband(
        &mut self,
        message: &[u8],
        _term: Option<GitSidebandLineTerminator>,
    ) -> io::Result<()> {
        if let Some(sink) = &self.0
            && let Ok(text) = std::str::from_utf8(message)
        {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                sink.send_typed(
                    "gg://progress",
                    &crate::messages::ProgressEvent::Message {
                        text: trimmed.to_string(),
                    },
                );
            }
        }
        Ok(())
    }

    fn remote_sideband(
        &mut self,
        message: &[u8],
        _term: Option<GitSidebandLineTerminator>,
    ) -> io::Result<()> {
        if let Some(sink) = &self.0
            && let Ok(text) = std::str::from_utf8(message)
        {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                sink.send_typed(
                    "gg://progress",
                    &crate::messages::ProgressEvent::Message {
                        text: format!("remote: {trimmed}"),
                    },
                );
            }
        }
        Ok(())
    }
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
        f: impl FnOnce(&mut SinkSubprogressCallback, HashMap<OsString, OsString>) -> T,
    ) -> T {
        let stop_flag = Arc::new(AtomicBool::new(false));

        // socket for the askpass process to call back into this process
        let socket_name = format!("gg-askpass-{}", Uuid::new_v4());
        let socket_path = if cfg!(windows) {
            PathBuf::from(format!(r"\\.\pipe\{}", socket_name))
        } else {
            env::temp_dir().join(format!("{}.sock", socket_name))
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

        // if we did manage to start a server, specify environment variables which cause git to invoke gg as a client
        let environment = if handle.is_some()
            && let Ok(exe_path) = env::current_exe()
        {
            HashMap::from([
                ("GIT_ASKPASS".into(), exe_path.clone().into()),
                ("GIT_TERMINAL_PROMPT".into(), "0".into()),
                ("SSH_ASKPASS".into(), exe_path.into()),
                ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
                ("GG_ASKPASS_SOCKET".into(), socket_path.into()),
            ])
        } else {
            HashMap::new()
        };

        let mut callback = SinkSubprogressCallback(sink);
        let result = f(&mut callback, environment);

        // shut down the server
        stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = handle {
            let _ = handle.join();
        }

        result
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
    if let Ok(socket_path) = env::var("GG_ASKPASS_SOCKET") {
        Some(askpass_client(socket_path).context("askpass_client"))
    } else {
        None
    }
}

fn askpass_client(socket_path: String) -> Result<()> {
    // the prompt is supplied to askpass as our first argument
    let prompt = env::args().nth(1).unwrap_or_default();

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
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                // spin until the client starts
                thread::sleep(Duration::from_millis(100));
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
) -> io::Result<()> {
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

        ctx.with_callbacks(None, |_cb, environment| {
            let socket_path = environment
                .get(&OsString::from("GG_ASKPASS_SOCKET"))
                .expect("socket env set")
                .clone();
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

        ctx.with_callbacks(None, |_cb, environment| {
            let socket_path = environment
                .get(&OsString::from("GG_ASKPASS_SOCKET"))
                .expect("socket env set")
                .clone();
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
