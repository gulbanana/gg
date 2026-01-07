mod handler;
mod menu;
#[cfg(target_os = "macos")]
mod recent_items;
mod sink;

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};
use jj_lib::config::ConfigSource;
use jj_lib::settings::UserSettings;
use log::LevelFilter;
use tauri::async_runtime;
use tauri::ipc::InvokeError;
use tauri::menu::Menu;
use tauri::webview::WebviewWindowBuilder;
use tauri::{AppHandle, Emitter, EventTarget, Listener, Manager, State, Window, WindowEvent, Wry};
use tauri_plugin_window_state::StateFlags;

use crate::messages::{
    self, AbandonRevisions, BackoutRevisions, CheckoutRevision, CloneRepository, CopyChanges,
    CopyHunk, CreateRef, CreateRevision, CreateRevisionBetween, DeleteRef, DescribeRevision,
    DuplicateRevisions, GitFetch, GitPush, InitRepository, InsertRevision, MoveChanges, MoveHunk,
    MoveRef, MoveRevision, MoveSource, MutationResult, RenameBranch, TrackBranch, UndoOperation,
    UntrackBranch,
};
use crate::worker::{Mutation, Session, SessionEvent, WorkerSession};
use sink::TauriSink;

struct AppState {
    windows: Arc<Mutex<HashMap<String, WindowState>>>,
    settings: UserSettings,
}

impl AppState {
    fn new(settings: UserSettings) -> Self {
        Self {
            windows: Arc::new(Mutex::new(HashMap::new())),
            settings,
        }
    }
}

struct WindowState {
    _worker: JoinHandle<()>,
    worker_channel: Sender<SessionEvent>,
    revision_menu: Menu<Wry>,
    tree_menu: Menu<Wry>,
    ref_menu: Menu<Wry>,
    selection: Option<messages::RevHeader>,
    has_workspace: bool,
}

impl AppState {
    fn get_session(&self, window_label: &str) -> Sender<SessionEvent> {
        self.windows
            .lock()
            .expect("state mutex poisoned")
            .get(window_label)
            .expect("session not found")
            .worker_channel
            .clone()
    }

    fn get_selection(&self, window_label: &str) -> Option<messages::RevHeader> {
        self.windows
            .lock()
            .expect("state mutex poisoned")
            .get(window_label)
            .and_then(|state| state.selection.clone())
    }

    fn get_has_workspace(&self, window_label: &str) -> bool {
        self.windows
            .lock()
            .expect("state mutex poisoned")
            .get(window_label)
            .map(|state| state.has_workspace)
            .unwrap_or(false)
    }

    fn set_has_workspace(&self, window_label: &str, has_workspace: bool) {
        if let Some(state) = self
            .windows
            .lock()
            .expect("state mutex poisoned")
            .get_mut(window_label)
        {
            state.has_workspace = has_workspace;
        }
    }
}

fn label_for_path(path: Option<&PathBuf>) -> String {
    let path = path
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    let hash = hasher.finish();
    format!("repo-{:08x}", hash as u32)
}

pub fn run_gui(options: super::RunOptions) -> Result<()> {
    let app = tauri::Builder::default()
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
                    if options.debug {
                        LevelFilter::Debug
                    } else {
                        LevelFilter::Warn
                    },
                )
                .level_for(
                    "tao",
                    if options.debug {
                        LevelFilter::Info
                    } else {
                        LevelFilter::Error
                    },
                )
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            forward_accelerator,
            forward_context_menu,
            forward_clone_url,
            init_repository,
            clone_repository,
            query_workspace,
            query_recent_workspaces,
            query_log,
            query_log_next_page,
            query_revision,
            query_remotes,
            query_snapshot,
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
        ])
        .menu(menu::build_main)
        .manage(AppState::new(options.settings))
        .setup(move |app| {
            // after tauri initialises NSApplication, set the dock icon if we're running as CLI
            #[cfg(all(target_os = "macos", not(feature = "app")))]
            {
                crate::macos::set_dock_icon();
            }

            // open initial repo, unless a deep link's already done so
            #[cfg(target_os = "macos")]
            let has_window = recent_items::received_event();
            #[cfg(not(target_os = "macos"))]
            let has_window = false;

            if !has_window {
                try_create_window(app.handle(), options.workspace.clone())?;
            }

            if options.is_child {
                println!("Startup complete.");
            }

            Ok(())
        });

    #[cfg(target_os = "macos")]
    let app = app.plugin(recent_items::init());

    app.run(options.context)?;

    Ok(())
}

#[tauri::command(async)]
fn query_workspace(
    window: Window,
    path: Option<String>,
) -> Result<messages::RepoConfig, InvokeError> {
    log::debug!("query_workspace: {path:?}");
    handler::fatal!(window.show());
    handler::optional!(window.set_focus());
    try_open_repository(&window, path.map(PathBuf::from)).map_err(InvokeError::from_anyhow)
}

#[tauri::command]
fn forward_accelerator(window: Window, state: State<AppState>, key: char, ctrl: bool, shift: bool) {
    match (key, ctrl, shift) {
        ('o', true, false) => menu::repo_open(&window),
        ('o', true, true) => menu::repo_clone(&window),
        ('n', true, true) => menu::repo_init(&window),
        ('n', true, false) => {
            if state.get_selection(window.label()).is_some() {
                handler::nonfatal!(window.emit_to(
                    EventTarget::window(window.label()),
                    "gg://menu/revision",
                    "new_child"
                ));
            }
        }
        ('m', true, false) => {
            if let Some(header) = state.get_selection(window.label()) {
                if !header.is_immutable && header.parent_ids.len() == 1 {
                    handler::nonfatal!(window.emit_to(
                        EventTarget::window(window.label()),
                        "gg://menu/revision",
                        "new_parent"
                    ));
                }
            }
        }
        _ => (),
    }
}

#[tauri::command]
fn forward_context_menu(window: Window, context: messages::Operand) -> Result<(), InvokeError> {
    menu::handle_context(window, context).map_err(InvokeError::from_anyhow)?;
    Ok(())
}

#[tauri::command(async)]
fn forward_clone_url(window: Window, url: String) {
    menu::repo_clone_with_url(&window, url);
}

#[tauri::command]
fn init_repository(
    window: Window,
    app_state: State<AppState>,
    mutation: InitRepository,
) -> Result<MutationResult, InvokeError> {
    log::debug!(
        "init_repository {}, colocated: {}",
        mutation.path,
        mutation.colocated
    );

    let session_tx: Sender<SessionEvent> = app_state.get_session(window.label());
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::InitWorkspace {
            tx: call_tx,
            wd: PathBuf::from(&mutation.path),
            colocated: mutation.colocated,
        })
        .map_err(InvokeError::from_error)?;

    let initialized_path = call_rx
        .recv()
        .map_err(InvokeError::from_error)?
        .map_err(InvokeError::from_anyhow)?;

    let result = match try_reopen_repository(&window, initialized_path) {
        Ok(None) => MutationResult::Unchanged,
        Ok(Some(new_config)) => MutationResult::Reconfigured { new_config },
        Err(err) => MutationResult::InternalError {
            message: (&*format!("{err:?}")).into(),
        },
    };

    Ok(result)
}

#[tauri::command(async)]
fn clone_repository(
    window: Window,
    app_state: State<AppState>,
    mutation: CloneRepository,
) -> Result<MutationResult, InvokeError> {
    log::debug!(
        "clone_repository {} -> {}, colocated: {}",
        mutation.url,
        mutation.path,
        mutation.colocated
    );

    let session_tx: Sender<SessionEvent> = app_state.get_session(window.label());
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::CloneWorkspace {
            tx: call_tx,
            source_url: mutation.url.clone(),
            wd: PathBuf::from(&mutation.path),
            colocated: mutation.colocated,
        })
        .map_err(InvokeError::from_error)?;

    let cloned_path = call_rx
        .recv()
        .map_err(InvokeError::from_error)?
        .map_err(InvokeError::from_anyhow)?;

    let result = match try_reopen_repository(&window, cloned_path) {
        Ok(None) => MutationResult::Unchanged,
        Ok(Some(new_config)) => MutationResult::Reconfigured { new_config },
        Err(err) => MutationResult::InternalError {
            message: (&*format!("{err:?}")).into(),
        },
    };

    Ok(result)
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
    id: messages::RevId,
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
fn query_snapshot(
    window: Window,
    app_state: State<AppState>,
) -> Result<Option<messages::RepoStatus>, InvokeError> {
    let session_tx: Sender<SessionEvent> = app_state.get_session(window.label());
    let (call_tx, call_rx) = channel();

    session_tx
        .send(SessionEvent::ExecuteSnapshot { tx: call_tx })
        .map_err(InvokeError::from_error)?;
    call_rx.recv().map_err(InvokeError::from_error)
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

pub fn try_create_window(app_handle: &AppHandle, workspace: Option<PathBuf>) -> Result<()> {
    log::debug!("try_create_window: {:?}", workspace);

    let label = label_for_path(workspace.as_ref());

    if let Some(existing) = app_handle.get_webview_window(&label) {
        existing.set_focus()?;
        return Ok(());
    }

    // configure and register a new window
    let window = WebviewWindowBuilder::new(
        app_handle,
        &label,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("GG - Gui for JJ")
    .inner_size(1280.0, 720.0)
    .focused(true)
    .visible(false)
    .disable_drag_drop_handler()
    .build()?;

    let app_state = app_handle.state::<AppState>();
    let settings = app_state.settings.clone();

    // create a worker for the specified path
    let (sender, receiver) = channel();

    let handle = window.as_ref().window();
    let window_worker = thread::spawn(move || {
        async_runtime::block_on(worker_thread(handle, receiver, workspace, settings))
    });

    // setup command dependencies
    let (revision_menu, tree_menu, ref_menu) = menu::build_context(app_handle)?;

    let windows = app_state.windows.clone();
    windows.lock().unwrap().insert(
        window.label().to_owned(),
        WindowState {
            _worker: window_worker,
            worker_channel: sender,
            revision_menu,
            tree_menu,
            ref_menu,
            selection: None,
            has_workspace: false,
        },
    );

    // window lifecycle events
    let handle = window.as_ref().window();
    window.on_window_event(move |event| {
        handler::nonfatal!(handle_window_event(&handle, event));
    });

    // menu selection events
    window.on_menu_event(|w, e| handler::fatal!(menu::handle_event(w, e)));

    // menu enablement events
    let windows = app_state.windows.clone();
    let handle = app_handle.clone();
    let label = window.label().to_owned();
    window.listen("gg://revision/select", move |event| {
        let payload: Result<Option<messages::RevHeader>, serde_json::Error> =
            serde_json::from_str(event.payload());
        if let Ok(selection) = payload {
            if let Some(state) = windows.lock().unwrap().get_mut(&label) {
                state.selection = selection.clone();
            }
            if let Some(menu) = handle.menu() {
                handler::fatal!(menu::handle_selection(menu, selection));
            }
        }
    });

    Ok(())
}

async fn worker_thread(
    window: Window,
    rx: std::sync::mpsc::Receiver<SessionEvent>,
    workspace: Option<PathBuf>,
    settings: UserSettings,
) {
    log::info!("Worker started.");

    let progress = TauriSink::new(window.clone());

    while let Err(err) = WorkerSession::new(workspace.clone(), settings.clone(), progress.clone())
        .handle_events(&rx)
        .await
        .context("worker")
    {
        log::debug!("restart worker: {err:#}");

        // mark window as unloaded
        let app_state = window.state::<AppState>();
        app_state.set_has_workspace(window.label(), false);

        // it's ok if the worker has to restart, as long as we can notify the frontend of it
        handler::fatal!(window.emit_to(
            EventTarget::labeled(window.label()),
            "gg://repo/config",
            messages::RepoConfig::WorkerError {
                message: format!("{err:#}"),
            },
        ));
    }
}

fn reopen_repository(window: &Window, wd: PathBuf) -> Result<()> {
    if let Some(config) = try_reopen_repository(window, wd)? {
        window.emit_to(
            EventTarget::window(window.label()),
            "gg://repo/config",
            config,
        )?;
    }

    Ok(())
}

fn try_reopen_repository(window: &Window, wd: PathBuf) -> Result<Option<messages::RepoConfig>> {
    let state = window.state::<AppState>();

    if state.get_has_workspace(window.label()) {
        let app = window.app_handle().clone();
        tauri::async_runtime::spawn(async move {
            handler::nonfatal!(try_create_window(&app, Some(wd)).context("try_create_window"));
        });
        Ok(None)
    } else {
        Ok(Some(
            try_open_repository(window, Some(wd)).context("try_open_repository")?,
        ))
    }
}

fn try_open_repository(window: &Window, cwd: Option<PathBuf>) -> Result<messages::RepoConfig> {
    log::debug!("load workspace {cwd:#?}");

    let app_state = window.state::<AppState>();

    let session_tx: Sender<SessionEvent> = app_state.get_session(window.label());
    let (call_tx, call_rx) = channel();

    session_tx.send(SessionEvent::OpenWorkspace {
        tx: call_tx,
        wd: cwd.clone(),
    })?;

    let config = match call_rx.recv()? {
        Ok(config) => {
            log::debug!("load workspace succeeded");
            config
        }
        Err(err) => {
            log::warn!("load workspace failed: {err}");
            messages::RepoConfig::LoadError {
                absolute_path: cwd.unwrap_or_default().into(),
                message: format!("{:#}", err),
            }
        }
    };

    // mark window as loaded/unloaded
    let app_state = window.state::<AppState>();
    match &config {
        messages::RepoConfig::Workspace {
            absolute_path,
            track_recent_workspaces,
            ..
        } => {
            app_state.set_has_workspace(window.label(), true);

            let workspace_path = absolute_path.0.clone();
            _ = window.set_title((String::from("GG - ") + workspace_path.as_str()).as_str());

            // update config and jump lists - this can be slow
            if *track_recent_workspaces {
                let window = window.clone();
                thread::spawn(move || {
                    handler::nonfatal!(add_recent_workspaces(window, workspace_path));
                });
            }
        }
        _ => {
            app_state.set_has_workspace(window.label(), false);

            let _ = window.set_title("GG - Gui for JJ");
        }
    }

    Ok(config)
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

fn handle_window_event(window: &Window, event: &WindowEvent) -> Result<()> {
    match *event {
        WindowEvent::CloseRequested { .. } => {
            // not only does tauri not do this, it's got an internal UAF!
            window.remove_menu()?;
        }
        WindowEvent::Destroyed => {
            let app_state = window.state::<AppState>();
            app_state.windows.lock().unwrap().remove(window.label());
        }
        WindowEvent::Focused(true) => {
            log::debug!("window focused; notifying frontend");

            let app_state = window.state::<AppState>();

            let selection = app_state.get_selection(window.label());
            if let Some(menu) = window.app_handle().menu() {
                menu::handle_selection(menu, selection)?;
            }

            window.emit_to(EventTarget::labeled(window.label()), "gg://focus", ())?;
        }
        _ => (),
    }

    Ok(())
}

// we're working with OS bindings that _don't_ use OsStr/PathBuf
fn add_recent_workspaces(window: Window, workspace_path: String) -> Result<()> {
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
    recent.retain(|x| x != &workspace_path);
    recent.insert(0, workspace_path.clone());
    recent.truncate(10);

    #[cfg(windows)]
    {
        crate::windows::update_jump_list(&mut recent)?;
    }

    #[cfg(target_os = "macos")]
    {
        window
            .app_handle()
            .run_on_main_thread(move || {
                crate::macos::note_recent_document(workspace_path);
            })
            .ok();
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
