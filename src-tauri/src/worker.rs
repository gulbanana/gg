//! Worker per window, owning repo data (jj-lib is not thread-safe)
//! The worker thread is a state machine, running different handle functions based on loaded data

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
};

use anyhow::{anyhow, Context, Result};

use crate::gui_util::{WorkerSession, WorkspaceSession};
use crate::messages;

pub mod mutations;
pub mod queries;

#[derive(Debug)]
pub enum SessionEvent {
    #[allow(dead_code)]
    EndSession,
    OpenWorkspace {
        tx: Sender<Result<messages::RepoConfig>>,
        cwd: Option<PathBuf>,
    },
    QueryLog {
        tx: Sender<Result<messages::LogPage>>,
        query: String,
    },
    QueryLogNextPage {
        tx: Sender<Result<messages::LogPage>>,
    },
    QueryRevision {
        tx: Sender<Result<messages::RevDetail>>,
        rev: String,
    },
    DescribeRevision {
        tx: Sender<messages::MutationResult>,
        mutation: messages::DescribeRevision,
    },
}

pub trait Session {
    type Transition;
    fn handle_events(self, rx: &Receiver<SessionEvent>) -> Result<Self::Transition>;
}

impl Session for WorkerSession {
    type Transition = ();

    fn handle_events(mut self, rx: &Receiver<SessionEvent>) -> Result<()> {
        loop {
            match rx.recv() {
                Ok(SessionEvent::EndSession) => return Ok(()),
                Ok(SessionEvent::OpenWorkspace { mut tx, mut cwd }) => loop {
                    let resolved_cwd = &cwd
                        .clone()
                        .unwrap_or_else(|| std::env::current_dir().unwrap());

                    let ws = match self.load_directory(resolved_cwd) {
                        Ok(ws) => ws,
                        Err(err) => {
                            tx.send(Ok(messages::RepoConfig::NoWorkspace {
                                absolute_path: resolved_cwd.into(),
                                error: format!("{err:#}"),
                            }))?;
                            break;
                        }
                    };

                    tx.send(Ok(ws.format_config()))?;

                    match ws.handle_events(rx).context("WorkspaceSession")? {
                        WorkspaceResult::Reopen(new_tx, new_cwd) => (tx, cwd) = (new_tx, new_cwd),
                        WorkspaceResult::SessionComplete => return Ok(()),
                    }
                },
                Ok(_) => {
                    return Err(anyhow::anyhow!(
                        "A repo must be loaded before any other operations"
                    ))
                }
                Err(err) => return Err(anyhow!(err)),
            };
        }
    }
}

pub enum WorkspaceResult {
    Reopen(Sender<Result<messages::RepoConfig>>, Option<PathBuf>),
    SessionComplete,
}

impl Session for WorkspaceSession<'_> {
    type Transition = WorkspaceResult;

    fn handle_events(mut self, rx: &Receiver<SessionEvent>) -> Result<WorkspaceResult> {
        let mut current_query: Option<queries::LogQuery> = None;

        loop {
            match rx.recv() {
                Ok(SessionEvent::EndSession) => return Ok(WorkspaceResult::SessionComplete),
                Ok(SessionEvent::OpenWorkspace { tx, cwd }) => {
                    return Ok(WorkspaceResult::Reopen(tx, cwd));
                }
                Ok(SessionEvent::QueryLog {
                    tx,
                    query: revset_string,
                }) => {
                    let query = current_query.insert(queries::LogQuery::new(revset_string.clone()));
                    let page = query.get_page(&self)?;
                    tx.send(Ok(page))?;

                    self.session.latest_query = Some(revset_string);
                }
                Ok(SessionEvent::QueryLogNextPage { tx }) => match current_query {
                    None => tx.send(Err(anyhow!("No log query is in progress")))?,
                    Some(ref mut query) => {
                        let page = query.get_page(&self)?;
                        tx.send(Ok(page))?;
                    }
                },
                Ok(SessionEvent::QueryRevision { tx, rev: rev_id }) => {
                    tx.send(queries::query_revision(&self, &rev_id))?
                }
                Ok(SessionEvent::DescribeRevision { tx, mutation }) => {
                    tx.send(mutations::describe_revision(&mut self, mutation)?)?
                }
                Err(err) => return Err(anyhow!(err)),
            };
        }
    }
}
