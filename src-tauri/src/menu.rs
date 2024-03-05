use anyhow::Result;
#[cfg(target_os = "macos")]
use tauri::menu::AboutMetadata;
use tauri::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Manager, Window, Wry,
};
use tauri_plugin_dialog::DialogExt;

use crate::{messages::RevHeader, AppState};

pub fn build_main(app_handle: &AppHandle) -> tauri::Result<Menu<Wry>> {
    #[cfg(target_os = "macos")]
    let pkg_info = app_handle.package_info();
    #[cfg(target_os = "macos")]
    let config = app_handle.config();
    #[cfg(target_os = "macos")]
    let about_metadata = AboutMetadata {
        name: Some("GG".into()),
        version: Some(pkg_info.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };

    let repo_menu = Submenu::with_items(
        app_handle,
        "Repository",
        true,
        &[
            &MenuItem::with_id(
                app_handle,
                "repo_open",
                "Open...",
                true,
                Some("cmdorctrl+o"),
            )?,
            &MenuItem::with_id(app_handle, "repo_reopen", "Reopen", true, Some("f5"))?,
            &PredefinedMenuItem::close_window(app_handle, Some("Close"))?,
        ],
    )?;

    let commit_menu = Submenu::with_id_and_items(
        app_handle,
        "commit",
        "Commit",
        true,
        &[
            &MenuItem::with_id(
                app_handle,
                "commit_new",
                "New child",
                true,
                Some("cmdorctrl+n"),
            )?,
            &MenuItem::with_id(
                app_handle,
                "commit_edit",
                "Edit as working copy",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app_handle,
                "commit_duplicate",
                "Duplicate",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(app_handle, "commit_abandon", "Abandon", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app_handle)?,
            &MenuItem::with_id(
                app_handle,
                "commit_squash",
                "Squash into parent",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app_handle,
                "commit_restore",
                "Restore from parent",
                true,
                None::<&str>,
            )?,
        ],
    )?;

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
        ],
    )?;

    Ok(menu)
}

// XXX make anyhow when possible
pub fn build_context(app_handle: &AppHandle<Wry>) -> Result<Menu<Wry>, tauri::Error> {
    Menu::with_items(
        app_handle,
        &[
            &MenuItem::with_id(app_handle, "revision_new", "New child", true, None::<&str>)?,
            &MenuItem::with_id(
                app_handle,
                "revision_edit",
                "Edit as working copy",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app_handle,
                "revision_duplicate",
                "Duplicate",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app_handle,
                "revision_abandon",
                "Abandon",
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app_handle)?,
            &MenuItem::with_id(
                app_handle,
                "revision_squash",
                "Squash into parent",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app_handle,
                "revision_restore",
                "Restore from parent",
                true,
                None::<&str>,
            )?,
        ],
    )
}

// XXX unwrap(): see https://github.com/tauri-apps/tauri/pull/8777
// enables global menu items based on currently selected revision
pub fn handle_selection(menu: Menu<Wry>, selection: Option<RevHeader>) {
    let commit_submenu = menu.get("commit").unwrap();
    let commit_submenu = commit_submenu.as_submenu_unchecked();

    let set_enabled = |id: &str, value: bool| {
        if let Some(item) = commit_submenu
            .get(id)
            .as_ref()
            .and_then(|item| item.as_menuitem())
        {
            item.set_enabled(value).unwrap();
        }
    };

    match selection {
        None => {
            set_enabled("commit_new", false);
            set_enabled("commit_edit", false);
            set_enabled("commit_duplicate", false);
            set_enabled("commit_abandon", false);
            set_enabled("commit_squash", false);
            set_enabled("commit_restore", false);
        }
        Some(rev) => {
            set_enabled("commit_new", true);
            set_enabled("commit_edit", !rev.is_immutable && !rev.is_working_copy);
            set_enabled("commit_duplicate", true);
            set_enabled("commit_abandon", !rev.is_immutable);
            set_enabled(
                "commit_squash",
                !rev.is_immutable && rev.parent_ids.len() == 1,
            );
            set_enabled(
                "commit_restore",
                !rev.is_immutable && rev.parent_ids.len() == 1,
            );
        }
    }
}

// XXX make anyhow when possible
// enables context menu items for a revision and shows the menu
pub fn handle_context(window: Window, rev: RevHeader) -> Result<(), tauri::Error> {
    let state = window.state::<AppState>();
    let guard = state.0.lock().expect("state mutex poisoned");
    let context_menu = &guard
        .get(window.label())
        .expect("session not found")
        .context_menu;

    let set_enabled = |id: &str, value: bool| {
        if let Some(item) = context_menu
            .get(id)
            .as_ref()
            .and_then(|item| item.as_menuitem())
        {
            item.set_enabled(value).unwrap();
        }
    };

    set_enabled("revision_new", true);
    set_enabled("revision_edit", !rev.is_immutable && !rev.is_working_copy);
    set_enabled("revision_duplicate", true);
    set_enabled("revision_abandon", !rev.is_immutable);
    set_enabled(
        "revision_squash",
        !rev.is_immutable && rev.parent_ids.len() == 1,
    );
    set_enabled(
        "revision_restore",
        !rev.is_immutable && rev.parent_ids.len() == 1,
    );

    window.popup_menu(context_menu)?;

    Ok(())
}

// XXX unwrap(): see https://github.com/tauri-apps/tauri/pull/8777
// events from fixed menus
pub fn handle_event(window: &Window, event: MenuEvent) {
    match event.id.0.as_str() {
        "repo_open" => repo_open(window),
        "repo_reopen" => repo_reopen(window),
        "commit_new" => window.emit("gg://menu/commit", "new").unwrap(),
        "commit_edit" => window.emit("gg://menu/commit", "edit").unwrap(),
        "commit_duplicate" => window.emit("gg://menu/commit", "duplicate").unwrap(),
        "commit_abandon" => window.emit("gg://menu/commit", "abandon").unwrap(),
        "commit_squash" => window.emit("gg://menu/commit", "squash").unwrap(),
        "commit_restore" => window.emit("gg://menu/commit", "restore").unwrap(),
        "revision_new" => window.emit("gg://context/revision", "new").unwrap(),
        "revision_edit" => window.emit("gg://context/revision", "edit").unwrap(),
        "revision_duplicate" => window.emit("gg://context/revision", "duplicate").unwrap(),
        "revision_abandon" => window.emit("gg://context/revision", "abandon").unwrap(),
        "revision_squash" => window.emit("gg://context/revision", "squash").unwrap(),
        "revision_restore" => window.emit("gg://context/revision", "restore").unwrap(),
        _ => (),
    }
}

pub fn repo_open(window: &Window) {
    let window = window.clone();
    window.dialog().file().pick_folder(move |picked| {
        if let Some(cwd) = picked {
            crate::try_open_repository(&window, Some(cwd)).expect("try_open_repository");
        }
    });
}

fn repo_reopen(window: &Window) {
    crate::try_open_repository(window, None).expect("try_open_repository");
}
