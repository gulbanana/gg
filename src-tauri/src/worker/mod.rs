//! Worker per window, owning repo data (jj-lib is not thread-safe)
//! The worker thread is a state machine, running different handle functions based on loaded data

mod gui_util;
mod mutations;
mod queries;
mod session;
#[cfg(all(test, not(feature = "ts-rs")))]
mod tests;

use std::fmt::Debug;

use anyhow::Result;
use jj_lib::{git::RemoteCallbacks, repo::MutableRepo};

use crate::messages;
use gui_util::WorkspaceSession;
pub use session::{Session, SessionEvent};

/// implemented by structured-change commands
pub trait Mutation: Debug {
    fn describe(&self) -> String {
        std::any::type_name::<Self>().to_owned()
    }

    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<messages::MutationResult>;

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
}

impl WorkerSession {
    pub fn new<T: WorkerCallbacks + 'static>(callbacks: T) -> Self {
        WorkerSession {
            callbacks: Box::new(callbacks),
            ..Default::default()
        }
    }
}

impl Default for WorkerSession {
    fn default() -> Self {
        WorkerSession {
            force_log_page_size: None,
            latest_query: None,
            callbacks: Box::new(NoCallbacks),
        }
    }
}
