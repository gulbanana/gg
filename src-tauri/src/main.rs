#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gui_util;
mod messages;
#[cfg(all(test, not(feature = "ts-rs")))]
mod tests;
mod worker;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};
use gui_util::WorkerSession;
use messages::{DescribeRevision, MutationResult};
use tauri::menu::{AboutMetadata, PredefinedMenuItem, HELP_SUBMENU_ID};
use tauri::{
    ipc::InvokeError,
    menu::{Menu, MenuItem, Submenu},
    Manager,
};
use tauri::{AppHandle, State, Window, Wry};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_window_state::StateFlags;

use worker::{Session, SessionEvent};

#[derive(Default)]
struct AppState(Mutex<HashMap<String, WindowState>>);

struct WindowState {
    _worker: JoinHandle<()>,
    channel: Sender<SessionEvent>,
}

impl AppState {
    fn get_sender(&self, window: &Window) -> Sender<SessionEvent> {
        self.0
            .lock()
            .expect("state mutex poisoned")
            .get(window.label())
            .expect("session not found")
            .channel
            .clone()
    }
}

fn main() -> Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
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
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            notify_window_ready,
            forward_accelerator,
            query_log,
            query_log_next_page,
            query_revision,
            describe_revision
        ])
        .menu(build_menu)
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
            window.on_menu_event(|window, event| match event.id.0.as_str() {
                "open" => menu_open_repository(window.clone()),
                "reopen" => menu_reopen_repository(window.clone()),
                _ => (),
            });

            let app_state = app.state::<AppState>();
            app_state.0.lock().unwrap().insert(
                window.label().to_owned(),
                WindowState {
                    _worker: window_worker,
                    channel: sender,
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
    try_open_repository(window.clone(), None).unwrap();
    window.show().unwrap();
}

#[tauri::command]
fn forward_accelerator(window: Window, key: char) {
    if key == 'o' {
        menu_open_repository(window);
    }
}

#[tauri::command(async)]
fn query_log(
    window: Window,
    app_state: State<AppState>,
    revset: String,
) -> Result<messages::LogPage, InvokeError> {
    let session_tx: Sender<SessionEvent> = app_state.get_sender(&window);
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
    let session_tx: Sender<SessionEvent> = app_state.get_sender(&window);
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
    rev: String,
) -> Result<messages::RevDetail, InvokeError> {
    let session_tx: Sender<SessionEvent> = app_state.get_sender(&window);
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::QueryRevision {
            tx: call_tx,
            change_id: rev,
        })
        .map_err(InvokeError::from_error)?;
    call_rx
        .recv()
        .map_err(InvokeError::from_error)?
        .map_err(InvokeError::from_anyhow)
}

#[tauri::command(async)]
fn describe_revision(
    window: Window,
    app_state: State<AppState>,
    mutation: DescribeRevision,
) -> Result<MutationResult, InvokeError> {
    let session_tx: Sender<SessionEvent> = app_state.get_sender(&window);
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::DescribeRevision {
            tx: call_tx,
            mutation,
        })
        .map_err(InvokeError::from_error)?;
    call_rx.recv().map_err(InvokeError::from_error)
}

fn try_open_repository(window: Window, cwd: Option<PathBuf>) -> Result<()> {
    let app_state = window.state::<AppState>();

    let session_tx: Sender<SessionEvent> = app_state.get_sender(&window);
    let (call_tx, call_rx) = channel();

    session_tx.send(SessionEvent::OpenWorkspace { tx: call_tx, cwd })?;
    let config = call_rx.recv()??;

    window.emit("gg://repo/config", config).unwrap(); // XXX https://github.com/tauri-apps/tauri/pull/8777

    Ok(())
}

fn build_menu(app_handle: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let pkg_info = app_handle.package_info();
    let config = app_handle.config();
    let about_metadata = AboutMetadata {
        name: Some("GG".into()),
        version: Some(pkg_info.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };

    let help_menu = Submenu::with_id_and_items(
        app_handle,
        HELP_SUBMENU_ID,
        "Help",
        true,
        &[
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(app_handle, None, Some(about_metadata))?,
        ],
    )?;

    let repo_menu = Submenu::with_items(
        app_handle,
        "Repository",
        true,
        &[
            &MenuItem::with_id(app_handle, "open", "Open...", true, Some("cmdorctrl+o"))?,
            &MenuItem::with_id(app_handle, "reopen", "Reopen", true, Some("f5"))?,
            &PredefinedMenuItem::close_window(app_handle, Some("Close"))?,
        ],
    )?;

    let commit_menu = Submenu::with_items(app_handle, "Commit", true, &[])?;

    let edit_menu = Submenu::with_items(
        app_handle,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app_handle, None)?,
            &PredefinedMenuItem::redo(app_handle, None)?,
            &PredefinedMenuItem::separator(app_handle)?,
            &PredefinedMenuItem::cut(app_handle, None)?,
            &PredefinedMenuItem::copy(app_handle, None)?,
            &PredefinedMenuItem::paste(app_handle, None)?,
            &PredefinedMenuItem::select_all(app_handle, None)?,
        ],
    )?;

    let menu = Menu::with_items(
        app_handle,
        &[
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app_handle,
                pkg_info.name.clone(),
                true,
                &[
                    &PredefinedMenuItem::about(app_handle, None, Some(about_metadata))?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::services(app_handle, None)?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::hide(app_handle, None)?,
                    &PredefinedMenuItem::hide_others(app_handle, None)?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::quit(app_handle, None)?,
                ],
            )?,
            &repo_menu,
            &commit_menu,
            &edit_menu,
            &help_menu,
        ],
    )?;

    Ok(menu)
}

fn menu_open_repository(window: Window) {
    window.dialog().file().pick_folder(move |picked| {
        if let Some(cwd) = picked {
            try_open_repository(window, Some(cwd)).expect("open repository");
        }
    });
}

fn menu_reopen_repository(window: Window) {
    try_open_repository(window, None).unwrap();
}
