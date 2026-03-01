use std::path::{self, Path, PathBuf};

use anyhow::{Context, Result, anyhow};
#[cfg(target_os = "macos")]
use tauri::menu::AboutMetadata;
use tauri::{
    AppHandle, Emitter, EventTarget, Manager, Window, Wry,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
};
use tauri_plugin_dialog::{DialogExt, FilePath};

use super::{AppState, handler};
use gg_lib::messages::{Operand, RepoEvent, RevHeader, StoreRef};

pub fn build_main(app_handle: &AppHandle, recent_items: &[String]) -> tauri::Result<Menu<Wry>> {
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

    let repo_menu = Submenu::new(app_handle, "Repository", true)?;

    repo_menu.append(&MenuItem::with_id(
        app_handle,
        "menu_repo_init",
        "Init...",
        true,
        Some("cmdorctrl+shift+n"),
    )?)?;
    repo_menu.append(&MenuItem::with_id(
        app_handle,
        "menu_repo_clone",
        "Clone...",
        true,
        Some("cmdorctrl+shift+o"),
    )?)?;

    repo_menu.append(&PredefinedMenuItem::separator(app_handle)?)?;

    repo_menu.append(&MenuItem::with_id(
        app_handle,
        "menu_repo_open",
        "Open...",
        true,
        Some("cmdorctrl+o"),
    )?)?;
    repo_menu.append(&MenuItem::with_id(
        app_handle,
        "menu_repo_reopen",
        "Reopen",
        true,
        Some("f5"),
    )?)?;

    if !recent_items.is_empty() {
        let recent_submenu = Submenu::new(app_handle, "Open Recent", true)?;
        for path in recent_items.iter().take(10) {
            let label = abbreviate_path(path);
            let id = format!("recent:{}", path);
            if let Ok(item) = MenuItem::with_id(app_handle, id, label, true, None::<&str>) {
                recent_submenu.append(&item)?;
            }
        }
        repo_menu.append(&recent_submenu)?;
    }

    repo_menu.append(&PredefinedMenuItem::separator(app_handle)?)?;

    repo_menu.append(&PredefinedMenuItem::close_window(
        app_handle,
        Some("Close"),
    )?)?;

    let revision_menu = Submenu::with_id_and_items(
        app_handle,
        "revision",
        "Revision",
        true,
        &[
            &MenuItem::with_id(
                app_handle,
                "menu_revision_new_child",
                "New child",
                true,
                Some("cmdorctrl+n"),
            )?,
            &MenuItem::with_id(
                app_handle,
                "menu_revision_new_parent",
                "New inserted parent",
                true,
                Some("cmdorctrl+m"),
            )?,
            &PredefinedMenuItem::separator(app_handle)?,
            &MenuItem::with_id(
                app_handle,
                "menu_revision_edit",
                "Edit as working copy",
                true,
                Some("enter"),
            )?,
            &MenuItem::with_id(
                app_handle,
                "menu_revision_revert",
                "Revert into working copy",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app_handle,
                "menu_revision_duplicate",
                "Duplicate",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app_handle,
                "menu_revision_abandon",
                "Abandon",
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app_handle)?,
            &MenuItem::with_id(
                app_handle,
                "menu_revision_squash",
                "Squash into parent",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app_handle,
                "menu_revision_restore",
                "Restore from parent",
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app_handle)?,
            &MenuItem::with_id(
                app_handle,
                "menu_revision_bookmark",
                "Create bookmark...",
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
            &revision_menu,
            &edit_menu,
        ],
    )?;

    Ok(menu)
}

pub fn rebuild_main(app_handle: &AppHandle, recent_items: Vec<String>) -> Result<()> {
    let menu = build_main(app_handle, &recent_items)?;
    app_handle.set_menu(menu)?;
    Ok(())
}

#[allow(clippy::type_complexity)]
pub fn build_context(
    app_handle: &AppHandle<Wry>,
) -> Result<(Menu<Wry>, Menu<Wry>, Menu<Wry>), tauri::Error> {
    let revision_menu = Menu::with_items(
        app_handle,
        &[
            &MenuItem::with_id(
                app_handle,
                "revision_new_child",
                "New child",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app_handle,
                "revision_new_parent",
                "New inserted parent",
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app_handle)?,
            &MenuItem::with_id(
                app_handle,
                "revision_edit",
                "Edit as working copy",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app_handle,
                "revision_revert",
                "Revert into working copy",
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
            &PredefinedMenuItem::separator(app_handle)?,
            &MenuItem::with_id(
                app_handle,
                "revision_bookmark",
                "Create bookmark...",
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
            &MenuItem::with_id(app_handle, "bookmark_track", "Track", true, None::<&str>)?,
            &MenuItem::with_id(
                app_handle,
                "bookmark_untrack",
                "Untrack",
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app_handle)?,
            &MenuItem::with_id(app_handle, "bookmark_push_all", "Push", true, None::<&str>)?,
            &MenuItem::with_id(
                app_handle,
                "bookmark_push_single",
                "Push to remote...",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app_handle,
                "bookmark_fetch_all",
                "Fetch",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app_handle,
                "bookmark_fetch_single",
                "Fetch from remote...",
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app_handle)?,
            &MenuItem::with_id(
                app_handle,
                "bookmark_rename",
                "Rename...",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(app_handle, "bookmark_delete", "Delete", true, None::<&str>)?,
        ],
    )?;

    Ok((revision_menu, tree_menu, ref_menu))
}

/// Computed enablement state for revision menu items
struct RevisionEnablement {
    new_child: bool,
    new_parent: bool,
    edit: bool,
    revert: bool,
    duplicate: bool,
    abandon: bool,
    squash: bool,
    restore: bool,
    bookmark: bool,
}

fn compute_revision_enablement(
    headers: &[RevHeader],
    ignore_immutable: bool,
) -> RevisionEnablement {
    let is_singleton = headers.len() == 1;
    let any_immutable = !ignore_immutable && headers.iter().any(|h| h.is_immutable);
    let oldest = headers.last();
    let oldest_has_single_parent = oldest.map(|h| h.parent_ids.len() == 1).unwrap_or(false);
    let newest = headers.first();

    RevisionEnablement {
        new_child: true,
        new_parent: !any_immutable && oldest_has_single_parent,
        edit: is_singleton && !any_immutable && !newest.map(|h| h.is_working_copy).unwrap_or(false),
        revert: true,
        duplicate: true,
        abandon: !any_immutable,
        squash: !any_immutable && oldest_has_single_parent,
        restore: is_singleton && !any_immutable && oldest_has_single_parent,
        bookmark: is_singleton,
    }
}

// enables global menu items based on currently selected revision(s)
pub fn handle_selection(
    menu: Menu<Wry>,
    headers: Option<&[RevHeader]>,
    ignore_immutable: bool,
) -> Result<()> {
    let revision_submenu = menu
        .get("revision")
        .ok_or(anyhow!("Revision menu not found"))?;
    let revision_submenu = revision_submenu.as_submenu_unchecked();

    match headers {
        None | Some([]) => {
            revision_submenu.enable("menu_revision_new_child", false)?;
            revision_submenu.enable("menu_revision_new_parent", false)?;
            revision_submenu.enable("menu_revision_edit", false)?;
            revision_submenu.enable("menu_revision_revert", false)?;
            revision_submenu.enable("menu_revision_duplicate", false)?;
            revision_submenu.enable("menu_revision_abandon", false)?;
            revision_submenu.enable("menu_revision_squash", false)?;
            revision_submenu.enable("menu_revision_restore", false)?;
            revision_submenu.enable("menu_revision_bookmark", false)?;
        }
        Some(headers) => {
            let state = compute_revision_enablement(headers, ignore_immutable);
            revision_submenu.enable("menu_revision_new_child", state.new_child)?;
            revision_submenu.enable("menu_revision_new_parent", state.new_parent)?;
            revision_submenu.enable("menu_revision_edit", state.edit)?;
            revision_submenu.enable("menu_revision_revert", state.revert)?;
            revision_submenu.enable("menu_revision_duplicate", state.duplicate)?;
            revision_submenu.enable("menu_revision_abandon", state.abandon)?;
            revision_submenu.enable("menu_revision_squash", state.squash)?;
            revision_submenu.enable("menu_revision_restore", state.restore)?;
            revision_submenu.enable("menu_revision_bookmark", state.bookmark)?;
        }
    }

    Ok(())
}

// enables context menu items for a revision and shows the menu
pub fn handle_context(window: Window, ctx: Operand, ignore_immutable: bool) -> Result<()> {
    log::debug!("handling context {ctx:?}");

    let state = window.state::<AppState>();
    let guard = state.windows.lock().expect("state mutex poisoned");

    match ctx {
        Operand::Revision { header } => {
            let context_menu = &guard
                .get(window.label())
                .expect("session not found")
                .revision_menu;

            let state = compute_revision_enablement(&[header], ignore_immutable);
            context_menu.enable("revision_new_child", state.new_child)?;
            context_menu.enable("revision_new_parent", state.new_parent)?;
            context_menu.enable("revision_edit", state.edit)?;
            context_menu.enable("revision_revert", state.revert)?;
            context_menu.enable("revision_duplicate", state.duplicate)?;
            context_menu.enable("revision_abandon", state.abandon)?;
            context_menu.enable("revision_squash", state.squash)?;
            context_menu.enable("revision_restore", state.restore)?;
            context_menu.enable("revision_bookmark", state.bookmark)?;

            window.popup_menu(context_menu)?;
        }
        Operand::Revisions { headers } => {
            let context_menu = &guard
                .get(window.label())
                .expect("session not found")
                .revision_menu;

            let state = compute_revision_enablement(&headers, ignore_immutable);
            context_menu.enable("revision_new_child", state.new_child)?;
            context_menu.enable("revision_new_parent", state.new_parent)?;
            context_menu.enable("revision_edit", state.edit)?;
            context_menu.enable("revision_revert", state.revert)?;
            context_menu.enable("revision_duplicate", state.duplicate)?;
            context_menu.enable("revision_abandon", state.abandon)?;
            context_menu.enable("revision_squash", state.squash)?;
            context_menu.enable("revision_restore", state.restore)?;
            context_menu.enable("revision_bookmark", state.bookmark)?;

            window.popup_menu(context_menu)?;
        }
        Operand::Change { headers, .. } => {
            let context_menu = &guard
                .get(window.label())
                .expect("session not found")
                .tree_menu;

            let any_immutable =
                !ignore_immutable && headers.iter().any(|header| header.is_immutable);
            context_menu.enable(
                "tree_squash",
                !any_immutable
                    && headers
                        .last()
                        .is_some_and(|header| header.parent_ids.len() == 1),
            )?;
            context_menu.enable(
                "tree_restore",
                !any_immutable
                    && headers
                        .last()
                        .is_some_and(|header| header.parent_ids.len() == 1),
            )?;

            window.popup_menu(context_menu)?;
        }
        Operand::Ref { r#ref, .. } => {
            let context_menu = &guard
                .get(window.label())
                .expect("session not found")
                .ref_menu;

            // give remotes a local, or undelete them
            context_menu.enable(
                "bookmark_track",
                matches!(
                    r#ref,
                    StoreRef::RemoteBookmark {
                        is_tracked: false,
                        ..
                    }
                ),
            )?;

            // remove a local's remotes, or a remote from its local
            context_menu.enable(
                "bookmark_untrack",
                matches!(
                    r#ref,
                    StoreRef::LocalBookmark {
                        ref tracking_remotes,
                        ..
                    } if !tracking_remotes.is_empty()
                ) || matches!(
                    r#ref,
                    StoreRef::RemoteBookmark {
                        is_synced: false, // we can *see* the remote ref, and
                        is_tracked: true, // it has a local, and
                        is_absent: false, // that local is somewhere else
                        ..
                    }
                ),
            )?;

            // push a local to its remotes, or finish a CLI delete
            context_menu.enable("bookmark_push_all",
                matches!(r#ref, StoreRef::LocalBookmark { ref tracking_remotes, .. } if !tracking_remotes.is_empty()) ||
                matches!(r#ref, StoreRef::RemoteBookmark { is_tracked: true, is_absent: true, .. }))?;

            // push a local to a selected remote, tracking first if necessary
            context_menu.enable("bookmark_push_single",
                matches!(r#ref, StoreRef::LocalBookmark { potential_remotes, .. } if potential_remotes > 0))?;

            // fetch a local's remotes, or just a remote (unless we're deleting it; that would be silly)
            context_menu.enable("bookmark_fetch_all",
                matches!(r#ref, StoreRef::LocalBookmark { ref tracking_remotes, .. } if !tracking_remotes.is_empty()) ||
                matches!(r#ref, StoreRef::RemoteBookmark { is_tracked, is_absent, .. } if (!is_tracked || !is_absent)))?;

            // fetch a local, tracking first if necessary
            context_menu.enable("bookmark_fetch_single",
                matches!(r#ref, StoreRef::LocalBookmark { available_remotes, .. } if available_remotes > 0))?;

            // rename a local, which also untracks remotes
            context_menu.enable(
                "bookmark_rename",
                matches!(r#ref, StoreRef::LocalBookmark { .. }),
            )?;

            // remove a local, or make a remote absent
            context_menu.enable(
                "bookmark_delete",
                !matches!(
                    r#ref,
                    StoreRef::RemoteBookmark {
                        is_absent: true,
                        is_tracked: true,
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

    // we use a shared menu due to cleanup issues
    if !window.is_focused()? {
        return Ok(());
    }

    let target = EventTarget::window(window.label());
    match event.id.0.as_str() {
        "menu_repo_init" => repo_init(window),
        "menu_repo_clone" => repo_clone(window),
        "menu_repo_open" => repo_open(window),
        "menu_repo_reopen" => repo_reopen(window),
        "menu_revision_new_child" => window.emit_to(target, "gg://menu/revision", "new_child")?,
        "menu_revision_new_parent" => window.emit_to(target, "gg://menu/revision", "new_parent")?,
        "menu_revision_edit" => window.emit_to(target, "gg://menu/revision", "edit")?,
        "menu_revision_revert" => window.emit_to(target, "gg://menu/revision", "revert")?,
        "menu_revision_duplicate" => window.emit_to(target, "gg://menu/revision", "duplicate")?,
        "menu_revision_abandon" => window.emit_to(target, "gg://menu/revision", "abandon")?,
        "menu_revision_squash" => window.emit_to(target, "gg://menu/revision", "squash")?,
        "menu_revision_restore" => window.emit_to(target, "gg://menu/revision", "restore")?,
        "menu_revision_bookmark" => window.emit_to(target, "gg://menu/revision", "bookmark")?,
        "revision_new_child" => window.emit_to(target, "gg://context/revision", "new_child")?,
        "revision_new_parent" => window.emit_to(target, "gg://context/revision", "new_parent")?,
        "revision_edit" => window.emit_to(target, "gg://context/revision", "edit")?,
        "revision_revert" => window.emit_to(target, "gg://context/revision", "revert")?,
        "revision_duplicate" => window.emit_to(target, "gg://context/revision", "duplicate")?,
        "revision_abandon" => window.emit_to(target, "gg://context/revision", "abandon")?,
        "revision_squash" => window.emit_to(target, "gg://context/revision", "squash")?,
        "revision_restore" => window.emit_to(target, "gg://context/revision", "restore")?,
        "revision_bookmark" => window.emit_to(target, "gg://context/revision", "bookmark")?,
        "tree_squash" => window.emit_to(target, "gg://context/tree", "squash")?,
        "tree_restore" => window.emit_to(target, "gg://context/tree", "restore")?,
        "bookmark_track" => window.emit_to(target, "gg://context/bookmark", "track")?,
        "bookmark_untrack" => window.emit_to(target, "gg://context/bookmark", "untrack")?,
        "bookmark_push_all" => window.emit_to(target, "gg://context/bookmark", "push-all")?,
        "bookmark_push_single" => window.emit_to(target, "gg://context/bookmark", "push-single")?,
        "bookmark_fetch_all" => window.emit_to(target, "gg://context/bookmark", "fetch-all")?,
        "bookmark_fetch_single" => {
            window.emit_to(target, "gg://context/bookmark", "fetch-single")?
        }
        "bookmark_rename" => window.emit_to(target, "gg://context/bookmark", "rename")?,
        "bookmark_delete" => window.emit_to(target, "gg://context/bookmark", "delete")?,
        recent_id if recent_id.starts_with("recent:") => {
            let path = PathBuf::from(&recent_id["recent:".len()..]);
            let app_handle = window.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = super::try_create_window(&app_handle, Some(path)) {
                    log::error!("Failed to open recent workspace: {e:#}");
                }
            });
        }
        _ => (),
    };

    Ok(())
}

pub fn repo_open(window: &Window) {
    let window = window.clone();
    window.dialog().file().pick_folder(move |picked| {
        if let Some(FilePath::Path(wd)) = picked {
            handler::nonfatal!(super::reopen_repository(&window, wd));
        }
    });
}

fn repo_reopen(window: &Window) {
    handler::fatal!(super::try_open_repository(window, None).context("try_open_repository"));
}

pub fn repo_init(window: &Window) {
    let window = window.clone();
    window.dialog().file().pick_folder(move |picked| {
        if let Some(FilePath::Path(cwd)) = picked {
            let git_path = cwd.join(".git");

            let payload = RepoEvent::InitConfirm {
                path: cwd.to_string_lossy().to_string(),
                has_git: git_path.exists(),
            };

            handler::nonfatal!(window.emit_to(
                EventTarget::window(window.label()),
                "gg://menu/repo",
                payload
            ));
        }
    });
}

pub fn repo_clone(window: &Window) {
    let payload = RepoEvent::CloneURL;

    handler::nonfatal!(window.emit_to(
        EventTarget::window(window.label()),
        "gg://menu/repo",
        payload
    ));
}

pub fn repo_clone_with_url(window: &Window, url: String) {
    let window = window.clone();
    window.dialog().file().pick_folder(move |picked| {
        if let Some(FilePath::Path(parent_dir)) = picked {
            let repo_name = extract_repo_name(&url);
            let dest_path = parent_dir.join(&repo_name);
            let path = dest_path.to_string_lossy().to_string();

            let payload = RepoEvent::CloneConfirm { url, path };

            handler::nonfatal!(window.emit_to(
                EventTarget::window(window.label()),
                "gg://menu/repo",
                payload
            ));
        }
    });
}

// https://github.com/user/repo.git -> repo
// git@github.com:user/repo.git -> repo
// /path/to/repo -> repo
// /path/to/repo.git -> repo
fn extract_repo_name(url: &str) -> String {
    let url = url.trim_end_matches('/');
    let name = url
        .rsplit('/')
        .next()
        .or_else(|| url.rsplit(':').next())
        .unwrap_or("repo");

    name.strip_suffix(".git").unwrap_or(name).to_string()
}

fn abbreviate_path(path: &str) -> String {
    let path = Path::new(path);

    if let Ok(home) = etcetera::home_dir()
        && let Ok(relative) = path.strip_prefix(&home)
    {
        let sep = path::MAIN_SEPARATOR;
        return format!("~{sep}{}", relative.display());
    }

    path.display().to_string()
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
