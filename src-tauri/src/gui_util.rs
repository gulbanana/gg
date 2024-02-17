//! Analogous to cli_util from jj-cli
//! We reuse some jj-cli code, but many of its types include TUI concerns or are not suitable for a long-running server

use std::{path::Path, rc::Rc, sync::Arc};

use anyhow::{anyhow, Result};
use chrono::{DateTime, FixedOffset, Local, LocalResult, TimeZone, Utc};
use itertools::Itertools;
use jj_cli::{
    cli_util::start_repo_transaction,
    config::{default_config, LayeredConfigs},
};
use jj_lib::{
    backend::{CommitId, Timestamp},
    commit::Commit,
    id_prefix::IdPrefixContext,
    op_heads_store,
    operation::Operation,
    repo::{ReadonlyRepo, StoreFactories},
    revset::{
        self, DefaultSymbolResolver, Revset, RevsetAliasesMap, RevsetExpression,
        RevsetParseContext, RevsetWorkspaceContext,
    },
    settings::{ConfigResultExt, UserSettings},
    workspace::{self, Workspace, WorkspaceLoader},
};

use crate::{
    format::CommitOrChangeId,
    messages::{self},
};

pub struct WorkspaceSession {
    settings: UserSettings,
    aliases_map: RevsetAliasesMap,
    pub workspace: Workspace,
}

pub struct SessionOperation<'a> {
    pub session: &'a WorkspaceSession,
    pub repo: Arc<ReadonlyRepo>,
    parse_context: RevsetParseContext<'a>,
    prefix_context: IdPrefixContext,
}

impl WorkspaceSession {
    pub fn from_cwd(cwd: &Path) -> Result<WorkspaceSession> {
        let loader = WorkspaceLoader::init(find_workspace_dir(cwd))?;

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

        let aliases_map = load_revset_aliases(&configs)?;

        Ok(WorkspaceSession {
            settings,
            aliases_map,
            workspace,
        })
    }

    fn resolve_at_head(&self) -> Result<Operation> {
        let loader = self.workspace.repo_loader();

        op_heads_store::resolve_op_heads(
            loader.op_heads_store().as_ref(),
            loader.op_store(),
            |op_heads| {
                let base_repo = loader.load_at(&op_heads[0])?;
                let mut tx = start_repo_transaction(&base_repo, &self.settings, &vec![]);
                for other_op_head in op_heads.into_iter().skip(1) {
                    tx.merge_operation(other_op_head)?;
                    let _num_rebased = tx.mut_repo().rebase_descendants(&self.settings)?;
                }
                Ok(tx
                    .write("resolve concurrent operations")
                    .leave_unpublished()
                    .operation()
                    .clone())
            },
        )
    }

    pub fn load_at_head(&self) -> Result<SessionOperation> {
        let op_head = self.resolve_at_head()?;

        let repo: Arc<ReadonlyRepo> = self.workspace.repo_loader().load_at(&op_head)?;

        let parse_context: RevsetParseContext<'_> = {
            let workspace_context = RevsetWorkspaceContext {
                cwd: self.workspace.workspace_root(),
                workspace_id: self.workspace.workspace_id(),
                workspace_root: self.workspace.workspace_root(),
            };
            RevsetParseContext {
                aliases_map: &self.aliases_map,
                user_email: self.settings.user_email(),
                workspace: Some(workspace_context),
            }
        };

        let mut prefix_context: IdPrefixContext = IdPrefixContext::default();
        let revset_string: String = self
            .settings
            .config()
            .get_string("revsets.short-prefixes")
            .unwrap_or_else(|_| self.settings.default_revset());
        if !revset_string.is_empty() {
            let disambiguation_revset: Rc<RevsetExpression> =
                parse_revset(&parse_context, &revset_string)?;
            prefix_context = prefix_context.disambiguate_within(disambiguation_revset);
        };

        Ok(SessionOperation {
            session: self,
            repo,
            parse_context,
            prefix_context,
        })
    }
}

impl SessionOperation<'_> {
    pub fn evaluate_revset(&self, revset_str: &str) -> Result<Box<dyn Revset + '_>> {
        let symbol_resolver = self.create_symbol_resolver()?;
        let expression = parse_revset(&self.parse_context, revset_str)?;
        let resolved_expression =
            expression.resolve_user_expression(self.repo.as_ref(), &symbol_resolver)?;
        let revset = resolved_expression.evaluate(self.repo.as_ref())?;

        Ok(revset)
    }

    pub fn format_status(&self) -> messages::WSStatus {
        messages::WSStatus {
            root_path: self
                .session
                .workspace
                .workspace_root()
                .to_string_lossy()
                .into_owned(),
            operation_description: self
                .repo
                .operation()
                .store_operation()
                .metadata
                .description
                .clone(),
        }
    }

    pub fn format_rev_header(&self, commit: &Commit) -> messages::RevHeader {
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

    // XXX this is currently called on every eval
    fn create_symbol_resolver(&self) -> Result<DefaultSymbolResolver> {
        let commit_id_resolver: revset::PrefixResolver<CommitId> =
            Box::new(|repo, prefix| self.prefix_context.resolve_commit_prefix(repo, prefix));
        let change_id_resolver: revset::PrefixResolver<Vec<CommitId>> =
            Box::new(|repo, prefix| self.prefix_context.resolve_change_prefix(repo, prefix));
        let symbol_resolver = DefaultSymbolResolver::new(self.repo.as_ref())
            .with_commit_id_resolver(commit_id_resolver)
            .with_change_id_resolver(change_id_resolver);
        Ok(symbol_resolver)
    }
}

fn find_workspace_dir(cwd: &Path) -> &Path {
    cwd.ancestors()
        .find(|path| path.join(".jj").is_dir())
        .unwrap_or(cwd)
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

fn parse_revset(
    parse_context: &RevsetParseContext,
    revision: &str,
) -> Result<Rc<RevsetExpression>> {
    let expression = revset::parse(revision, parse_context)?;
    let expression = revset::optimize(expression);
    Ok(expression)
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
