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
};

use anyhow::{Error, Result, anyhow};
use jj_lib::{git::RemoteCallbacks, repo::MutableRepo, settings::UserSettings};

use crate::{config::read_config, messages};
use gui_util::WorkspaceSession;
pub use session::{Session, SessionEvent};

/// implemented by structured-change commands
pub trait Mutation: Debug {
    fn describe(&self) -> String {
        std::any::type_name::<Self>().to_owned()
    }

    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<messages::MutationResult>;

    #[cfg(test)]
    fn execute_unboxed(self, ws: &mut WorkspaceSession) -> Result<messages::MutationResult>
    where
        Self: Sized,
    {
        Box::new(self).execute(ws)
    }
}

/// implemented by UI layers to request user input and receive progress
pub trait WorkerCallbacks {
    fn with_git(
        &self,
        repo: &mut MutableRepo,
        f: &dyn Fn(&mut MutableRepo, RemoteCallbacks<'_>) -> Result<()>,
    ) -> Result<()>;
}

struct NoCallbacks;

impl WorkerCallbacks for NoCallbacks {
    fn with_git(
        &self,
        repo: &mut MutableRepo,
        f: &dyn Fn(&mut MutableRepo, RemoteCallbacks<'_>) -> Result<()>,
    ) -> Result<()> {
        f(repo, RemoteCallbacks::default())
    }
}

/// state that doesn't depend on jj-lib borrowings
pub struct WorkerSession {
    pub force_log_page_size: Option<usize>,
    pub latest_query: Option<String>,
    pub callbacks: Box<dyn WorkerCallbacks>,
    pub working_directory: Option<PathBuf>,
    #[allow(dead_code, reason = "read in handle_events")]
    pub global_settings: Option<UserSettings>,
}

impl WorkerSession {
    pub fn new<T: WorkerCallbacks + 'static>(callbacks: T, workspace: Option<PathBuf>) -> Self {
        let global_settings = match read_config(None) {
            Ok((settings, _)) => Some(settings),
            Err(_) => None,
        };
        WorkerSession {
            callbacks: Box::new(callbacks),
            working_directory: workspace,
            global_settings,
            ..Default::default()
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
}

impl Default for WorkerSession {
    fn default() -> Self {
        WorkerSession {
            force_log_page_size: None,
            latest_query: None,
            callbacks: Box::new(NoCallbacks),
            working_directory: None,
            global_settings: None,
        }
    }
}
