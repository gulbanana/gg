#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod format;
mod gui_util;
mod messages;
mod worker;

use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};

use anyhow::Result;
use tauri::{
    ipc::InvokeError,
    menu::{Menu, MenuItem, Submenu},
    Manager,
};
use tauri::{State, Window};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_window_state::StateFlags;

use worker::SessionEvent;

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
            load_log,
            load_change
        ])
        .menu(|handle| {
            Menu::with_items(
                handle,
                &[
                    &Submenu::with_items(
                        handle,
                        "Repository",
                        true,
                        &[&MenuItem::with_id(
                            handle,
                            "open",
                            "Open...",
                            true,
                            Some("cmdorctrl+o"),
                        )?],
                    )?,
                    &Submenu::with_items(handle, "Commit", true, &[])?,
                    &Submenu::with_items(handle, "Operation", true, &[])?,
                ],
            )
        })
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            let (sender, receiver) = channel();
            let window_worker = thread::spawn(move || {
                if let Err(err) = worker::main(receiver) {
                    panic!("{:?}", err);
                }
            });
            window.on_menu_event(|window, event| {
                if event.id == "open" {
                    menu_open_repository(window.clone());
                }
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
        .unwrap(); // use ? after https://github.com/tauri-apps/tauri/pull/8777

    Ok(())
}

#[tauri::command(async)]
fn notify_window_ready(window: Window) {
    window.show().unwrap();
}

#[tauri::command]
fn forward_accelerator(window: Window, key: char) {
    if key == 'o' {
        menu_open_repository(window);
    }
}

#[tauri::command]
fn load_log(
    window: Window,
    app_state: State<AppState>,
) -> Result<Vec<messages::RevHeader>, InvokeError> {
    let session_tx: Sender<SessionEvent> = app_state.get_sender(&window);
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::GetLog { tx: call_tx })
        .map_err(InvokeError::from_error)?;
    call_rx
        .recv()
        .map_err(InvokeError::from_error)?
        .map_err(InvokeError::from_anyhow)
}

#[tauri::command]
fn load_change(
    window: Window,
    app_state: State<AppState>,
    revision: String,
) -> Result<messages::RevDetail, InvokeError> {
    let session_tx: Sender<SessionEvent> = app_state.get_sender(&window);
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::GetChange {
            tx: call_tx,
            revision,
        })
        .map_err(InvokeError::from_error)?;
    call_rx
        .recv()
        .map_err(InvokeError::from_error)?
        .map_err(InvokeError::from_anyhow)
}

fn menu_open_repository(window: Window) {
    window.dialog().file().pick_folder(move |picked| {
        if let Some(cwd) = picked {
            let app_state = window.state::<AppState>();
            let session_tx: Sender<SessionEvent> = app_state.get_sender(&window);
            let (call_tx, call_rx) = channel();

            session_tx
                .send(SessionEvent::SetCwd { tx: call_tx, cwd })
                .map_err(InvokeError::from_error)
                .expect("send message to worker thread");
            call_rx.recv().unwrap().unwrap();

            window.emit("gg://repo_loaded", ()).expect("emit event");
        }
    });
}
