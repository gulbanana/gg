#![cfg_attr(feature = "gui", windows_subsystem = "windows")]

mod callbacks;
mod config;
mod handler;
#[cfg(all(target_os = "macos", not(feature = "gui")))]
mod macos;
mod menu;
mod messages;
#[cfg(windows)]
mod windows;
mod worker;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{Sender, channel};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use jj_lib::config::ConfigSource;
use log::LevelFilter;
use tauri::async_runtime;
use tauri::menu::Menu;
use tauri::{Emitter, Listener, State, Window, WindowEvent, Wry};
use tauri::{Manager, ipc::InvokeError};
use tauri_plugin_window_state::StateFlags;

use messages::{
    AbandonRevisions, BackoutRevisions, CheckoutRevision, CopyChanges, CreateRef, CreateRevision,
    CreateRevisionBetween, DeleteRef, DescribeRevision, DuplicateRevisions, GitFetch, GitPush,
    InputResponse, InsertRevision, MoveChanges, MoveHunk, MoveRef, MoveRevision, MoveSource,
    MutationResult, RenameBranch, RevId, TrackBranch, UndoOperation, UntrackBranch,
};
use worker::{Mutation, Session, SessionEvent, WorkerSession};

use crate::callbacks::FrontendCallbacks;
use crate::messages::CopyHunk;

#[derive(Parser, Debug)]
#[command(version, author)]
struct Args {
    #[arg(
        index(1),
        help = "Open this directory (instead of the current working directory)."
    )]
    workspace: Option<PathBuf>,
    #[arg(short, long, help = "Enable debug logging.")]
    debug: bool,
    #[cfg(not(feature = "gui"))]
    #[arg(
        long,
        help = "Run in foreground (don't spawn a background process).",
        hide = true
    )]
    foreground: bool,
}

#[derive(Default)]
struct AppState(Mutex<HashMap<String, WindowState>>);

struct WindowState {
    _worker: JoinHandle<()>,
    worker_channel: Sender<SessionEvent>,
    input_channel: Option<Sender<InputResponse>>,
    revision_menu: Menu<Wry>,
    tree_menu: Menu<Wry>,
    ref_menu: Menu<Wry>,
}

impl AppState {
    fn get_session(&self, window_label: &str) -> Sender<SessionEvent> {
        self.0
            .lock()
            .expect("state mutex poisoned")
            .get(window_label)
            .expect("session not found")
            .worker_channel
            .clone()
    }

    fn set_input(&self, window_label: &str, tx: Sender<InputResponse>) {
        self.0
            .lock()
            .expect("state mutex poisoned")
            .get_mut(window_label)
            .expect("session not found")
            .input_channel = Some(tx);
    }

    fn take_input(&self, window_label: &str) -> Option<Sender<InputResponse>> {
        self.0
            .lock()
            .expect("state mutex poisoned")
            .get_mut(window_label)
            .expect("session not found")
            .input_channel
            .take()
    }
}

fn main() -> Result<()> {
    // before parsing args, attach a console on windows - will fail if not started from a shell, but that's fine
    #[cfg(windows)]
    {
        windows::reattach_console();
    }

    let args = Args::parse();

    // cargo run/install: act like a CLI that spawns a GUI in the background
    #[cfg(not(feature = "gui"))]
    if !args.foreground {
        spawn_app()
    } else {
        run_app(args)
    }

    #[cfg(feature = "gui")]
    {
        run_app(args)
    }
}

#[cfg(not(feature = "gui"))]
fn spawn_app() -> Result<()> {
    use std::process::{Command, exit};

    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(&exe);
    cmd.arg("--foreground");
    cmd.args(std::env::args().skip(1)); // forward all original arguments

    #[cfg(windows)]
    {
        use ::windows::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};
        use std::os::windows::process::CommandExt;

        // Spawn with DETACHED_PROCESS flag so the child runs independently
        cmd.creation_flags(DETACHED_PROCESS.0 | CREATE_NEW_PROCESS_GROUP.0);

        match cmd.spawn() {
            Err(err) => Err(anyhow!("Failed to spawn GG: {}", err)),
            Ok(_) => exit(0),
        }
    }

    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::process::CommandExt;

        // safety: fork() is ok here because:
        // 1. We're in a single-threaded context (early in main before any threads spawn)
        // 2. We only use async-signal-safe functions in the child before exec
        // 3. The child immediately execs, replacing itself entirely
        // 4. The parent exits without doing anything else
        unsafe {
            match libc::fork() {
                -1 => Err(anyhow!("fork() failed")),

                // child: detach from terminal, redirect stdio and exec
                0 => {
                    if libc::setsid() == -1 {
                        eprintln!("Warning: setsid() failed");
                    }

                    let devnull = CString::new("/dev/null")?;
                    let null_fd = libc::open(devnull.as_ptr(), libc::O_RDWR);
                    if null_fd != -1 {
                        if libc::dup2(null_fd, 0) == -1
                            || libc::dup2(null_fd, 1) == -1
                            || libc::dup2(null_fd, 2) == -1
                        {
                            eprintln!("Warning: failed to redirect stdio");
                        }
                        libc::close(null_fd);
                    }

                    let err = cmd.exec();
                    Err(anyhow!("exec() failed: {}", err))
                }

                // parent: we're done
                _ => exit(0),
            }
        }
    }
}

fn run_app(args: Args) -> Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    StateFlags::SIZE
                        | StateFlags::POSITION
                        | StateFlags::SIZE
                        | StateFlags::FULLSCREEN,
                )
                .build(),
        )
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(LevelFilter::Warn)
                .level_for(
                    "gg",
                    if args.debug {
                        LevelFilter::Debug
                    } else {
                        LevelFilter::Warn
                    },
                )
                .level_for(
                    "tao",
                    if args.debug {
                        LevelFilter::Info
                    } else {
                        LevelFilter::Error
                    },
                )
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            notify_window_ready,
            notify_input,
            forward_accelerator,
            forward_context_menu,
            query_log,
            query_log_next_page,
            query_revision,
            query_remotes,
            abandon_revisions,
            backout_revisions,
            checkout_revision,
            create_revision,
            create_revision_between,
            describe_revision,
            duplicate_revisions,
            insert_revision,
            move_revision,
            move_source,
            move_changes,
            copy_changes,
            move_hunk,
            copy_hunk,
            track_branch,
            untrack_branch,
            rename_branch,
            create_ref,
            delete_ref,
            move_ref,
            git_push,
            git_fetch,
            undo_operation,
            query_recent_workspaces,
            open_workspace_at_path,
        ])
        .menu(menu::build_main)
        .setup(|app| {
            // after tauri initialises NSApplication, set the dock icon in case we're running as CLI
            #[cfg(all(target_os = "macos", not(feature = "gui")))]
            {
                macos::set_dock_icon();
            }

            let window = app
                .get_webview_window("main")
                .ok_or(anyhow!("preconfigured window not found"))?;
            let (sender, receiver) = channel();

            let mut handle = window.as_ref().window();
            let window_worker = thread::spawn(move || {
                async_runtime::block_on(work(handle.clone(), receiver, args.workspace))
            });

            window.on_menu_event(|w, e| handler::fatal!(menu::handle_event(w, e)));

            handle = window.as_ref().window();
            window.on_window_event(move |event| handle_window_event(&handle, event));

            handle = window.as_ref().window();
            window.listen("gg://revision/select", move |event| {
                let payload: Result<Option<messages::RevHeader>, serde_json::Error> =
                    serde_json::from_str(event.payload());
                if let Some(menu) = handle.menu()
                    && let Ok(selection) = payload
                {
                    handler::fatal!(menu::handle_selection(menu, selection));
                }
            });

            let (revision_menu, tree_menu, ref_menu) = menu::build_context(app.handle())?;

            let app_state = app.state::<AppState>();
            app_state.0.lock().unwrap().insert(
                window.label().to_owned(),
                WindowState {
                    _worker: window_worker,
                    worker_channel: sender,
                    input_channel: None,
                    revision_menu,
                    tree_menu,
                    ref_menu,
                },
            );

            Ok(())
        })
        .manage(AppState::default())
        .run(tauri::generate_context!())?;

    Ok(())
}

async fn work(
    window: Window,
    rx: std::sync::mpsc::Receiver<SessionEvent>,
    workspace: Option<PathBuf>,
) {
    log::info!("start worker");

    while let Err(err) = WorkerSession::new(FrontendCallbacks(window.clone()), workspace.clone())
        .handle_events(&rx)
        .await
        .context("worker")
    {
        log::info!("restart worker: {err:#}");

        // it's ok if the worker has to restart, as long as we can notify the frontend of it
        handler::fatal!(window.emit(
            "gg://repo/config",
            messages::RepoConfig::WorkerError {
                message: format!("{err:#}"),
            },
        ));
    }
}

#[tauri::command(async)]
fn notify_window_ready(window: Window) {
    log::debug!("window opened; loading cwd");
    handler::fatal!(window.show());
    handler::nonfatal!(try_open_repository(&window, None));
}

#[tauri::command(async)]
fn notify_input(
    window: Window,
    app_state: State<AppState>,
    response: InputResponse,
) -> Result<(), InvokeError> {
    let response_tx = app_state
        .take_input(window.label())
        .ok_or(anyhow!("Nobody is listening."))
        .map_err(InvokeError::from_anyhow)?;
    response_tx.send(response).map_err(InvokeError::from_error)
}

#[tauri::command]
fn forward_accelerator(window: Window, key: char) {
    if key == 'o' {
        menu::repo_open(&window);
    }
}

#[tauri::command]
fn forward_context_menu(window: Window, context: messages::Operand) -> Result<(), InvokeError> {
    menu::handle_context(window, context).map_err(InvokeError::from_anyhow)?;
    Ok(())
}

#[tauri::command(async)]
fn query_log(
    window: Window,
    app_state: State<AppState>,
    revset: String,
) -> Result<messages::LogPage, InvokeError> {
    let session_tx: Sender<SessionEvent> = app_state.get_session(window.label());
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::QueryLog {
            tx: call_tx,
            query: revset,
        })
        .map_err(InvokeError::from_error)?;
    call_rx
        .recv()
        .map_err(InvokeError::from_error)?
        .map_err(InvokeError::from_anyhow)
}

#[tauri::command(async)]
fn query_log_next_page(
    window: Window,
    app_state: State<AppState>,
) -> Result<messages::LogPage, InvokeError> {
    let session_tx: Sender<SessionEvent> = app_state.get_session(window.label());
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::QueryLogNextPage { tx: call_tx })
        .map_err(InvokeError::from_error)?;
    call_rx
        .recv()
        .map_err(InvokeError::from_error)?
        .map_err(InvokeError::from_anyhow)
}

#[tauri::command(async)]
fn query_revision(
    window: Window,
    app_state: State<AppState>,
    id: RevId,
) -> Result<messages::RevResult, InvokeError> {
    let session_tx: Sender<SessionEvent> = app_state.get_session(window.label());
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::QueryRevision { tx: call_tx, id })
        .map_err(InvokeError::from_error)?;
    call_rx
        .recv()
        .map_err(InvokeError::from_error)?
        .map_err(InvokeError::from_anyhow)
}

#[tauri::command(async)]
fn query_remotes(
    window: Window,
    app_state: State<AppState>,
    tracking_branch: Option<String>,
) -> Result<Vec<String>, InvokeError> {
    let session_tx: Sender<SessionEvent> = app_state.get_session(window.label());
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::QueryRemotes {
            tx: call_tx,
            tracking_branch,
        })
        .map_err(InvokeError::from_error)?;
    call_rx
        .recv()
        .map_err(InvokeError::from_error)?
        .map_err(InvokeError::from_anyhow)
}

#[tauri::command(async)]
fn abandon_revisions(
    window: Window,
    app_state: State<AppState>,
    mutation: AbandonRevisions,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn backout_revisions(
    window: Window,
    app_state: State<AppState>,
    mutation: BackoutRevisions,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn checkout_revision(
    window: Window,
    app_state: State<AppState>,
    mutation: CheckoutRevision,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn create_revision(
    window: Window,
    app_state: State<AppState>,
    mutation: CreateRevision,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn create_revision_between(
    window: Window,
    app_state: State<AppState>,
    mutation: CreateRevisionBetween,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn insert_revision(
    window: Window,
    app_state: State<AppState>,
    mutation: InsertRevision,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn describe_revision(
    window: Window,
    app_state: State<AppState>,
    mutation: DescribeRevision,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn duplicate_revisions(
    window: Window,
    app_state: State<AppState>,
    mutation: DuplicateRevisions,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn move_revision(
    window: Window,
    app_state: State<AppState>,
    mutation: MoveRevision,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn move_source(
    window: Window,
    app_state: State<AppState>,
    mutation: MoveSource,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn move_changes(
    window: Window,
    app_state: State<AppState>,
    mutation: MoveChanges,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn copy_changes(
    window: Window,
    app_state: State<AppState>,
    mutation: CopyChanges,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn move_hunk(
    window: Window,
    app_state: State<AppState>,
    mutation: MoveHunk,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn copy_hunk(
    window: Window,
    app_state: State<AppState>,
    mutation: CopyHunk,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn track_branch(
    window: Window,
    app_state: State<AppState>,
    mutation: TrackBranch,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn untrack_branch(
    window: Window,
    app_state: State<AppState>,
    mutation: UntrackBranch,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn rename_branch(
    window: Window,
    app_state: State<AppState>,
    mutation: RenameBranch,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn create_ref(
    window: Window,
    app_state: State<AppState>,
    mutation: CreateRef,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn delete_ref(
    window: Window,
    app_state: State<AppState>,
    mutation: DeleteRef,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn move_ref(
    window: Window,
    app_state: State<AppState>,
    mutation: MoveRef,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn git_push(
    window: Window,
    app_state: State<AppState>,
    mutation: GitPush,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn git_fetch(
    window: Window,
    app_state: State<AppState>,
    mutation: GitFetch,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, mutation)
}

#[tauri::command(async)]
fn undo_operation(
    window: Window,
    app_state: State<AppState>,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, UndoOperation)
}

fn try_open_repository(window: &Window, cwd: Option<PathBuf>) -> Result<()> {
    log::info!("load workspace {cwd:#?}");

    let app_state = window.state::<AppState>();

    let session_tx: Sender<SessionEvent> = app_state.get_session(window.label());
    let (call_tx, call_rx) = channel();

    session_tx.send(SessionEvent::OpenWorkspace {
        tx: call_tx,
        wd: cwd.clone(),
    })?;

    match call_rx.recv()? {
        Ok(config) => {
            log::debug!("load workspace succeeded");
            match &config {
                messages::RepoConfig::Workspace {
                    absolute_path,
                    track_recent_workspaces,
                    ..
                } => {
                    let repo_path = absolute_path.0.clone();
                    window.set_title((String::from("GG - ") + repo_path.as_str()).as_str())?;

                    // update config and jump lists - this can be slow
                    if *track_recent_workspaces {
                        let window = window.clone();
                        thread::spawn(move || {
                            handler::nonfatal!(add_recent_workspaces(window, &repo_path));
                        });
                    }
                }
                _ => {
                    window.set_title("GG - Gui for JJ")?;
                }
            }
            window.emit("gg://repo/config", config)?;
        }
        Err(err) => {
            log::warn!("load workspace failed: {err}");
            window.set_title("GG - Gui for JJ")?;
            window.emit(
                "gg://repo/config",
                messages::RepoConfig::LoadError {
                    absolute_path: cwd.unwrap_or_default().into(),
                    message: format!("{:#?}", err),
                },
            )?;
        }
    }

    Ok(())
}

fn try_mutate<T: Mutation + Send + Sync + 'static>(
    window: Window,
    app_state: State<AppState>,
    mutation: T,
) -> Result<MutationResult, InvokeError> {
    let session_tx: Sender<SessionEvent> = app_state.get_session(window.label());
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::ExecuteMutation {
            tx: call_tx,
            mutation: Box::new(mutation),
        })
        .map_err(InvokeError::from_error)?;
    call_rx.recv().map_err(InvokeError::from_error)
}

fn handle_window_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::Focused(true) = *event {
        log::debug!("window focused; requesting snapshot");

        let app_state = window.state::<AppState>();

        let session_tx: Sender<SessionEvent> = app_state.get_session(window.label());
        let (call_tx, call_rx) = channel();

        handler::nonfatal!(session_tx.send(SessionEvent::ExecuteSnapshot { tx: call_tx }));

        // events are handled on the main thread, so don't wait for
        // a worker response - that's a recipe for deadlock
        let window = window.clone();
        thread::spawn(move || {
            if let Some(status) = handler::nonfatal!(call_rx.recv()) {
                handler::nonfatal!(window.emit("gg://repo/status", status));
            }
        });
    }
}

fn add_recent_workspaces(window: Window, repo_path: &str) -> Result<()> {
    let app_state = window.state::<AppState>();
    let session_tx: Sender<SessionEvent> = app_state.get_session(window.label());

    let (read_tx, read_rx) = channel();
    session_tx.send(SessionEvent::ReadConfigArray {
        key: vec![
            "gg".to_string(),
            "ui".to_string(),
            "recent-workspaces".to_string(),
        ],
        tx: read_tx,
    })?;
    let mut recent = read_rx.recv()??;
    recent.retain(|x| x != repo_path);
    recent.insert(0, repo_path.to_owned());
    recent.truncate(10);

    #[cfg(windows)]
    {
        windows::update_jump_list(&mut recent)?;
    }

    session_tx.send(SessionEvent::WriteConfigArray {
        key: vec![
            "gg".to_string(),
            "ui".to_string(),
            "recent-workspaces".to_string(),
        ],
        scope: ConfigSource::User,
        values: recent,
    })?;

    Ok(())
}

#[tauri::command(async)]
fn query_recent_workspaces(
    window: Window,
    app_state: State<AppState>,
) -> Result<Vec<String>, InvokeError> {
    let session_tx: Sender<SessionEvent> = app_state.get_session(window.label());
    let (call_tx, call_rx) = channel();
    session_tx
        .send(SessionEvent::ReadConfigArray {
            key: vec![
                "gg".to_string(),
                "ui".to_string(),
                "recent-workspaces".to_string(),
            ],
            tx: call_tx,
        })
        .map_err(InvokeError::from_error)?;

    match call_rx.recv().map_err(InvokeError::from_error)? {
        Ok(mut workspaces) => {
            workspaces.truncate(10);
            Ok(workspaces)
        }
        Err(_) => Ok(vec![]),
    }
}

#[tauri::command]
fn open_workspace_at_path(window: Window, path: String) -> Result<(), InvokeError> {
    match try_open_repository(&window, Some(PathBuf::from(path))) {
        Ok(_) => Ok(()),
        Err(err) => {
            log::error!("try_open_repository: {:#}", err);
            Err(InvokeError::from_anyhow(err))
        }
    }
}
