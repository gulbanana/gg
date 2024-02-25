//! Worker per window, owning repo data (jj-lib is not thread-safe)
//! The worker is organised as a matryoshka doll of state machines, each owning more session data than the one in which it is contained

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
};

use anyhow::{anyhow, Context, Result};

use crate::{
    gui_util::{SessionEvaluator, SessionOperation, WorkspaceSession},
    messages,
};

mod mutations;
mod queries;

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

// mutable state that doesn't depend on jj-lib borrowings
#[derive(Default)]
pub struct Session {
    latest_query: Option<String>,
}

enum WorkspaceResult {
    Reopen(Sender<Result<messages::RepoConfig>>, Option<PathBuf>),
    SessionComplete,
}

enum QueryResult {
    Workspace(WorkspaceResult),
    Requery(Sender<Result<messages::LogPage>>, String),
    QueryComplete,
}

impl Session {
    pub fn main(&mut self, rx: &Receiver<SessionEvent>) -> Result<()> {
        loop {
            match rx.recv() {
                Ok(SessionEvent::EndSession) => return Ok(()),
                Ok(SessionEvent::OpenWorkspace { mut tx, mut cwd }) => loop {
                    let wd = &cwd
                        .clone()
                        .unwrap_or_else(|| std::env::current_dir().unwrap());

                    let session = match WorkspaceSession::from_cwd(wd) {
                        Ok(session) => session,
                        Err(err) => {
                            tx.send(Ok(messages::RepoConfig::NoWorkspace {
                                absolute_path: wd.into(),
                                error: format!("{err:#}"),
                            }))?;
                            break;
                        }
                    };

                    let op = match SessionOperation::from_head(&session) {
                        Ok(op) => op,
                        Err(err) => {
                            tx.send(Ok(messages::RepoConfig::NoOperation {
                                absolute_path: wd.into(),
                                error: format!("{err:#}"),
                            }))?;
                            break;
                        }
                    };

                    let eval = SessionEvaluator::from_operation(&op);

                    tx.send(Ok(op.format_config(self.latest_query.clone())))?;

                    match self
                        .with_workspace(rx, &session, &op, &eval)
                        .context("with_workspace")?
                    {
                        WorkspaceResult::Reopen(new_tx, new_cwd) => (tx, cwd) = (new_tx, new_cwd),
                        WorkspaceResult::SessionComplete => return Ok(()),
                    };
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

    fn with_workspace(
        &mut self,
        rx: &Receiver<SessionEvent>,
        session: &WorkspaceSession,
        op: &SessionOperation,
        eval: &SessionEvaluator,
    ) -> Result<WorkspaceResult> {
        loop {
            match rx.recv() {
                Ok(SessionEvent::EndSession) => return Ok(WorkspaceResult::SessionComplete),
                Ok(SessionEvent::OpenWorkspace { tx, cwd }) => {
                    return Ok(WorkspaceResult::Reopen(tx, cwd));
                }
                Ok(SessionEvent::QueryLog {
                    mut tx,
                    query: mut revset_string,
                }) => loop {
                    let revset = eval
                        .evaluate_revset(&revset_string)
                        .context("evaluate revset")?;
                    self.latest_query = Some(revset_string);
                    let mut query = queries::LogQuery::new(&*revset);
                    let first_page = query.get(&op)?;
                    let incomplete = first_page.has_more;
                    tx.send(Ok(first_page))?;

                    if incomplete {
                        match Self::with_query(rx, session, &op, &eval, &mut query)
                            .context("state_query")?
                        {
                            QueryResult::Workspace(r) => return Ok(r),
                            QueryResult::Requery(new_tx, new_revset_string) => {
                                (tx, revset_string) = (new_tx, new_revset_string)
                            }
                            QueryResult::QueryComplete => break,
                        };
                    } else {
                        break;
                    }
                },
                Ok(SessionEvent::QueryLogNextPage { tx: _tx }) => {
                    return Err(anyhow!("No log query is in progress"))
                }
                Ok(SessionEvent::QueryRevision { tx, rev: rev_id }) => {
                    tx.send(queries::query_revision(&op, &rev_id))?
                }
                Ok(SessionEvent::DescribeRevision { tx, mutation }) => {
                    tx.send(mutations::describe_revision(&op, mutation)?)?
                }
                Err(err) => return Err(anyhow!(err)),
            };
        }
    }

    fn with_query(
        rx: &Receiver<SessionEvent>,
        _session: &WorkspaceSession,
        op: &SessionOperation,
        _eval: &SessionEvaluator,
        query: &mut queries::LogQuery,
    ) -> Result<QueryResult> {
        loop {
            match rx.recv() {
                Ok(SessionEvent::EndSession) => {
                    return Ok(QueryResult::Workspace(WorkspaceResult::SessionComplete));
                }
                Ok(SessionEvent::OpenWorkspace { tx, cwd }) => {
                    return Ok(QueryResult::Workspace(WorkspaceResult::Reopen(tx, cwd)));
                }
                Ok(SessionEvent::QueryLog { tx, query }) => {
                    return Ok(QueryResult::Requery(tx, query));
                }
                Ok(SessionEvent::QueryLogNextPage { tx }) => {
                    let page = query.get(&op);
                    let mut complete = false;
                    tx.send(page.map(|p| {
                        if !p.has_more {
                            complete = true;
                        }
                        p
                    }))?;
                    if complete {
                        return Ok(QueryResult::QueryComplete);
                    }
                }
                Ok(SessionEvent::QueryRevision { tx, rev: rev_id }) => {
                    tx.send(queries::query_revision(&op, &rev_id))?
                }
                Ok(SessionEvent::DescribeRevision { tx, mutation }) => {
                    tx.send(mutations::describe_revision(&op, mutation)?)?
                }
                Err(err) => return Err(anyhow!(err)),
            };
        }
    }
}
