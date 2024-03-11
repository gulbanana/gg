#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod gui_util;
mod handler;
mod menu;
mod messages;
#[cfg(all(test, not(feature = "ts-rs")))]
mod tests;
mod worker;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};

use anyhow::{anyhow, Context, Result};
use log::LevelFilter;
use tauri::menu::Menu;
use tauri::{ipc::InvokeError, Manager};
use tauri::{State, WebviewWindow, Window, WindowEvent, Wry};
use tauri_plugin_window_state::StateFlags;

use gui_util::WorkerSession;
use messages::{
    AbandonRevisions, CheckoutRevision, CopyChanges, CreateRevision, DescribeRevision,
    DuplicateRevisions, MoveChanges, MutationResult, TrackBranch, UndoOperation, UntrackBranch,
};
use worker::{Mutation, Session, SessionEvent};

#[derive(Default)]
struct AppState(Mutex<HashMap<String, WindowState>>);

struct WindowState {
    _worker: JoinHandle<()>,
    channel: Sender<SessionEvent>,
    revision_menu: Menu<Wry>,
    tree_menu: Menu<Wry>,
    ref_menu: Menu<Wry>,
}

impl AppState {
    fn get_sender(&self, window_label: &str) -> Sender<SessionEvent> {
        self.0
            .lock()
            .expect("state mutex poisoned")
            .get(window_label)
            .expect("session not found")
            .channel
            .clone()
    }
}

fn main() -> Result<()> {
    let debug = std::env::args()
        .find(|arg| arg.as_str() == "--debug")
        .is_some();

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
                    if debug {
                        LevelFilter::Debug
                    } else {
                        LevelFilter::Warn
                    },
                )
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            notify_window_ready,
            forward_accelerator,
            forward_context_menu,
            query_log,
            query_log_next_page,
            query_revision,
            checkout_revision,
            create_revision,
            describe_revision,
            duplicate_revisions,
            abandon_revisions,
            move_changes,
            copy_changes,
            track_branch,
            untrack_branch,
            undo_operation
        ])
        .menu(menu::build_main)
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .ok_or(anyhow!("preconfigured window not found"))?;
            let (sender, receiver) = channel();

            let handle = window.clone();
            let window_worker = thread::spawn(move || {
                log::info!("start worker");

                while let Err(err) = WorkerSession::default()
                    .handle_events(&receiver)
                    .context("worker")
                {
                    log::info!("restart worker: {err:#}");

                    // it's ok if the worker has to restart, as long as we can notify the frontend of it
                    handler::fatal!(handle.emit(
                        "gg://repo/config",
                        messages::RepoConfig::DeadWorker {
                            error: format!("{err:#}"),
                        },
                    ));
                }
            });

            window.on_menu_event(|w, e| handler::fatal!(menu::handle_event(w, e)));

            let handle = window.clone();
            window.on_window_event(move |event| handle_window_event(&handle, event));

            let handle = window.clone();
            window.listen("gg://revision/select", move |event| {
                let payload: Result<Option<messages::RevHeader>, serde_json::Error> =
                    serde_json::from_str(event.payload());
                if let Some(menu) = handle.menu() {
                    if let Ok(selection) = payload {
                        handler::fatal!(menu::handle_selection(menu, selection));
                    }
                }
            });

            let (revision_menu, tree_menu, ref_menu) = menu::build_context(app.handle())?;

            let app_state = app.state::<AppState>();
            app_state.0.lock().unwrap().insert(
                window.label().to_owned(),
                WindowState {
                    _worker: window_worker,
                    channel: sender,
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

#[tauri::command(async)]
fn notify_window_ready(window: Window) {
    log::debug!("window opened; loading cwd");
    handler::fatal!(window.show());
    handler::nonfatal!(try_open_repository(&window, None));
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
    let session_tx: Sender<SessionEvent> = app_state.get_sender(window.label());
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
    let session_tx: Sender<SessionEvent> = app_state.get_sender(window.label());
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
    query: String,
) -> Result<messages::RevResult, InvokeError> {
    let session_tx: Sender<SessionEvent> = app_state.get_sender(window.label());
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::QueryRevision { tx: call_tx, query })
        .map_err(InvokeError::from_error)?;
    call_rx
        .recv()
        .map_err(InvokeError::from_error)?
        .map_err(InvokeError::from_anyhow)
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
fn abandon_revisions(
    window: Window,
    app_state: State<AppState>,
    mutation: AbandonRevisions,
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
fn undo_operation(
    window: Window,
    app_state: State<AppState>,
) -> Result<MutationResult, InvokeError> {
    try_mutate(window, app_state, UndoOperation)
}

fn try_open_repository(window: &Window, cwd: Option<PathBuf>) -> Result<()> {
    log::info!("load workspace {cwd:#?}");

    let app_state = window.state::<AppState>();

    let session_tx: Sender<SessionEvent> = app_state.get_sender(window.label());
    let (call_tx, call_rx) = channel();

    session_tx.send(SessionEvent::OpenWorkspace {
        tx: call_tx,
        wd: cwd.clone(),
    })?;

    match call_rx.recv()? {
        Ok(config) => {
            log::debug!("load workspace succeeded");
            window.emit("gg://repo/config", config)?;
        }
        Err(err) => {
            log::warn!("load workspace failed: {err}");
            window.emit(
                "gg://repo/config",
                messages::RepoConfig::NoWorkspace {
                    absolute_path: cwd.unwrap_or(PathBuf::new()).into(),
                    error: format!("{:#?}", err),
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
    let session_tx: Sender<SessionEvent> = app_state.get_sender(window.label());
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::ExecuteMutation {
            tx: call_tx,
            mutation: Box::new(mutation),
        })
        .map_err(InvokeError::from_error)?;
    call_rx.recv().map_err(InvokeError::from_error)
}

fn handle_window_event(window: &WebviewWindow, event: &WindowEvent) {
    match *event {
        WindowEvent::Focused(true) => {
            log::debug!("window focused; requesting snapshot");

            let app_state = window.state::<AppState>();

            let session_tx: Sender<SessionEvent> = app_state.get_sender(window.label());
            let (call_tx, call_rx) = channel();

            handler::nonfatal!(session_tx.send(SessionEvent::ExecuteSnapshot { tx: call_tx }));

            if let Some(status) = handler::nonfatal!(call_rx.recv()) {
                handler::nonfatal!(window.emit("gg://repo/status", status));
            }
        }
        _ => (),
    }
}
