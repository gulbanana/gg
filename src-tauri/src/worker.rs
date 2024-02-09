use crate::{format::CommitOrChangeId, messages};
use anyhow::{anyhow, Result};
use itertools::Itertools;
use jj_cli::{
    cli_util::start_repo_transaction,
    config::{default_config, LayeredConfigs},
    time_util,
};
use jj_lib::{
    backend::CommitId,
    id_prefix::IdPrefixContext,
    op_heads_store,
    operation::Operation,
    repo::{ReadonlyRepo, Repo, RepoLoader, StoreFactories},
    revset::{
        self, Revset, RevsetAliasesMap, RevsetIteratorExt, RevsetParseContext,
        RevsetWorkspaceContext,
    },
    settings::{ConfigResultExt, UserSettings},
    workspace::{self, Workspace, WorkspaceLoader},
};
use std::{
    path::{Path, PathBuf},
    sync::{
        mpsc::{Receiver, Sender},
        Arc,
    },
};

#[derive(Debug)]
pub enum SessionEvent {
    SetCwd {
        tx: Sender<Result<()>>,
        cwd: PathBuf,
    },
    GetLog {
        tx: Sender<Result<Vec<messages::LogChange>>>,
    },
    GetChange {
        tx: Sender<Result<Vec<messages::ChangePath>>>,
        revision: String,
    },
}

pub fn main(rx: Receiver<SessionEvent>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let mut session = WorkspaceSession { cwd, data: None };

    loop {
        match rx.recv() {
            Err(err) => return Err(anyhow!(err)),
            Ok(SessionEvent::SetCwd { tx, cwd }) => tx.send(Ok(session.set_cwd(cwd)))?,
            Ok(SessionEvent::GetLog { tx }) => tx.send(session.get_log())?,
            Ok(SessionEvent::GetChange { tx, revision }) => {
                tx.send(session.get_change(revision))?
            }
        };
    }
}

struct WorkspaceData {
    settings: UserSettings,
    cwd: PathBuf,
    workspace: Workspace,
    repo: Arc<ReadonlyRepo>,
    aliases_map: RevsetAliasesMap,
    pub prefix_context: IdPrefixContext,
}

impl WorkspaceData {
    pub fn from_cwd(cwd: &Path) -> Result<WorkspaceData> {
        let loader = WorkspaceLoader::init(Self::find_workspace_dir(cwd))?;

        let mut configs = LayeredConfigs::from_environment(default_config());
        configs.read_user_config()?;
        configs.read_repo_config(loader.repo_path())?;
        let config = configs.merge();
        let settings = UserSettings::from_config(config);

        let workspace = loader.load(
            &settings,
            &StoreFactories::default(),
            &workspace::default_working_copy_factories(),
        )?;

        let op_head = resolve_op_head(&settings, workspace.repo_loader())?;
        let repo = workspace.repo_loader().load_at(&op_head)?;

        let aliases_map = load_revset_aliases(&configs)?;

        Ok(WorkspaceData {
            cwd: cwd.to_owned(),
            settings,
            workspace,
            repo,
            aliases_map,
            prefix_context: IdPrefixContext::default(), // XXX jj cli does some additional disambiguation
        })
    }

    pub fn parse_revset(&self, revision: &str) -> Result<Box<dyn Revset + '_>> {
        let expression: std::rc::Rc<revset::RevsetExpression> =
            revset::parse(revision, &self.revset_parse_context())?;
        let expression = revset::optimize(expression);
        let symbol_resolver = revset_symbol_resolver(&self.repo, &self.prefix_context)?;
        let resolved_expression =
            expression.resolve_user_expression(self.repo.as_ref(), &symbol_resolver)?;
        let revset = resolved_expression.evaluate(self.repo.as_ref())?;
        Ok(revset)
    }

    fn revset_parse_context(&self) -> RevsetParseContext<'_> {
        let workspace_context = RevsetWorkspaceContext {
            cwd: &self.cwd,
            workspace_id: self.workspace.workspace_id(),
            workspace_root: self.workspace.workspace_root(),
        };
        RevsetParseContext {
            aliases_map: &self.aliases_map,
            user_email: self.settings.user_email(),
            workspace: Some(workspace_context),
        }
    }

    fn find_workspace_dir(cwd: &Path) -> &Path {
        cwd.ancestors()
            .find(|path| path.join(".jj").is_dir())
            .unwrap_or(cwd)
    }
}

struct WorkspaceSession {
    cwd: PathBuf,
    data: Option<WorkspaceData>,
}

impl WorkspaceSession {
    pub fn set_cwd(&mut self, cwd: PathBuf) {
        self.cwd = cwd;
        self.data = None;
    }

    pub fn get_log(&mut self) -> Result<Vec<messages::LogChange>> {
        let data = self.lazy_load()?;

        let revset =
            data.parse_revset("@ | ancestors(immutable_heads().., 2) | heads(immutable_heads())")?;

        let mut output = Vec::new();
        for commit_or_error in revset.iter().commits(data.repo.store()) {
            let commit = commit_or_error?;
            let change_id = CommitOrChangeId::Change(commit.change_id().clone()).shortest(
                data.repo.as_ref(),
                &data.prefix_context,
                12,
            );
            let commit_id = CommitOrChangeId::Commit(commit.id().clone()).shortest(
                data.repo.as_ref(),
                &data.prefix_context,
                12,
            );
            output.push(messages::LogChange {
                change_id: messages::Id {
                    prefix: change_id.prefix,
                    rest: change_id.rest,
                },
                commit_id: messages::Id {
                    prefix: commit_id.prefix,
                    rest: commit_id.rest,
                },
                description: commit.description().into(),
                email: commit.author().email.clone(),
                timestamp: time_util::format_absolute_timestamp(&commit.author().timestamp),
            });
        }

        Ok(output)
    }

    pub fn get_change(&mut self, revision: String) -> Result<Vec<messages::ChangePath>> {
        let data = self.lazy_load()?;

        let revset = data.parse_revset(&revision)?;

        let commit = revset
            .iter()
            .commits(data.repo.store())
            .next()
            .ok_or(anyhow!("commit not found"))??;

        Ok(vec![messages::ChangePath {
            relative_path: commit.description().into(),
        }])
    }

    fn lazy_load(&mut self) -> Result<&mut WorkspaceData> {
        try_get_or_insert_with(&mut self.data, || WorkspaceData::from_cwd(&self.cwd))
    }
}

fn load_revset_aliases(layered_configs: &LayeredConfigs) -> Result<RevsetAliasesMap> {
    const TABLE_KEY: &str = "revset-aliases";
    let mut aliases_map = RevsetAliasesMap::new();
    // Load from all config layers in order. 'f(x)' in default layer should be
    // overridden by 'f(a)' in user.
    for (_, config) in layered_configs.sources() {
        let table = if let Some(table) = config.get_table(TABLE_KEY).optional()? {
            table
        } else {
            continue;
        };
        for (decl, value) in table.into_iter().sorted_by(|a, b| a.0.cmp(&b.0)) {
            value
                .into_string()
                .map_err(|e| anyhow!(e))
                .and_then(|v| aliases_map.insert(&decl, v).map_err(|e| anyhow!(e)))?;
        }
    }
    Ok(aliases_map)
}

fn resolve_op_head(settings: &UserSettings, repo_loader: &RepoLoader) -> Result<Operation> {
    op_heads_store::resolve_op_heads(
        repo_loader.op_heads_store().as_ref(),
        repo_loader.op_store(),
        |op_heads| {
            let base_repo = repo_loader.load_at(&op_heads[0])?;
            let mut tx = start_repo_transaction(&base_repo, &settings, &vec![]);
            for other_op_head in op_heads.into_iter().skip(1) {
                tx.merge_operation(other_op_head)?;
                let _num_rebased = tx.mut_repo().rebase_descendants(&settings)?;
            }
            Ok(tx
                .write("resolve concurrent operations")
                .leave_unpublished()
                .operation()
                .clone())
        },
    )
}

fn revset_symbol_resolver<'context>(
    repo: &'context Arc<ReadonlyRepo>,
    id_prefix_context: &'context IdPrefixContext,
) -> Result<revset::DefaultSymbolResolver<'context>> {
    let commit_id_resolver: revset::PrefixResolver<CommitId> =
        Box::new(|repo, prefix| id_prefix_context.resolve_commit_prefix(repo, prefix));
    let change_id_resolver: revset::PrefixResolver<Vec<CommitId>> =
        Box::new(|repo, prefix| id_prefix_context.resolve_change_prefix(repo, prefix));
    let symbol_resolver = revset::DefaultSymbolResolver::new(repo.as_ref())
        .with_commit_id_resolver(commit_id_resolver)
        .with_change_id_resolver(change_id_resolver);
    Ok(symbol_resolver)
}

fn try_get_or_insert_with<T, E, F>(option: &mut Option<T>, f: F) -> Result<&mut T, E>
where
    F: FnOnce() -> Result<T, E>,
{
    match option {
        Some(value) => Ok(value),
        None => Ok(option.get_or_insert(f()?)),
    }
}
