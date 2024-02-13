//! Duplicates some features of cli_util which are coupled to the TUI

use crate::{
    format::CommitOrChangeId,
    messages::{self, DiffPath, RevDetail},
};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, FixedOffset, Local, LocalResult, TimeZone, Utc};
use futures_util::StreamExt;
use itertools::Itertools;
use jj_cli::{
    cli_util::start_repo_transaction,
    config::{default_config, LayeredConfigs},
};
use jj_lib::{
    backend::{CommitId, Timestamp},
    commit::Commit,
    file_util,
    id_prefix::IdPrefixContext,
    matchers::EverythingMatcher,
    op_heads_store,
    operation::Operation,
    repo::{ReadonlyRepo, Repo, RepoLoader, StoreFactories},
    revset::{
        self, Revset, RevsetAliasesMap, RevsetIteratorExt, RevsetParseContext,
        RevsetWorkspaceContext,
    },
    rewrite::merge_commit_trees,
    settings::{ConfigResultExt, UserSettings},
    workspace::{self, Workspace, WorkspaceLoader},
};
use pollster::FutureExt;
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
        tx: Sender<Result<Vec<messages::RevHeader>>>,
    },
    GetChange {
        tx: Sender<Result<messages::RevDetail>>,
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
        let symbol_resolver = revset_symbol_resolver(&self.repo, &self.prefix_context)
            .context("revset_symbol_resolver")?;
        let resolved_expression =
            expression.resolve_user_expression(self.repo.as_ref(), &symbol_resolver)?;
        let revset = resolved_expression.evaluate(self.repo.as_ref())?;
        Ok(revset)
    }

    pub fn format_commit_header(&self, commit: &Commit) -> messages::RevHeader {
        let change_id = CommitOrChangeId::Change(commit.change_id().clone()).shortest(
            self.repo.as_ref(),
            &self.prefix_context,
            12,
        );

        let commit_id = CommitOrChangeId::Commit(commit.id().clone()).shortest(
            self.repo.as_ref(),
            &self.prefix_context,
            12,
        );

        let timestamp = datetime_from_timestamp(&commit.author().timestamp)
            .unwrap()
            .with_timezone(&Local);

        messages::RevHeader {
            change_id: messages::RevId {
                prefix: change_id.prefix,
                rest: change_id.rest,
            },
            commit_id: messages::RevId {
                prefix: commit_id.prefix,
                rest: commit_id.rest,
            },
            description: commit.description().into(),
            author: commit.author().name.clone(),
            email: commit.author().email.clone(),
            timestamp,
        }
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

    pub fn get_log(&mut self) -> Result<Vec<messages::RevHeader>> {
        let data = self.lazy_load()?;
        dbg!(&data.repo.operation().store_operation().metadata.description);

        let revset = data
            .parse_revset("..@ | ancestors(immutable_heads().., 2) | heads(immutable_heads())")
            .context("parse_revset")?;

        let mut output = Vec::new();
        for commit_or_error in revset.iter().commits(data.repo.store()) {
            let commit = commit_or_error?;
            output.push(data.format_commit_header(&commit));
        }

        Ok(output)
    }

    pub fn get_change(&mut self, revision: String) -> Result<messages::RevDetail> {
        let data = self.lazy_load()?;

        let revset = data.parse_revset(&revision).context("parse_revset")?;

        let commit = revset
            .iter()
            .commits(data.repo.store())
            .next()
            .ok_or(anyhow!("commit not found"))??;

        let parent_tree = merge_commit_trees(data.repo.as_ref(), &commit.parents())?;
        let tree = commit.tree()?;
        let mut tree_diff = parent_tree.diff_stream(&tree, &EverythingMatcher);

        let mut paths = Vec::new();
        async {
            while let Some((repo_path, diff)) = tree_diff.next().await {
                let base_path = data.workspace.workspace_root();
                let relative_path =
                    file_util::relative_path(base_path, &repo_path.to_fs_path(base_path))
                        .to_string_lossy()
                        .into_owned();
                let (before, after) = diff.unwrap();

                if before.is_present() && after.is_present() {
                    paths.push(DiffPath::Modified { relative_path });
                } else if before.is_absent() {
                    paths.push(DiffPath::Added { relative_path });
                } else {
                    paths.push(DiffPath::Deleted { relative_path });
                }
            }
        }
        .block_on();

        Ok(RevDetail {
            header: data.format_commit_header(&commit),
            diff: paths,
        })
    }

    fn lazy_load(&mut self) -> Result<&mut WorkspaceData> {
        let borrow = &mut self.data;
        match borrow {
            Some(value) => Ok(value),
            None => Ok(borrow.get_or_insert(WorkspaceData::from_cwd(&self.cwd)?)),
        }
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

// from time_util; not pub
fn datetime_from_timestamp(context: &Timestamp) -> Option<DateTime<FixedOffset>> {
    let utc = match Utc.timestamp_opt(
        context.timestamp.0.div_euclid(1000),
        (context.timestamp.0.rem_euclid(1000)) as u32 * 1000000,
    ) {
        LocalResult::None => {
            return None;
        }
        LocalResult::Single(x) => x,
        LocalResult::Ambiguous(y, _z) => y,
    };

    Some(
        utc.with_timezone(
            &FixedOffset::east_opt(context.tz_offset * 60)
                .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap()),
        ),
    )
}
