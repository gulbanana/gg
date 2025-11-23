use std::{panic::AssertUnwindSafe, path::PathBuf};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use anyhow::{Context, Result, anyhow};
use futures_util::FutureExt;
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
    async fn handle_events(
        self,
        rx: &mut UnboundedReceiver<SessionEvent>,
    ) -> Result<Self::Transition>;
}

/// messages sent to a worker from other threads. most come with a channel allowing a response
#[derive(Debug)]
pub enum SessionEvent {
    #[allow(dead_code)] // used by tests
    EndSession,
    OpenWorkspace {
        tx: UnboundedSender<Result<messages::RepoConfig>>,
        wd: Option<PathBuf>,
    },
    QueryRevision {
        tx: UnboundedSender<Result<messages::RevResult>>,
        id: messages::RevId,
    },
    QueryRemotes {
        tx: UnboundedSender<Result<Vec<String>>>,
        tracking_branch: Option<String>,
    },
    QueryLog {
        tx: UnboundedSender<Result<messages::LogPage>>,
        query: String,
    },
    QueryLogNextPage {
        tx: UnboundedSender<Result<messages::LogPage>>,
    },
    ExecuteSnapshot {
        tx: UnboundedSender<Option<messages::RepoStatus>>,
    },
    ExecuteMutation {
        tx: UnboundedSender<messages::MutationResult>,
        mutation: Box<dyn Mutation + Send + Sync>,
    },
    ReadConfigArray {
        tx: UnboundedSender<Result<Vec<String>>>,
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
    Reopen(
        UnboundedSender<Result<messages::RepoConfig>>,
        Option<PathBuf>,
    ), // workspace -> workspace
    SessionComplete, // workspace -> worker
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

    async fn handle_events(mut self, rx: &mut UnboundedReceiver<SessionEvent>) -> Result<()> {
        let mut latest_wd: Option<PathBuf> = None;

        loop {
            let evt = rx.recv().await;
            log::debug!("WorkerSession handling {evt:?}");
            match evt {
                Some(SessionEvent::EndSession) => return Ok(()),
                Some(SessionEvent::ExecuteSnapshot { .. }) => (),
                Some(SessionEvent::ReadConfigArray { key, tx }) => {
                    let name: ConfigNamePathBuf = key.iter().collect();
                    if let Some(global_settings) = self.user_settings.as_ref() {
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
                Some(SessionEvent::OpenWorkspace { mut tx, mut wd }) => loop {
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

                    match ws.handle_events(rx).await.context("WorkspaceSession")? {
                        WorkspaceResult::Reopen(new_tx, new_cwd) => (tx, wd) = (new_tx, new_cwd),
                        WorkspaceResult::SessionComplete => return Ok(()),
                    }
                },
                Some(evt) => {
                    log::error!(
                        "WorkerSession::handle_events(): repo not loaded when receiving {evt:?}"
                    );
                    return Err(anyhow::anyhow!(
                        "A repo must be loaded before any other operations"
                    ));
                }
                None => {
                    log::error!("WorkerSession::handle_events(): channel closed");
                    return Err(anyhow!("channel closed"));
                }
            };
        }
    }
}

impl Session for WorkspaceSession<'_> {
    type Transition = WorkspaceResult;

    async fn handle_events(
        mut self,
        rx: &mut UnboundedReceiver<SessionEvent>,
    ) -> Result<WorkspaceResult> {
        let mut state = WorkspaceState::default();

        loop {
            let next_event = if state.unhandled_event.is_some() {
                state.unhandled_event.take().unwrap()
            } else {
                let evt = rx.recv().await;
                log::debug!("WorkspaceSession handling {evt:?}");
                evt.ok_or_else(|| anyhow!("channel closed"))?
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
                        .unwrap_or(self.data.workspace_settings.query_log_page_size());
                    handle_query(
                        &mut state,
                        &self,
                        tx,
                        rx,
                        Some(&revset_string),
                        Some(QueryState::new(log_page_size)),
                    )
                    .await?;

                    self.session.latest_query = Some(revset_string);
                }
                SessionEvent::QueryLogNextPage { tx } => {
                    let revset_string = self.session.latest_query.as_deref();
                    handle_query(&mut state, &self, tx, rx, revset_string, None).await?;
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
                    match AssertUnwindSafe(async {
                        mutation
                            .execute(&mut self)
                            .await
                            .with_context(|| name.clone())
                    })
                    .catch_unwind()
                    .await
                    {
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
                            .workspace_settings
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

                    (self.data.workspace_settings, self.data.aliases_map) =
                        read_config(Some(self.workspace.repo_path()))?;
                }
            };
        }
    }
}

impl Session for queries::QuerySession<'_, '_> {
    type Transition = QueryResult;

    async fn handle_events(
        mut self,
        rx: &mut UnboundedReceiver<SessionEvent>,
    ) -> Result<Self::Transition> {
        loop {
            let evt = rx.recv().await;
            log::debug!("LogQuery handling {evt:?}");
            match evt {
                Some(SessionEvent::QueryRevision { tx, id }) => {
                    tx.send(queries::query_revision(self.ws, id))?
                }
                Some(SessionEvent::QueryRemotes {
                    tx,
                    tracking_branch,
                }) => tx.send(queries::query_remotes(self.ws, tracking_branch))?,
                Some(SessionEvent::QueryLogNextPage { tx }) => tx.send(self.get_page())?,
                Some(unhandled) => return Ok(QueryResult(unhandled, self.state)),
                None => return Err(anyhow!("channel closed")),
            };
        }
    }
}

/// helper function for transitioning from workspace state to query state
async fn handle_query(
    state: &mut WorkspaceState,
    ws: &WorkspaceSession<'_>,
    tx: UnboundedSender<Result<messages::LogPage>>,
    rx: &mut UnboundedReceiver<SessionEvent>,
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

    let QueryResult(next_event, next_query) = query.handle_events(rx).await.context("LogQuery")?;

    state.unhandled_event = Some(next_event);
    state.unpaged_query = Some(next_query);
    Ok(())
}
