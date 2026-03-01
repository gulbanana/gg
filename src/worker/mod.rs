//! Worker thread that owns all jj-lib state for a single window or connection.
//!
//! jj-lib is not thread-safe, so each window gets its own worker thread.
//! The thread runs a state machine with three levels:
//!
//! 1. **[`WorkerSession`]** — initial state before a repository is loaded.
//!    Handles `OpenWorkspace`, `InitWorkspace`, and `CloneWorkspace` events.
//! 2. **`WorkspaceSession`** (internal) — a repository is loaded and ready for
//!    queries and mutations.
//! 3. **`QuerySession`** (internal) — a log query is in progress and the
//!    frontend is paging through results.
//!
//! Callers drive the worker by sending [`SessionEvent`] messages over a
//! standard [`std::sync::mpsc`] channel. See [`web::create_app`](crate::web::create_app)
//! for an example of wiring this up.

mod gui_util;
mod mutations;
mod queries;
mod session;
#[cfg(all(test, not(feature = "ts-rs")))]
mod tests;

use std::env::{self, VarError};
use std::fmt::Debug;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Error, Result, anyhow};
use jj_lib::git::{GitFetch, GitFetchRefExpression, GitSettings};
use jj_lib::ref_name::{RefNameBuf, RemoteName, RemoteNameBuf, RemoteRefSymbol};
use jj_lib::repo::{Repo, StoreFactories};
use jj_lib::settings::UserSettings;
use jj_lib::str_util::StringExpression;
use jj_lib::workspace::{
    self, DefaultWorkspaceLoaderFactory, Workspace, WorkspaceLoaderFactory as _,
};
use jj_lib::{backend::CommitId, git};
use serde::Serialize;

use jj_cli::{git_util::load_git_import_options, ui::Ui};

use crate::git_util::AuthContext;
use crate::messages;
use gui_util::WorkspaceSession;
pub use session::{Session, SessionEvent};

/// A repository-modifying operation (e.g. abandon, rebase, bookmark create).
///
/// Concrete mutation types live in [`crate::messages`] and are deserialized
/// from frontend requests. This trait is effectively sealed — [`execute`](Mutation::execute)
/// takes a `WorkspaceSession` which is not exported — so it cannot be
/// implemented outside of this crate.
#[async_trait::async_trait(?Send)]
pub trait Mutation: Debug {
    /// Human-readable name used in error messages and logging.
    /// Defaults to the Rust type name.
    fn describe(&self) -> String {
        std::any::type_name::<Self>().to_owned()
    }

    /// Run the mutation against a loaded workspace. Implementations should
    /// start a jj transaction, perform their changes, and commit it.
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        options: &messages::MutationOptions,
    ) -> Result<messages::MutationResult>;

    /// Test helper that avoids the `Box<Self>` indirection and uses default
    /// options.
    #[cfg(test)]
    async fn execute_unboxed(self, ws: &mut WorkspaceSession) -> Result<messages::MutationResult>
    where
        Self: Sized,
    {
        Box::new(self)
            .execute(ws, &messages::MutationOptions::default())
            .await
    }
}

/// Abstraction over how backend-to-frontend push events are delivered.
///
/// In GUI mode this wraps Tauri's event emitter; in web mode it feeds a
/// broadcast channel that clients consume via Server-Sent Events (SSE).
/// Provide your own implementation to integrate with a different transport.
pub trait EventSink: Send + Sync {
    /// Emit a named event with a pre-serialized JSON payload.
    fn send(&self, event_name: &str, payload: serde_json::Value);
}

/// Convenience extension that serializes a typed payload before sending.
///
/// Automatically implemented for all `EventSink` implementors — kept as a
/// separate trait so that `EventSink` itself remains dyn-compatible.
pub trait EventSinkExt: EventSink {
    /// Serialize `payload` to JSON and forward to [`EventSink::send`].
    fn send_typed<T: Serialize>(&self, event_name: &str, payload: &T) {
        let value = serde_json::to_value(payload).expect("T: Serialize");
        self.send(event_name, value)
    }
}

impl<S: EventSink + ?Sized> EventSinkExt for S {}

/// Initial state of the worker thread, before any repository is loaded.
///
/// Create one with [`WorkerSession::new`], then call
/// [`handle_events`](Session::handle_events) in a dedicated thread to start
/// processing [`SessionEvent`] messages. The session automatically transitions
/// to an internal `WorkspaceSession` when it receives
/// [`SessionEvent::OpenWorkspace`].
///
/// See the [crate-level docs](crate) for a usage overview.
pub struct WorkerSession {
    /// Override the default log page size (mainly useful in tests).
    pub force_log_page_size: Option<usize>,
    /// Tracks the most recently evaluated revset string for paging.
    pub latest_query: Option<String>,
    /// Working directory to open when no explicit path is given.
    pub working_directory: Option<PathBuf>,
    /// Channel for pushing events to the frontend.
    pub sink: Arc<dyn EventSink>,
    /// Jujutsu user settings (loaded from `jj config`).
    pub user_settings: UserSettings,
    /// When `true`, immutability checks on commits are skipped.
    pub ignore_immutable: bool,
}

impl WorkerSession {
    /// Create a new worker session.
    ///
    /// After construction, spawn a thread and call
    /// [`handle_events`](Session::handle_events) with the receiving end of
    /// a [`SessionEvent`] channel.
    pub fn new(
        sink: Arc<dyn EventSink>,
        working_directory: Option<PathBuf>,
        user_settings: UserSettings,
        ignore_immutable: bool,
    ) -> Self {
        WorkerSession {
            force_log_page_size: None,
            latest_query: None,
            sink,
            working_directory,
            user_settings,
            ignore_immutable,
        }
    }

    /// Resolve the effective working directory.
    ///
    /// Checks, in order: the explicit `working_directory` field, the `$OWD`
    /// environment variable (set by AppImage), and finally `std::env::current_dir`.
    fn get_cwd(&self) -> Result<PathBuf> {
        self.working_directory
            .as_ref()
            .map(|cwd| Ok(fs::canonicalize(cwd.clone())?))
            .or_else(|| match env::var("OWD") {
                Ok(var) => Some(Ok(PathBuf::from(var))),
                Err(VarError::NotPresent) => None,
                Err(err) => Some(Err(anyhow!(err))),
            })
            .unwrap_or_else(|| env::current_dir().map_err(Error::new))
    }

    /// Initialize a new Jujutsu repository at `location`.
    ///
    /// When `colocated` is `true` and a `.git/` directory already exists, the
    /// new repo shares it; otherwise a fresh Git backend is created (either
    /// colocated or internal depending on the flag). Returns the canonicalized
    /// path on success.
    fn init_workspace(&self, location: &PathBuf, colocated: bool) -> Result<PathBuf> {
        // precondition: not already a jj repo
        let jj_path = location.join(".jj");
        if jj_path.exists() {
            return Err(anyhow!(
                "A Jujutsu repository already exists in this directory"
            ));
        }

        let canonical_location = dunce::canonicalize(location)?;
        let (settings, _, _) = crate::config::read_config(None)?;

        if colocated {
            let git_path = location.join(".git");
            if git_path.exists() {
                Workspace::init_external_git(&settings, &canonical_location, &git_path)?; // existing .git/, create .jj
            } else {
                Workspace::init_colocated_git(&settings, &canonical_location)?; // create .git/ and .jj/
            }
        } else {
            Workspace::init_internal_git(&settings, &canonical_location)?; // create .jj/ with a .git/ inside it
        }

        Ok(canonical_location)
    }

    /// Clone a remote repository into `location`.
    ///
    /// This is a three-stage process: initialize an empty repo, fetch from
    /// `source_url` (added as the `origin` remote), and check out the default
    /// branch. Progress events are pushed through the session's [`EventSink`].
    /// Returns the canonicalized path on success.
    fn clone_workspace(
        &self,
        source_url: &str,
        location: &PathBuf,
        colocated: bool,
    ) -> Result<PathBuf> {
        // precondition: new or empty location
        if location.exists() {
            let has_files = fs::read_dir(location)
                .context("Failed to read destination directory")?
                .next()
                .is_some();
            if has_files {
                return Err(anyhow!(
                    "Destination directory is not empty: {}",
                    location.display()
                ));
            }
        } else {
            fs::create_dir_all(location).context("Failed to create destination directory")?;
        }

        let canonical_location = dunce::canonicalize(location)?;
        let (settings, _, _) = crate::config::read_config(None)?;

        // init empty
        let (_workspace, repo) = if colocated {
            Workspace::init_colocated_git(&settings, &canonical_location)?
        } else {
            Workspace::init_internal_git(&settings, &canonical_location)?
        };

        // add origin
        let remote_name = RemoteName::new("origin");
        {
            let mut tx = repo.start_transaction();
            git::add_remote(
                tx.repo_mut(),
                remote_name,
                source_url,
                None, // push_url = fetch_url
                gix::remote::fetch::Tags::Included,
                &StringExpression::all(),
            )
            .context("add_remote(origin)")?;
            tx.commit("add git remote origin")?;
        }

        // reload to apply config
        let loader = DefaultWorkspaceLoaderFactory.create(&canonical_location)?;
        let workspace = loader.load(
            &settings,
            &StoreFactories::default(),
            &workspace::default_working_copy_factories(),
        )?;
        let repo = workspace.repo_loader().load_at_head()?;

        // fetch from origin
        let mut auth_ctx = AuthContext::new(None);
        let git_settings = GitSettings::from_settings(&settings)?;
        let remote_settings = settings.remote_settings()?;
        let import_options = load_git_import_options(&Ui::null(), &git_settings, &remote_settings)
            .map_err(|e| Error::new(e.error))?;

        let git_head_id = auth_ctx.with_callbacks(Some(self.sink.clone()), |cb, env| {
            let mut subprocess_options = git_settings.to_subprocess_options();
            subprocess_options.environment = env;

            let mut tx = repo.start_transaction();
            let mut fetcher = GitFetch::new(tx.repo_mut(), subprocess_options, &import_options)?;
            let refspecs = git::expand_fetch_refspecs(
                remote_name,
                GitFetchRefExpression {
                    bookmark: StringExpression::all(),
                    tag: StringExpression::none(),
                },
            )?;

            fetcher
                .fetch(remote_name, refspecs, cb, None, None)
                .context("Failed to fetch from remote")?;

            fetcher.import_refs().context("Failed to import refs")?;

            // find HEAD if at all possible
            let workspace_name = workspace.workspace_name().to_owned();
            let mut checkout_commit_id = None;
            if let Some(bookmark_name) = Self::find_default_branch(tx.repo()) {
                let bookmark_ref = RefNameBuf::from(bookmark_name.as_str());
                let remote_ref = RemoteNameBuf::from("origin");
                let symbol = RemoteRefSymbol {
                    name: &bookmark_ref,
                    remote: &remote_ref,
                };
                let remote_bookmark = tx.repo().view().get_remote_bookmark(symbol);
                if let Some(commit_id) = remote_bookmark.target.as_normal().cloned() {
                    let commit = tx.repo().store().get_commit(&commit_id)?;
                    tx.repo_mut().track_remote_bookmark(symbol)?;
                    tx.repo_mut().check_out(workspace_name, &commit)?;

                    checkout_commit_id = Some(commit_id);
                }
            }

            // nominal rebase to preserve invariants
            tx.repo_mut().rebase_descendants()?;
            tx.commit("clone from git remote")?;

            Ok::<Option<CommitId>, Error>(checkout_commit_id)
        })?;

        // check out the working copy after one last reload
        if let Some(wc_id) = git_head_id {
            let loader = DefaultWorkspaceLoaderFactory.create(&canonical_location)?;
            let mut workspace = loader.load(
                &settings,
                &jj_lib::repo::StoreFactories::default(),
                &jj_lib::workspace::default_working_copy_factories(),
            )?;
            let repo = workspace.repo_loader().load_at_head()?;

            let commit = repo.store().get_commit(&wc_id)?;
            workspace.check_out(repo.op_id().clone(), None, &commit)?;
        }

        Ok(canonical_location)
    }

    fn find_default_branch(repo: &dyn jj_lib::repo::Repo) -> Option<String> {
        use jj_lib::ref_name::{RefNameBuf, RemoteNameBuf, RemoteRefSymbol};

        // common default branch names
        for bookmark_name in ["main", "master", "trunk", "develop"] {
            let bookmark_ref = RefNameBuf::from(bookmark_name);
            let remote_ref = RemoteNameBuf::from("origin");
            let symbol = RemoteRefSymbol {
                name: &bookmark_ref,
                remote: &remote_ref,
            };
            if repo.view().get_remote_bookmark(symbol).is_present() {
                return Some(bookmark_name.to_string());
            }
        }

        // any branch at all!
        for (symbol, _) in repo.view().all_remote_bookmarks() {
            if symbol.remote.as_str() == "origin" {
                return Some(symbol.name.as_str().to_string());
            }
        }

        None
    }
}
