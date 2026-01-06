//! Worker per window, owning repo data (jj-lib is not thread-safe)
//! The worker thread is a state machine, running different handle functions based on loaded data

mod gui_util;
mod mutations;
mod queries;
mod session;
#[cfg(all(test, not(feature = "ts-rs")))]
mod tests;

use std::collections::HashMap;
use std::env::{self, VarError};
use std::fmt::Debug;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Error, Result, anyhow};
use jj_lib::git::{GitFetch, GitImportOptions, GitSettings};
use jj_lib::ref_name::{RefNameBuf, RemoteName, RemoteNameBuf, RemoteRefSymbol};
use jj_lib::repo::{Repo, StoreFactories};
use jj_lib::settings::UserSettings;
use jj_lib::str_util::StringExpression;
use jj_lib::workspace::{
    self, DefaultWorkspaceLoaderFactory, Workspace, WorkspaceLoaderFactory as _,
};
use jj_lib::{backend::CommitId, git};
use serde::Serialize;

use crate::git_util::AuthContext;
use crate::messages;
use gui_util::WorkspaceSession;
pub use session::{Session, SessionEvent};

/// implemented by structured-change commands
#[async_trait::async_trait(?Send)]
pub trait Mutation: Debug {
    fn describe(&self) -> String {
        std::any::type_name::<Self>().to_owned()
    }

    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
    ) -> Result<messages::MutationResult>;

    #[cfg(test)]
    async fn execute_unboxed(self, ws: &mut WorkspaceSession) -> Result<messages::MutationResult>
    where
        Self: Sized,
    {
        Box::new(self).execute(ws).await
    }
}

/// mode-specific dispatch mechanism for sending events to the frontend
pub trait EventSink: Send + Sync {
    fn send(&self, event_name: &str, payload: serde_json::Value);
}

// extension trait to keep EventSink dyn-compatible
pub trait EventSinkExt: EventSink {
    fn send_typed<T: Serialize>(&self, event_name: &str, payload: &T) {
        let value = serde_json::to_value(payload).expect("T: Serialize");
        self.send(event_name, value)
    }
}

impl<S: EventSink + ?Sized> EventSinkExt for S {}

/// state that doesn't depend on jj-lib borrowings
pub struct WorkerSession {
    pub force_log_page_size: Option<usize>,
    pub latest_query: Option<String>,
    pub working_directory: Option<PathBuf>,
    pub user_settings: UserSettings,
    pub sink: Arc<dyn EventSink>,
}

impl WorkerSession {
    pub fn new(
        workspace: Option<PathBuf>,
        user_settings: UserSettings,
        sink: Arc<dyn EventSink>,
    ) -> Self {
        WorkerSession {
            force_log_page_size: None,
            latest_query: None,
            working_directory: workspace,
            user_settings,
            sink,
        }
    }

    // AppImage runs the executable from somewhere weird, but sets OWD=cwd() first.
    pub fn get_cwd(&self) -> Result<PathBuf> {
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

    /// actually creates a repository, but jj considers that a special case of creating a workspace
    pub fn init_workspace(&self, location: &PathBuf, colocated: bool) -> Result<PathBuf> {
        // precondition: not already a jj repo
        let jj_path = location.join(".jj");
        if jj_path.exists() {
            return Err(anyhow!(
                "A Jujutsu repository already exists in this directory"
            ));
        }

        let canonical_location = dunce::canonicalize(location)?;
        let (settings, _) = crate::config::read_config(None)?;

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

    /// clone a repository, which is a three-stage process: init, fetch, checkout
    pub fn clone_workspace(
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
        let (settings, _) = crate::config::read_config(None)?;

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
                &remote_name,
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
        let import_options = GitImportOptions {
            auto_local_bookmark: git_settings.auto_local_bookmark,
            abandon_unreachable_commits: git_settings.abandon_unreachable_commits,
            remote_auto_track_bookmarks: HashMap::new(),
        };

        let git_head_id = auth_ctx.with_callbacks(Some(self.sink.clone()), |cb, env| {
            let mut subprocess_options = git_settings.to_subprocess_options();
            subprocess_options.environment = env;

            let mut tx = repo.start_transaction();
            let mut fetcher = GitFetch::new(tx.repo_mut(), subprocess_options, &import_options)?;
            let refspecs = git::expand_fetch_refspecs(&remote_name, StringExpression::all())?;

            fetcher
                .fetch(&remote_name, refspecs, cb, None, None)
                .context("Failed to fetch from remote")?;

            fetcher.import_refs().context("Failed to import refs")?;

            // find HEAD if at all possible
            let workspace_name = workspace.workspace_name().to_owned();
            let mut checkout_commit_id = None;
            if let Some(branch_name) = Self::find_default_branch(tx.repo()) {
                let branch_ref = RefNameBuf::from(branch_name.as_str());
                let remote_ref = RemoteNameBuf::from("origin");
                let symbol = RemoteRefSymbol {
                    name: &branch_ref,
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
        for branch_name in ["main", "master", "trunk", "develop"] {
            let branch_ref = RefNameBuf::from(branch_name);
            let remote_ref = RemoteNameBuf::from("origin");
            let symbol = RemoteRefSymbol {
                name: &branch_ref,
                remote: &remote_ref,
            };
            if repo.view().get_remote_bookmark(symbol).is_present() {
                return Some(branch_name.to_string());
            }
        }

        // any branch at all
        for (symbol, _) in repo.view().all_remote_bookmarks() {
            if symbol.remote.as_str() == "origin" {
                return Some(symbol.name.as_str().to_string());
            }
        }

        None
    }
}
