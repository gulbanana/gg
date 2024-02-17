//! Analogous to cli_util from jj-cli
//! We reuse a bit of jj-cli code, but most of its types include TUI concerns or are not suitable for a long-running server

use std::{path::Path, rc::Rc, sync::Arc};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, FixedOffset, Local, LocalResult, TimeZone, Utc};
use itertools::Itertools;
use jj_cli::config::{default_config, LayeredConfigs};
use jj_lib::{
    backend::{ChangeId, CommitId, Timestamp},
    commit::Commit,
    hex_util::to_reverse_hex,
    id_prefix::IdPrefixContext,
    object_id::ObjectId,
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

use crate::messages;

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

pub struct SessionEvaluator<'a> {
    repo: &'a ReadonlyRepo,
    parse_context: &'a RevsetParseContext<'a>,
    resolver: DefaultSymbolResolver<'a>,
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

    fn load_head(&self) -> Result<Operation> {
        let loader = self.workspace.repo_loader();

        op_heads_store::resolve_op_heads(
            loader.op_heads_store().as_ref(),
            loader.op_store(),
            |op_heads| {
                let base_repo = loader.load_at(&op_heads[0])?;
                // might want to set some tags
                let mut tx = base_repo.start_transaction(&self.settings);
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
}

impl SessionOperation<'_> {
    pub fn from_head(session: &WorkspaceSession) -> Result<SessionOperation> {
        let op_head = session.load_head()?;

        let repo: Arc<ReadonlyRepo> = session
            .workspace
            .repo_loader()
            .load_at(&op_head)
            .context("load op head")?;

        let parse_context: RevsetParseContext<'_> = {
            let workspace_context = RevsetWorkspaceContext {
                cwd: session.workspace.workspace_root(),
                workspace_id: session.workspace.workspace_id(),
                workspace_root: session.workspace.workspace_root(),
            };
            RevsetParseContext {
                aliases_map: &session.aliases_map,
                user_email: session.settings.user_email(),
                workspace: Some(workspace_context),
            }
        };

        let mut prefix_context: IdPrefixContext = IdPrefixContext::default();
        let revset_string: String = session
            .settings
            .config()
            .get_string("revsets.short-prefixes")
            .unwrap_or_else(|_| session.settings.default_revset());
        if !revset_string.is_empty() {
            let disambiguation_revset: Rc<RevsetExpression> =
                parse_revset(&parse_context, &revset_string)?;
            prefix_context = prefix_context.disambiguate_within(disambiguation_revset);
        };

        Ok(SessionOperation {
            session,
            repo,
            parse_context,
            prefix_context,
        })
    }

    pub fn format_config(&self) -> messages::RepoConfig {
        messages::RepoConfig {
            absolute_path: self.session.workspace.workspace_root().into(),
            default_revset: self.session.settings.default_revset(),
            status: self.format_status(),
        }
    }

    pub fn format_status(&self) -> messages::RepoStatus {
        messages::RepoStatus {
            operation_description: self
                .repo
                .operation()
                .store_operation()
                .metadata
                .description
                .clone(),
            working_copy: self.format_commit_id(
                self.repo
                    .view()
                    .get_wc_commit_id(self.session.workspace.workspace_id())
                    .expect("working copy not found for workspace"),
            ),
        }
    }

    pub fn format_header(&self, commit: &Commit) -> messages::RevHeader {
        let timestamp = datetime_from_timestamp(&commit.author().timestamp)
            .unwrap()
            .with_timezone(&Local);

        messages::RevHeader {
            change_id: self.format_change_id(commit.change_id()),
            commit_id: self.format_commit_id(commit.id()),
            description: commit.description().into(),
            author: commit.author().name.clone(),
            email: commit.author().email.clone(),
            timestamp,
        }
    }

    pub fn format_commit_id(&self, id: &CommitId) -> messages::RevId {
        let mut hex = id.hex();
        let prefix_len = self
            .prefix_context
            .shortest_commit_prefix_len(self.repo.as_ref(), id);
        let rest = hex.split_off(prefix_len);
        messages::RevId { prefix: hex, rest }
    }

    fn format_change_id(&self, id: &ChangeId) -> messages::RevId {
        let mut hex = to_reverse_hex(&id.hex()).expect("format change id as reverse hex");
        let prefix_len = self
            .prefix_context
            .shortest_change_prefix_len(self.repo.as_ref(), id);
        let rest = hex.split_off(prefix_len);
        messages::RevId { prefix: hex, rest }
    }
}

impl SessionEvaluator<'_> {
    pub fn from_operation<'a>(operation: &'a SessionOperation) -> SessionEvaluator<'a> {
        let commit_id_resolver: revset::PrefixResolver<CommitId> =
            Box::new(|repo, prefix| operation.prefix_context.resolve_commit_prefix(repo, prefix));
        let change_id_resolver: revset::PrefixResolver<Vec<CommitId>> =
            Box::new(|repo, prefix| operation.prefix_context.resolve_change_prefix(repo, prefix));
        let symbol_resolver = DefaultSymbolResolver::new(operation.repo.as_ref())
            .with_commit_id_resolver(commit_id_resolver)
            .with_change_id_resolver(change_id_resolver);

        SessionEvaluator {
            repo: &operation.repo.as_ref(),
            parse_context: &operation.parse_context,
            resolver: symbol_resolver,
        }
    }

    pub fn evaluate_revset(&self, revset_str: &str) -> Result<Box<dyn Revset + '_>> {
        let expression = parse_revset(self.parse_context, revset_str)?;
        let resolved_expression = expression.resolve_user_expression(self.repo, &self.resolver)?;
        let revset = resolved_expression.evaluate(self.repo)?;

        Ok(revset)
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
    let expression = revset::parse(revision, parse_context).context("parse revset")?;
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
