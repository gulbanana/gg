//! Analogous to cli_util from jj-cli
//! We reuse a bit of jj-cli code, but most of its types include TUI concerns or are not suitable for a long-running server

use std::{cell::OnceCell, collections::HashMap, path::Path, rc::Rc, sync::Arc};

use anyhow::{anyhow, Context, Result};
use itertools::Itertools;
use jj_cli::{
    config::{default_config, LayeredConfigs},
    git_util::is_colocated_git_workspace,
};
use jj_lib::{
    backend::{ChangeId, CommitId},
    commit::Commit,
    git,
    git_backend::GitBackend,
    hex_util::to_reverse_hex,
    id_prefix::IdPrefixContext,
    object_id::ObjectId,
    op_heads_store,
    operation::Operation,
    repo::{ReadonlyRepo, Repo, StoreFactories},
    revset::{
        self, DefaultSymbolResolver, Revset, RevsetAliasesMap, RevsetExpression,
        RevsetParseContext, RevsetWorkspaceContext,
    },
    settings::{ConfigResultExt, UserSettings},
    transaction::Transaction,
    workspace::{self, Workspace, WorkspaceLoader},
};

use crate::messages::{self, MutationResult};

pub struct WorkspaceSession {
    pub settings: UserSettings,
    pub workspace: Workspace,
    aliases_map: RevsetAliasesMap,
}

pub struct SessionOperation<'a> {
    pub session: &'a WorkspaceSession,
    pub repo: Arc<ReadonlyRepo>,
    parse_context: RevsetParseContext<'a>,
    prefix_context: IdPrefixContext,
    working_copy_id: CommitId,
    branches_index: OnceCell<Rc<RefNamesIndex>>,
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

        let working_copy_id = repo
            .view()
            .get_wc_commit_id(session.workspace.workspace_id())
            .ok_or_else(|| anyhow!("No working copy found for workspace"))?
            .clone();

        Ok(SessionOperation {
            session,
            repo,
            parse_context,
            prefix_context,
            working_copy_id,
            branches_index: OnceCell::new(),
        })
    }

    pub fn branches_index(&self) -> &Rc<RefNamesIndex> {
        self.branches_index
            .get_or_init(|| Rc::new(self.build_branches_index()))
    }

    fn build_branches_index(&self) -> RefNamesIndex {
        let mut index = RefNamesIndex::default();
        for (branch_name, branch_target) in self.repo.view().branches() {
            let local_target = branch_target.local_target;
            let remote_refs = branch_target.remote_refs;
            if local_target.is_present() {
                let ref_name = messages::RefName {
                    name: branch_name.to_owned(),
                    remote: None,
                    has_conflict: local_target.has_conflict(),
                    is_synced: remote_refs.iter().all(|&(_, remote_ref)| {
                        !remote_ref.is_tracking() || remote_ref.target == *local_target
                    }),
                };
                index.insert(local_target.added_ids(), ref_name);
            }
            for &(remote_name, remote_ref) in &remote_refs {
                let ref_name = messages::RefName {
                    name: branch_name.to_owned(),
                    remote: Some(remote_name.to_owned()),
                    has_conflict: remote_ref.target.has_conflict(),
                    is_synced: remote_ref.is_tracking() && remote_ref.target == *local_target,
                };
                index.insert(remote_ref.target.added_ids(), ref_name);
            }
        }
        index
    }

    pub fn format_config(&self, default_revset: Option<String>) -> messages::RepoConfig {
        let default_query = self.session.settings.default_revset();

        messages::RepoConfig::Workspace {
            absolute_path: self.session.workspace.workspace_root().into(),
            status: self.format_status(),
            latest_query: default_revset.unwrap_or_else(|| default_query.clone()),
            default_query,
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
            working_copy: self.format_commit_id(&self.working_copy_id),
        }
    }

    pub fn format_header(&self, commit: &Commit) -> Result<messages::RevHeader> {
        let index = self.branches_index();
        let branches = index.get(commit.id()).iter().cloned().collect();

        Ok(messages::RevHeader {
            change_id: self.format_change_id(commit.change_id()),
            commit_id: self.format_commit_id(commit.id()),
            description: commit.description().into(),
            has_conflict: commit.has_conflict()?,
            is_working_copy: *commit.id() == self.working_copy_id,
            branches,
        })
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

    pub fn check_rewritable<'a>(&self, commits: impl IntoIterator<Item = &'a Commit>) -> bool {
        todo!()
    }

    pub fn start_transaction(&self) -> Transaction {
        self.repo.start_transaction(&self.session.settings)
    }

    pub fn finish_transaction(
        &self,
        mut tx: Transaction,
        description: impl Into<String>,
    ) -> Result<MutationResult> {
        if !tx.mut_repo().has_changes() {
            return Ok(MutationResult::Unchanged);
        }

        tx.mut_repo().rebase_descendants(&self.session.settings)?;

        let old_repo = tx.base_repo().clone();

        let maybe_old_wc_commit = old_repo
            .view()
            .get_wc_commit_id(self.session.workspace.workspace_id())
            .map(|commit_id| tx.base_repo().store().get_commit(commit_id))
            .transpose()?;
        let maybe_new_wc_commit = tx
            .repo()
            .view()
            .get_wc_commit_id(self.session.workspace.workspace_id())
            .map(|commit_id| tx.repo().store().get_commit(commit_id))
            .transpose()?;
        if is_colocated_git_workspace(&self.session.workspace, &self.repo) {
            let git_repo = self
                .repo
                .store()
                .backend_impl()
                .downcast_ref::<GitBackend>()
                .unwrap()
                .open_git_repo()?;
            if let Some(wc_commit) = &maybe_new_wc_commit {
                git::reset_head(tx.mut_repo(), &git_repo, wc_commit)?;
            }
            let failed_branches = git::export_refs(tx.mut_repo())?;
            //print_failed_git_export(ui, &failed_branches)?;
        }
        let new_repo = tx.commit(description);

        if true
        // self.loaded_at_head
        {
            if let Some(new_commit) = &maybe_new_wc_commit {
                self.update_working_copy(maybe_old_wc_commit.as_ref(), new_commit)?;
            }
        }

        Ok(MutationResult::Updated {
            new_status: todo!(),
        })
    }

    fn update_working_copy(
        &self,
        maybe_old_commit: Option<&Commit>,
        new_commit: &Commit,
    ) -> Result<()> {
        let old_tree_id = maybe_old_commit.map(|commit| commit.tree_id().clone());

        let stats = if Some(new_commit.tree_id()) != old_tree_id.as_ref() {
            let stats = self.session.workspace.check_out(
                self.repo.op_id().clone(),
                old_tree_id.as_ref(),
                new_commit,
            )?;
            Some(stats)
        } else {
            // Record new operation id which represents the latest working-copy state
            let locked_ws = self.session.workspace.start_working_copy_mutation()?;
            locked_ws.finish(self.repo.op_id().clone())?;
            None
        };

        Ok(())
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

/*************************/
/* from commit_templater */
/*************************/

#[derive(Default)]
pub struct RefNamesIndex {
    index: HashMap<CommitId, Vec<messages::RefName>>,
}

impl RefNamesIndex {
    fn insert<'a>(&mut self, ids: impl IntoIterator<Item = &'a CommitId>, name: messages::RefName) {
        for id in ids {
            let ref_names = self.index.entry(id.clone()).or_default();
            ref_names.push(name.clone());
        }
    }

    fn get(&self, id: &CommitId) -> &[messages::RefName] {
        if let Some(names) = self.index.get(id) {
            names
        } else {
            &[]
        }
    }
}
