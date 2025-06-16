use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
};

use anyhow::{Context, Result, anyhow};
use jj_cli::{config::ConfigEnv, ui::Ui};
use jj_lib::config::{ConfigNamePathBuf, ConfigSource};

use super::{
    Mutation, WorkerSession,
    gui_util::WorkspaceSession,
    queries::{self, QueryState},
};
use crate::{
    config::{GGSettings, read_config},
    handler, messages,
};

/// implemented by states of the event loop
pub trait Session {
    type Transition;
    fn handle_events(self, rx: &Receiver<SessionEvent>) -> Result<Self::Transition>;
}

/// messages sent to a worker from other threads. most come with a channel allowing a response
#[derive(Debug)]
pub enum SessionEvent {
    #[allow(dead_code)] // used by tests
    EndSession,
    OpenWorkspace {
        tx: Sender<Result<messages::RepoConfig>>,
        wd: Option<PathBuf>,
    },
    QueryRevision {
        tx: Sender<Result<messages::RevResult>>,
        id: messages::RevId,
    },
    QueryRemotes {
        tx: Sender<Result<Vec<String>>>,
        tracking_branch: Option<String>,
    },
    QueryLog {
        tx: Sender<Result<messages::LogPage>>,
        query: String,
    },
    QueryLogNextPage {
        tx: Sender<Result<messages::LogPage>>,
    },
    ExecuteSnapshot {
        tx: Sender<Option<messages::RepoStatus>>,
    },
    ExecuteMutation {
        tx: Sender<messages::MutationResult>,
        mutation: Box<dyn Mutation + Send + Sync>,
    },
    ReadConfigArray {
        tx: Sender<Result<Vec<String>>>,
        key: Vec<String>,
    },
    WriteConfigArray {
        scope: ConfigSource,
        key: Vec<String>,
        values: Vec<String>,
    },
}

/// transitions for a workspace session
pub enum WorkspaceResult {
    Reopen(Sender<Result<messages::RepoConfig>>, Option<PathBuf>), // workspace -> workspace
    SessionComplete,                                               // workspace -> worker
}

/// transition for a query session
pub struct QueryResult(SessionEvent, QueryState); // query -> workspace

/// event loop state for a workspace session
#[derive(Default)]
struct WorkspaceState {
    pub unhandled_event: Option<SessionEvent>,
    pub unpaged_query: Option<QueryState>,
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
                Ok(SessionEvent::ReadConfigArray { key, tx }) => {
                    let name: ConfigNamePathBuf = key.iter().collect();
                    if let Some(global_settings) = self.global_settings.as_ref() {
                        tx.send(
                            global_settings
                                .config()
                                .get_value_with(&name, |value| {
                                    value
                                        .as_array()
                                        .map(|values| {
                                            values
                                                .into_iter()
                                                .flat_map(|v| v.as_str().map(|s| s.to_string()))
                                                .collect::<Vec<String>>()
                                        })
                                        .ok_or(anyhow!("config value is not an array"))
                                })
                                .context("read config"),
                        )?;
                    } else {
                        tx.send(Err(anyhow!("global settings not found")))?;
                    }
                }
                Ok(SessionEvent::OpenWorkspace { mut tx, mut wd }) => loop {
                    let resolved_wd = match wd.clone().or(latest_wd) {
                        Some(wd) => wd,
                        None => match self.get_cwd() {
                            Ok(wd) => wd,
                            Err(err) => {
                                latest_wd = None;
                                tx.send(Ok(messages::RepoConfig::LoadError {
                                    absolute_path: PathBuf::new().into(),
                                    message: format!("{err:#}"),
                                }))?;
                                break;
                            }
                        },
                    };

                    let mut ws = match self.load_directory(&resolved_wd) {
                        Ok(ws) => ws,
                        Err(err) => {
                            latest_wd = None;
                            tx.send(Ok(messages::RepoConfig::LoadError {
                                absolute_path: resolved_wd.into(),
                                message: format!("{err:#}"),
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
                SessionEvent::QueryRevision { tx, id } => {
                    tx.send(queries::query_revision(&self, id))?
                }
                SessionEvent::QueryRemotes {
                    tx,
                    tracking_branch,
                } => tx.send(queries::query_remotes(&self, tracking_branch))?,
                SessionEvent::QueryLog {
                    tx,
                    query: revset_string,
                } => {
                    let log_page_size = self
                        .session
                        .force_log_page_size
                        .unwrap_or(self.data.settings.query_log_page_size());
                    handle_query(
                        &mut state,
                        &self,
                        tx,
                        rx,
                        Some(&revset_string),
                        Some(QueryState::new(log_page_size)),
                    )?;

                    self.session.latest_query = Some(revset_string);
                }
                SessionEvent::QueryLogNextPage { tx } => {
                    let revset_string = self.session.latest_query.as_deref();
                    handle_query(&mut state, &self, tx, rx, revset_string, None)?;
                }
                SessionEvent::ExecuteSnapshot { tx } => {
                    let updated_head = self.load_at_head()?; // alternatively, this could be folded into snapshot so that it's done by all mutations
                    if self.import_and_snapshot(false)? || updated_head {
                        tx.send(Some(self.format_status()))?;
                    } else {
                        tx.send(None)?;
                    }
                }
                SessionEvent::ExecuteMutation { tx, mutation } => {
                    let name = mutation.as_ref().describe();
                    match catch_unwind(AssertUnwindSafe(|| {
                        mutation.execute(&mut self).with_context(|| name.clone())
                    })) {
                        Ok(result) => {
                            tx.send(match result {
                                Ok(result) => result,
                                Err(err) => {
                                    log::error!("{err:?}");
                                    messages::MutationResult::InternalError {
                                        message: (&*format!("{err:?}")).into(),
                                    }
                                }
                            })?;
                        }
                        Err(panic) => {
                            let mut message = match panic.downcast::<&str>() {
                                Ok(v) => *v,
                                _ => "panic!()",
                            }
                            .to_owned();
                            message.insert_str(0, ": ");
                            message.insert_str(0, &name);
                            log::error!("{message}");
                            tx.send(messages::MutationResult::InternalError {
                                message: (&*message).into(),
                            })?;
                        }
                    }
                }
                SessionEvent::ReadConfigArray { key, tx } => {
                    let name: ConfigNamePathBuf = key.iter().collect();

                    tx.send(
                        self.data
                            .settings
                            .config()
                            .get_value_with(&name, |value| {
                                value
                                    .as_array()
                                    .map(|values| {
                                        values
                                            .into_iter()
                                            .flat_map(|v| v.as_str().map(|s| s.to_string()))
                                            .collect::<Vec<String>>()
                                    })
                                    .ok_or(anyhow!("config value is not an array"))
                            })
                            .context("read config"),
                    )?;
                }
                SessionEvent::WriteConfigArray { scope, key, values } => {
                    let name: ConfigNamePathBuf = key.iter().collect();
                    let config_env = ConfigEnv::from_environment(&Ui::null());
                    let path = match scope {
                        ConfigSource::User => config_env
                            .user_config_paths()
                            // TODO: If there are multiple config paths, is there
                            // a more intelligent way to pick one?
                            .next()
                            .ok_or_else(|| anyhow!("No user config path found to edit"))
                            .map(|p| p.to_path_buf()),
                        ConfigSource::Repo => Ok(self.workspace.repo_path().join("config.toml")),
                        _ => Err(anyhow!("Can't get path for config source {scope:?}")),
                    }
                    .and_then(|path| {
                        let toml_array: toml_edit::Value =
                            toml_edit::Value::Array(values.iter().collect());
                        let mut file = jj_lib::config::ConfigFile::load_or_empty(scope, &path)?;
                        file.set_value(&name, toml_array)?;
                        file.save()?;
                        Ok(())
                    });

                    handler::optional!(path);

                    (self.data.settings, self.data.aliases_map) =
                        read_config(Some(self.workspace.repo_path()))?;
                }
            };
        }
    }
}

impl Session for queries::QuerySession<'_, '_> {
    type Transition = QueryResult;

    fn handle_events(mut self, rx: &Receiver<SessionEvent>) -> Result<Self::Transition> {
        loop {
            let evt = rx.recv();
            log::debug!("LogQuery handling {evt:?}");
            match evt {
                Ok(SessionEvent::QueryRevision { tx, id }) => {
                    tx.send(queries::query_revision(self.ws, id))?
                }
                Ok(SessionEvent::QueryRemotes {
                    tx,
                    tracking_branch,
                }) => tx.send(queries::query_remotes(self.ws, tracking_branch))?,
                Ok(SessionEvent::QueryLogNextPage { tx }) => tx.send(self.get_page())?,
                Ok(unhandled) => return Ok(QueryResult(unhandled, self.state)),
                Err(err) => return Err(anyhow!(err)),
            };
        }
    }
}

/// helper function for transitioning from workspace state to query state
fn handle_query(
    state: &mut WorkspaceState,
    ws: &WorkspaceSession,
    tx: Sender<Result<messages::LogPage>>,
    rx: &Receiver<SessionEvent>,
    revset_str: Option<&str>,
    query_state: Option<QueryState>,
) -> Result<()> {
    let query_state = match query_state.or_else(|| state.unpaged_query.take()) {
        Some(x) => x,
        None => {
            tx.send(Err(anyhow!(
                "page requested without query in progress or new query"
            )))?;

            state.unhandled_event = None;
            state.unpaged_query = None;
            return Ok(());
        }
    };

    let revset_str = match revset_str {
        Some(x) => x,
        None => {
            tx.send(Err(anyhow!("page requested without query in progress")))?;

            state.unhandled_event = None;
            state.unpaged_query = None;
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

            state.unhandled_event = None;
            state.unpaged_query = None;
            return Ok(());
        }
    };

    let mut query = queries::QuerySession::new(ws, &*revset, query_state);
    let page = query.get_page();
    tx.send(page)?;

    let QueryResult(next_event, next_query) = query.handle_events(rx).context("LogQuery")?;

    state.unhandled_event = Some(next_event);
    state.unpaged_query = Some(next_query);
    Ok(())
}
