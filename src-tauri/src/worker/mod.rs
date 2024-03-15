//! Worker per window, owning repo data (jj-lib is not thread-safe)
//! The worker thread is a state machine, running different handle functions based on loaded data

use std::{
    any::type_name_of_val,
    fmt::Debug,
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
};

use anyhow::{anyhow, Context, Result};

use crate::messages;
use crate::{
    gui_util::{WorkerSession, WorkspaceSession},
    messages::LogPage,
};

use self::queries::LogQueryState;

pub mod mutations;
pub mod queries;

#[derive(Debug)]
pub enum SessionEvent {
    #[allow(dead_code)]
    EndSession,
    OpenWorkspace {
        tx: Sender<Result<messages::RepoConfig>>,
        wd: Option<PathBuf>,
    },
    QueryLog {
        tx: Sender<Result<messages::LogPage>>,
        query: String,
    },
    QueryLogNextPage {
        tx: Sender<Result<messages::LogPage>>,
    },
    QueryRevision {
        tx: Sender<Result<messages::RevResult>>,
        query: String,
    },
    ExecuteSnapshot {
        tx: Sender<Option<messages::RepoStatus>>,
    },
    ExecuteMutation {
        tx: Sender<messages::MutationResult>,
        mutation: Box<dyn Mutation + Send + Sync>,
    },
}

pub trait Mutation: Debug {
    fn execute(self: Box<Self>, ws: &mut WorkspaceSession) -> Result<messages::MutationResult>;

    fn execute_unboxed(self, ws: &mut WorkspaceSession) -> Result<messages::MutationResult>
    where
        Self: Sized,
    {
        Box::new(self).execute(ws)
    }
}

pub trait Session {
    type Transition;
    fn handle_events(self, rx: &Receiver<SessionEvent>) -> Result<Self::Transition>;
}

impl Session for WorkerSession {
    type Transition = ();

    fn handle_events(mut self, rx: &Receiver<SessionEvent>) -> Result<()> {
        let mut latest_wd: Option<PathBuf> = None;

        loop {
            let evt = rx.recv();
            log::debug!("WorkerSession handling {evt:?}");
            match evt {
                Ok(SessionEvent::EndSession) => return Ok(()),
                Ok(SessionEvent::ExecuteSnapshot { .. }) => (),
                Ok(SessionEvent::OpenWorkspace { mut tx, mut wd }) => loop {
                    let resolved_wd = match wd.clone().or(latest_wd) {
                        Some(wd) => wd,
                        None => match std::env::current_dir().context("current_dir") {
                            Ok(wd) => wd,
                            Err(err) => {
                                latest_wd = None;
                                tx.send(Ok(messages::RepoConfig::NoWorkspace {
                                    absolute_path: PathBuf::new().into(),
                                    error: format!("{err:#}"),
                                }))?;
                                break;
                            }
                        },
                    };

                    let mut ws = match self.load_directory(&resolved_wd) {
                        Ok(ws) => ws,
                        Err(err) => {
                            latest_wd = None;
                            tx.send(Ok(messages::RepoConfig::NoWorkspace {
                                absolute_path: resolved_wd.into(),
                                error: format!("{err:#}"),
                            }))?;
                            break;
                        }
                    };

                    latest_wd = Some(resolved_wd);

                    ws.import_and_snapshot(false)?;

                    tx.send(ws.format_config())?;

                    match ws.handle_events(rx).context("WorkspaceSession")? {
                        WorkspaceResult::Reopen(new_tx, new_cwd) => (tx, wd) = (new_tx, new_cwd),
                        WorkspaceResult::SessionComplete => return Ok(()),
                    }
                },
                Ok(evt) => {
                    log::error!(
                        "WorkerSession::handle_events(): repo not loaded when receiving {evt:?}"
                    );
                    return Err(anyhow::anyhow!(
                        "A repo must be loaded before any other operations"
                    ));
                }
                Err(err) => {
                    log::error!("WorkerSession::handle_events(): {err}");
                    return Err(anyhow!(err));
                }
            };
        }
    }
}

/// transition types for a WorkspaceSession
pub enum WorkspaceResult {
    Reopen(Sender<Result<messages::RepoConfig>>, Option<PathBuf>),
    SessionComplete,
}

/// event loop state for a WorkspaceSession
#[derive(Default)]
struct WorkspaceState {
    pub unhandled_event: Option<SessionEvent>,
    pub unpaged_query: Option<LogQueryState>,
}

impl WorkspaceState {
    pub fn handle_query(
        &mut self,
        ws: &WorkspaceSession,
        tx: Sender<Result<LogPage>>,
        rx: &Receiver<SessionEvent>,
        revset_str: Option<&str>,
        query_state: Option<LogQueryState>,
    ) -> Result<()> {
        let query_state = match query_state.or_else(|| self.unpaged_query.take()) {
            Some(x) => x,
            None => {
                tx.send(Err(anyhow!(
                    "page requested without query in progress or new query"
                )))?;

                self.unhandled_event = None;
                self.unpaged_query = None;
                return Ok(());
            }
        };

        let revset_str = match revset_str {
            Some(x) => x,
            None => {
                tx.send(Err(anyhow!("page requested without query in progress")))?;

                self.unhandled_event = None;
                self.unpaged_query = None;
                return Ok(());
            }
        };

        let revset = match ws
            .evaluate_revset_str(revset_str)
            .context("evaluate revset")
        {
            Ok(x) => x,
            Err(err) => {
                tx.send(Err(err))?;

                self.unhandled_event = None;
                self.unpaged_query = None;
                return Ok(());
            }
        };

        let mut query = queries::LogQuery::new(ws, &*revset, query_state);
        let page = query.get_page();
        tx.send(page)?;

        let QueryResult(next_event, next_query) = query.handle_events(rx).context("LogQuery")?;

        self.unhandled_event = Some(next_event);
        self.unpaged_query = Some(next_query);
        Ok(())
    }
}

impl Session for WorkspaceSession<'_> {
    type Transition = WorkspaceResult;

    fn handle_events(mut self, rx: &Receiver<SessionEvent>) -> Result<WorkspaceResult> {
        let mut state = WorkspaceState::default();

        loop {
            let next_event = if state.unhandled_event.is_some() {
                state.unhandled_event.take().unwrap()
            } else {
                let evt = rx.recv();
                log::debug!("WorkspaceSession handling {evt:?}");
                evt?
            };

            match next_event {
                SessionEvent::EndSession => return Ok(WorkspaceResult::SessionComplete),
                SessionEvent::OpenWorkspace { tx, wd: cwd } => {
                    return Ok(WorkspaceResult::Reopen(tx, cwd));
                }
                SessionEvent::QueryRevision { tx, query } => {
                    tx.send(queries::query_revision(&self, &query))?
                }
                SessionEvent::QueryLog {
                    tx,
                    query: revset_string,
                } => {
                    state.handle_query(
                        &self,
                        tx,
                        rx,
                        Some(&revset_string),
                        Some(LogQueryState::new(self.session.log_page_size)),
                    )?;

                    self.session.latest_query = Some(revset_string);
                }
                SessionEvent::QueryLogNextPage { tx } => {
                    let revset_string = self.session.latest_query.as_ref().map(|x| x.as_str());

                    state.handle_query(&self, tx, rx, revset_string, None)?;
                }
                SessionEvent::ExecuteSnapshot { tx } => {
                    if self.import_and_snapshot(false).is_ok_and(|updated| updated) {
                        tx.send(Some(self.format_status()))?;
                    } else {
                        tx.send(None)?;
                    }
                }
                SessionEvent::ExecuteMutation { tx, mutation } => {
                    let name = type_name_of_val(mutation.as_ref());
                    match catch_unwind(AssertUnwindSafe(|| {
                        mutation.execute(&mut self).context(name)
                    })) {
                        Ok(result) => {
                            tx.send(match result {
                                Ok(result) => result,
                                Err(err) => messages::MutationResult::InternalError {
                                    message: err.to_string(),
                                },
                            })?;
                        }
                        Err(panic) => {
                            let mut message = match panic.downcast::<&str>() {
                                Ok(v) => *v,
                                _ => "panic!()",
                            }
                            .to_owned();
                            message.insert_str(0, ": ");
                            message.insert_str(0, name);
                            tx.send(messages::MutationResult::InternalError { message })?;
                        }
                    }
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
            let evt = rx.recv();
            log::debug!("LogQuery handling {evt:?}");
            match evt {
                Ok(SessionEvent::QueryRevision { tx, query }) => {
                    tx.send(queries::query_revision(&self.ws, &query))?
                }
                Ok(SessionEvent::QueryLogNextPage { tx }) => tx.send(self.get_page())?,
                Ok(unhandled) => return Ok(QueryResult(unhandled, self.state)),
                Err(err) => return Err(anyhow!(err)),
            };
        }
    }
}
