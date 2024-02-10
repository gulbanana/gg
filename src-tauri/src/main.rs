#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod format;
mod messages;
mod worker;

use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};

use anyhow::Result;
use tauri::{
    ipc::InvokeError,
    menu::{Menu, MenuItem, Submenu},
    Manager,
};
use tauri_plugin_dialog::DialogExt;

use worker::SessionEvent;

struct SharedSession {
    _worker: JoinHandle<()>,
    channel: Mutex<Sender<SessionEvent>>,
}

fn main() -> Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            forward_accelerator,
            load_log,
            load_change
        ])
        .menu(|handle| {
            Menu::with_items(
                handle,
                &[&Submenu::with_items(
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
                )?],
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
            window.manage(SharedSession {
                _worker: window_worker,
                channel: Mutex::from(sender),
            });
            window.on_menu_event(|window, event| {
                if event.id == "open" {
                    menu_open_repository(window);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap(); // use ? after https://github.com/tauri-apps/tauri/pull/8777

    Ok(())
}

#[tauri::command]
fn forward_accelerator(window: tauri::Window, key: char) {
    if key == 'o' {
        menu_open_repository(&window);
    }
}

#[tauri::command]
fn load_log(window: tauri::Window) -> Result<Vec<messages::RevHeader>, InvokeError> {
    let state = window.state::<SharedSession>();
    let session_tx = state.channel.lock().expect("session lock poisoned");
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
    window: tauri::Window,
    revision: String,
) -> Result<messages::RevDetail, InvokeError> {
    let state = window.state::<SharedSession>();
    let session_tx = state.channel.lock().expect("session lock poisoned");
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

fn menu_open_repository(window: &tauri::Window) {
    let window_handle = window.clone();
    window.dialog().file().pick_folder(move |picked| {
        if let Some(cwd) = picked {
            let state = window_handle.state::<SharedSession>();
            let session_tx = state.channel.lock().expect("session lock poisoned");
            let (call_tx, call_rx) = channel();

            session_tx
                .send(SessionEvent::SetCwd { tx: call_tx, cwd })
                .map_err(InvokeError::from_error)
                .expect("send message to worker thread");
            call_rx.recv().unwrap().unwrap();

            window_handle
                .emit("gg://repo_loaded", ())
                .expect("emit event");
        }
    });
}
