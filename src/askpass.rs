//! Credential relay for git/ssh operations.
//!
//! Git and SSH support delegating credential prompts to an external program via
//! `GIT_ASKPASS` / `SSH_ASKPASS`. GG uses this by setting those variables to point
//! at its own binary, then communicating over a local socket so the main process
//! can supply (or record) credentials without a terminal.
//!
//! Flow:
//! 1. Worker operations spin up a named-pipe/unix-socket listener and set env
//!    vars so that git/ssh will invoke the current binary as the askpass program.
//! 2. Git spawns `gg` (or your binary) with the prompt as argv[1]. [`run_askpass`]
//!    detects `GG_ASKPASS_SOCKET`, enters client mode, and forwards the prompt via IPC.
//! 3. The server replies `OK:<credential>` or `NO`, and the client prints to stdout
//!    (which git reads) or exits non-zero.
use crate::messages::InputResponse;
use anyhow::{Context, Result, anyhow};
use interprocess::local_socket::{
    GenericFilePath, Listener, ListenerNonblockingMode, ListenerOptions, Stream, ToFsName,
    traits::{Listener as _, Stream as _},
};
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
    thread::{self, JoinHandle},
    time::Duration,
};
use uuid::Uuid;

/// Handle to a running askpass server. Stopped automatically on drop.
pub(crate) struct AskpassThread {
    pub environment: HashMap<OsString, OsString>,
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for AskpassThread {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Entry point when `gg` is re-invoked as an askpass helper by git/ssh.
///
/// Returns `Some` if `GG_ASKPASS_SOCKET` is set (i.e. we were spawned as a
/// credential helper), `None` otherwise so the caller can continue with
/// normal startup.
pub fn run_askpass() -> Option<Result<()>> {
    let socket_path = env::var("GG_ASKPASS_SOCKET").ok()?;
    Some(askpass_client(socket_path).context("askpass_client"))
}

/// Start an askpass server that listens for credential requests from child
/// git/ssh processes. `input` supplies known credentials; any unrecognized
/// prompts are recorded into `prompts` so the UI can surface them.
///
/// Failure to bind the socket is non-fatal — most operations don't need
/// credentials, so we just return an empty environment in that case.
pub(crate) fn serve_askpass(
    input: Option<InputResponse>,
    prompts: Arc<Mutex<Vec<String>>>,
) -> AskpassThread {
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
                let response = input;
                let prompts_clone = Arc::clone(&prompts);
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

    AskpassThread {
        environment,
        stop_flag,
        handle,
    }
}

/// Client side: connect to the server, send the prompt from argv[1], and
/// print the credential to stdout for git/ssh to read.
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

/// Server loop: accepts connections until `stop_flag` is set. Uses
/// non-blocking mode so we can poll the flag between accept attempts.
pub(crate) fn askpass_server(
    listener: Listener,
    response: Option<InputResponse>,
    prompts: Arc<Mutex<Vec<String>>>,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    // so that we can check stop_flag
    listener
        .set_nonblocking(ListenerNonblockingMode::Both)
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

/// Handle a single askpass connection: read the prompt, look it up in the
/// supplied credentials, and reply `OK:<value>` or `NO`.
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
