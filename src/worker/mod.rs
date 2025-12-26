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
use jj_lib::settings::UserSettings;

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

/// state that doesn't depend on jj-lib borrowings
pub struct WorkerSession {
    pub force_log_page_size: Option<usize>,
    pub latest_query: Option<String>,
    pub working_directory: Option<PathBuf>,
    pub user_settings: UserSettings,
}

impl WorkerSession {
    pub fn new(workspace: Option<PathBuf>, user_settings: UserSettings) -> Self {
        WorkerSession {
            force_log_page_size: None,
            latest_query: None,
            working_directory: workspace,
            user_settings,
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
