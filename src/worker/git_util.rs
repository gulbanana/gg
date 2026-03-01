//! conceptually similar to jj_cli::git_util, except non-blocking
//!
//! gg acts as server (started by a user) and client (started by git) to implement the askpass
//! protocol. its own ipc protocol is trivial, yet underspecified: the client sends newline-delimited
//! prompts to the server, which responds with either OK:(existing InputResponse) or NO if it needs
//! to provision an InputRequest first.

use std::{
    collections::HashMap,
    ffi::OsString,
    io,
    sync::{Arc, Mutex},
};

use jj_lib::git::{GitProgress, GitSidebandLineTerminator, GitSubprocessCallback};

use crate::{
    askpass::serve_askpass,
    messages::{
        InputField, InputRequest, InputResponse, MultilineString, mutations::MutationResult,
    },
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
    ///
    /// When `enable_askpass` is `false`, the askpass IPC server is skipped and
    /// the callback receives an empty environment map. Progress reporting via
    /// `SinkSubprogressCallback` still works regardless.
    pub fn with_callbacks<T>(
        &mut self,
        sink: Option<Arc<dyn crate::worker::EventSink>>,
        enable_askpass: bool,
        f: impl FnOnce(&mut SinkSubprogressCallback, HashMap<OsString, OsString>) -> T,
    ) -> T {
        let mut callback = SinkSubprogressCallback(sink);

        if enable_askpass {
            let askpass_handle = serve_askpass(self.input.clone(), Arc::clone(&self.prompts));
            f(&mut callback, askpass_handle.environment.clone())
        } else {
            f(&mut callback, HashMap::new())
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

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};

    use interprocess::local_socket::{GenericFilePath, Stream, ToFsName as _, traits::Stream as _};

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

        ctx.with_callbacks(None, true, |_cb, environment| {
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

        ctx.with_callbacks(None, true, |_cb, environment| {
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
