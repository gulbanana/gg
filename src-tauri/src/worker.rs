//! Worker per window, owning repo data (jj-lib is not thread-safe)
//! The worker thread is a state machine, running different handle functions based on loaded data

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
};

use anyhow::{anyhow, Context, Result};

use crate::gui_util::{WorkerSession, WorkspaceSession};
use crate::messages;

use self::queries::LogQueryState;

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
        change_id: String,
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
        let mut unhandled_event: Option<SessionEvent> = None;
        let mut unpaged_query: Option<LogQueryState> = None;

        loop {
            let next_event = if unhandled_event.is_some() {
                unhandled_event.take().unwrap()
            } else {
                rx.recv()?
            };

            match next_event {
                SessionEvent::EndSession => return Ok(WorkspaceResult::SessionComplete),
                SessionEvent::OpenWorkspace { tx, cwd } => {
                    return Ok(WorkspaceResult::Reopen(tx, cwd));
                }
                SessionEvent::QueryLog {
                    tx,
                    query: revset_string,
                } => {
                    self.session.latest_query = Some(revset_string.clone());

                    let revset = self
                        .evaluate_revset_str(&revset_string)
                        .context("evaluate revset")?;

                    let mut query = queries::LogQuery::new(
                        &self,
                        &*revset,
                        LogQueryState::new(self.session.log_page_size),
                    )?;

                    let page = query.get_page()?;

                    tx.send(Ok(page))?;

                    let QueryResult(next_event, next_query) =
                        query.handle_events(rx).context("LogQuery")?;
                    unhandled_event = Some(next_event);
                    unpaged_query = Some(next_query);
                }
                SessionEvent::QueryLogNextPage { tx } => {
                    let revset_string = self
                        .session
                        .latest_query
                        .as_ref()
                        .ok_or_else(|| anyhow!("NextPage called without query in progress"))?;

                    let state = unpaged_query
                        .take()
                        .ok_or_else(|| anyhow!("NextPage called without query in progress"))?;

                    let revset = self
                        .evaluate_revset_str(&revset_string)
                        .context("evaluate revset")?;

                    let mut query = queries::LogQuery::new(&self, &*revset, state)?;

                    let page = query.get_page();
                    tx.send(page)?;

                    let QueryResult(next_event, next_query) =
                        query.handle_events(rx).context("LogQuery")?;
                    unhandled_event = Some(next_event);
                    unpaged_query = Some(next_query);
                }
                SessionEvent::QueryRevision { tx, change_id } => {
                    tx.send(queries::query_revision(&self, &change_id))?
                }
                SessionEvent::DescribeRevision { tx, mutation } => {
                    tx.send(mutations::describe_revision(&mut self, mutation)?)?
                }
            };
        }
    }
}

pub struct QueryResult(SessionEvent, LogQueryState);

impl Session for queries::LogQuery<'_, '_> {
    type Transition = QueryResult;

    fn handle_events(mut self, rx: &Receiver<SessionEvent>) -> Result<Self::Transition> {
        loop {
            match rx.recv() {
                Ok(SessionEvent::QueryLogNextPage { tx }) => {
                    let page = self.get_page();
                    tx.send(page)?;
                }
                Ok(unhandled) => return Ok(QueryResult(unhandled, self.state)),
                Err(err) => return Err(anyhow!(err)),
            };
        }
    }
}
