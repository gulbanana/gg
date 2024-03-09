#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gui_util;
mod menu;
mod messages;
mod settings;
#[cfg(all(test, not(feature = "ts-rs")))]
mod tests;
mod worker;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};
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
            let window = app.get_webview_window("main").unwrap();
            let (sender, receiver) = channel();

            let handle = window.clone();
            let window_worker = thread::spawn(move || {
                while let Err(err) = WorkerSession::default()
                    .handle_events(&receiver)
                    .context("worker")
                {
                    handle
                        .emit(
                            "gg://repo/config",
                            messages::RepoConfig::DeadWorker {
                                error: format!("{err:#}"),
                            },
                        )
                        .unwrap();
                }
            });

            window.on_menu_event(menu::handle_event);

            let handle = window.clone();
            window.on_window_event(move |event| handle_window_event(&handle, event));

            let handle = window.clone();
            window.listen("gg://revision/select", move |event| {
                let payload: Result<Option<messages::RevHeader>, serde_json::Error> =
                    serde_json::from_str(event.payload());
                if let Some(menu) = handle.menu() {
                    if let Ok(selection) = payload {
                        menu::handle_selection(menu, selection);
                    }
                }
            });

            let (revision_menu, tree_menu, ref_menu) = menu::build_context(app.handle()).unwrap();

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
        .run(tauri::generate_context!())
        .unwrap(); // XXX https://github.com/tauri-apps/tauri/pull/8777

    Ok(())
}

#[tauri::command(async)]
fn notify_window_ready(window: Window) {
    try_open_repository(&window, None).unwrap();
    window.show().unwrap();
}

#[tauri::command]
fn forward_accelerator(window: Window, key: char) {
    if key == 'o' {
        menu::repo_open(&window);
    }
}

#[tauri::command]
fn forward_context_menu(window: Window, context: messages::MenuContext) -> Result<(), InvokeError> {
    menu::handle_context(window, context).map_err(InvokeError::from_error)?;
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
    let app_state = window.state::<AppState>();

    let session_tx: Sender<SessionEvent> = app_state.get_sender(window.label());
    let (call_tx, call_rx) = channel();

    session_tx.send(SessionEvent::OpenWorkspace { tx: call_tx, cwd })?;
    let config = call_rx.recv()??;

    window.emit("gg://repo/config", config).unwrap(); // XXX https://github.com/tauri-apps/tauri/pull/8777

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
            let app_state = window.state::<AppState>();

            let session_tx: Sender<SessionEvent> = app_state.get_sender(window.label());
            let (call_tx, call_rx) = channel();

            session_tx
                .send(SessionEvent::ExecuteSnapshot { tx: call_tx })
                .unwrap();

            if let Some(status) = call_rx.recv().unwrap() {
                window.emit("gg://repo/status", status).unwrap();
            }
        }
        _ => (),
    }
}
