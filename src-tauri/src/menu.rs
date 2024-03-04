use tauri::{
    menu::{
        AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID,
    },
    AppHandle, Manager, Window, Wry,
};
use tauri_plugin_dialog::DialogExt;

pub fn build(app_handle: &AppHandle) -> tauri::Result<Menu<Wry>> {
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

    let commit_menu = Submenu::with_items(
        app_handle,
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
                "commit_new",
                "Edit as working copy",
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
            &help_menu,
        ],
    )?;

    Ok(menu)
}

pub fn handle_event(window: &Window, event: MenuEvent) {
    match event.id.0.as_str() {
        "repo_open" => repo_open(window),
        "repo_reopen" => repo_reopen(window),
        "commit_new" => window.emit("gg://menu/commit", "new").unwrap(),
        "commit_edit" => window.emit("gg://menu/commit", "edit").unwrap(),
        _ => (),
    }
}

pub fn repo_open(window: &Window) {
    let window = window.clone();
    window.dialog().file().pick_folder(move |picked| {
        if let Some(cwd) = picked {
            crate::try_open_repository(&window, Some(cwd)).expect("open try_open_repository");
        }
    });
}

fn repo_reopen(window: &Window) {
    crate::try_open_repository(window, None).expect("open try_open_repository");
}
