//! Worker per window, owning repo data (jj-lib is not thread-safe)
//! The worker thread is a state machine, running different handle functions based on loaded data

mod gui_util;
mod mutations;
mod queries;
mod session;
#[cfg(all(test, not(feature = "ts-rs")))]
mod tests;

use std::{
    env::{self, VarError},
    fmt::Debug,
    fs,
    path::PathBuf,
    sync::Arc,
};

use anyhow::{Error, Result, anyhow};
use jj_lib::{settings::UserSettings, workspace::Workspace};
use serde::Serialize;

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
}
