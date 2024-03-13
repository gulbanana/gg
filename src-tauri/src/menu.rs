use anyhow::{anyhow, Context, Result};
#[cfg(target_os = "macos")]
use tauri::menu::AboutMetadata;
use tauri::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Manager, Window, Wry,
};
use tauri_plugin_dialog::DialogExt;

use crate::{
    handler,
    messages::{Operand, RefName, RevHeader},
    AppState,
};

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

pub fn build_context(
    app_handle: &AppHandle<Wry>,
) -> Result<(Menu<Wry>, Menu<Wry>, Menu<Wry>), tauri::Error> {
    let revision_menu = Menu::with_items(
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
    )?;

    let tree_menu = Menu::with_items(
        app_handle,
        &[
            &MenuItem::with_id(
                app_handle,
                "tree_squash",
                "Squash into parent",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app_handle,
                "tree_restore",
                "Restore from parent",
                true,
                None::<&str>,
            )?,
        ],
    )?;

    let ref_menu = Menu::with_items(
        app_handle,
        &[
            &MenuItem::with_id(app_handle, "branch_track", "Track", true, None::<&str>)?,
            &MenuItem::with_id(app_handle, "branch_untrack", "Untrack", true, None::<&str>)?,
        ],
    )?;

    Ok((revision_menu, tree_menu, ref_menu))
}

// enables global menu items based on currently selected revision
pub fn handle_selection(menu: Menu<Wry>, selection: Option<RevHeader>) -> Result<()> {
    let commit_submenu = menu.get("commit").ok_or(anyhow!("Commit menu not found"))?;
    let commit_submenu = commit_submenu.as_submenu_unchecked();

    match selection {
        None => {
            commit_submenu.enable("commit_new", false)?;
            commit_submenu.enable("commit_edit", false)?;
            commit_submenu.enable("commit_duplicate", false)?;
            commit_submenu.enable("commit_abandon", false)?;
            commit_submenu.enable("commit_squash", false)?;
            commit_submenu.enable("commit_restore", false)?;
        }
        Some(rev) => {
            commit_submenu.enable("commit_new", true)?;
            commit_submenu.enable("commit_edit", !rev.is_immutable && !rev.is_working_copy)?;
            commit_submenu.enable("commit_duplicate", true)?;
            commit_submenu.enable("commit_abandon", !rev.is_immutable)?;
            commit_submenu.enable(
                "commit_squash",
                !rev.is_immutable && rev.parent_ids.len() == 1,
            )?;
            commit_submenu.enable(
                "commit_restore",
                !rev.is_immutable && rev.parent_ids.len() == 1,
            )?;
        }
    };

    Ok(())
}

// enables context menu items for a revision and shows the menu
pub fn handle_context(window: Window, ctx: Operand) -> Result<()> {
    log::debug!("handling context {ctx:?}");

    let state = window.state::<AppState>();
    let guard = state.0.lock().expect("state mutex poisoned");

    match ctx {
        Operand::Revision { header } => {
            let context_menu = &guard
                .get(window.label())
                .expect("session not found")
                .revision_menu;

            context_menu.enable("revision_new", true)?;
            context_menu.enable(
                "revision_edit",
                !header.is_immutable && !header.is_working_copy,
            )?;
            context_menu.enable("revision_duplicate", true)?;
            context_menu.enable("revision_abandon", !header.is_immutable)?;
            context_menu.enable(
                "revision_squash",
                !header.is_immutable && header.parent_ids.len() == 1,
            )?;
            context_menu.enable(
                "revision_restore",
                !header.is_immutable && header.parent_ids.len() == 1,
            )?;

            window.popup_menu(context_menu)?;
        }
        Operand::Change { header, .. } => {
            let context_menu = &guard
                .get(window.label())
                .expect("session not found")
                .tree_menu;

            context_menu.enable(
                "tree_squash",
                !header.is_immutable && header.parent_ids.len() == 1,
            )?;
            context_menu.enable(
                "tree_restore",
                !header.is_immutable && header.parent_ids.len() == 1,
            )?;

            window.popup_menu(context_menu)?;
        }
        Operand::Branch { name, .. } => {
            let context_menu = &guard
                .get(window.label())
                .expect("session not found")
                .ref_menu;

            context_menu.enable(
                "branch_track",
                matches!(
                    name,
                    RefName::RemoteBranch {
                        is_tracked: false,
                        ..
                    }
                ),
            )?;
            context_menu.enable(
                "branch_untrack",
                matches!(
                    name,
                    RefName::RemoteBranch {
                        is_tracked: true,
                        ..
                    } | RefName::LocalBranch {
                        is_tracking: true,
                        ..
                    }
                ),
            )?;

            window.popup_menu(context_menu)?;
        }
        _ => (), // no popup required
    };

    Ok(())
}

pub fn handle_event(window: &Window, event: MenuEvent) -> Result<()> {
    log::debug!("handling event {event:?}");

    match event.id.0.as_str() {
        "repo_open" => repo_open(window),
        "repo_reopen" => repo_reopen(window),
        "commit_new" => window.emit("gg://menu/commit", "new")?,
        "commit_edit" => window.emit("gg://menu/commit", "edit")?,
        "commit_duplicate" => window.emit("gg://menu/commit", "duplicate")?,
        "commit_abandon" => window.emit("gg://menu/commit", "abandon")?,
        "commit_squash" => window.emit("gg://menu/commit", "squash")?,
        "commit_restore" => window.emit("gg://menu/commit", "restore")?,
        "revision_new" => window.emit("gg://context/revision", "new")?,
        "revision_edit" => window.emit("gg://context/revision", "edit")?,
        "revision_duplicate" => window.emit("gg://context/revision", "duplicate")?,
        "revision_abandon" => window.emit("gg://context/revision", "abandon")?,
        "revision_squash" => window.emit("gg://context/revision", "squash")?,
        "revision_restore" => window.emit("gg://context/revision", "restore")?,
        "tree_squash" => window.emit("gg://context/tree", "squash")?,
        "tree_restore" => window.emit("gg://context/tree", "restore")?,
        "branch_track" => window.emit("gg://context/branch", "track")?,
        "branch_untrack" => window.emit("gg://context/branch", "untrack")?,
        _ => (),
    };

    Ok(())
}

pub fn repo_open(window: &Window) {
    let window = window.clone();
    window.dialog().file().pick_folder(move |picked| {
        if let Some(cwd) = picked {
            handler::fatal!(
                crate::try_open_repository(&window, Some(cwd)).context("try_open_repository")
            );
        }
    });
}

fn repo_reopen(window: &Window) {
    handler::fatal!(crate::try_open_repository(window, None).context("try_open_repository"));
}

trait Enabler {
    fn enable(&self, id: &str, value: bool) -> tauri::Result<()>;
}

impl Enabler for Menu<Wry> {
    fn enable(&self, id: &str, value: bool) -> tauri::Result<()> {
        if let Some(item) = self.get(id).as_ref().and_then(|item| item.as_menuitem()) {
            item.set_enabled(value)
        } else {
            Ok(())
        }
    }
}

impl Enabler for Submenu<Wry> {
    fn enable(&self, id: &str, value: bool) -> tauri::Result<()> {
        if let Some(item) = self.get(id).as_ref().and_then(|item| item.as_menuitem()) {
            item.set_enabled(value)
        } else {
            Ok(())
        }
    }
}
