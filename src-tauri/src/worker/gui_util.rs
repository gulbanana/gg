//! Analogous to cli_util from jj-cli
//! We reuse a bit of jj-cli code, but many of its modules include TUI concerns or are not suitable for a long-running server

use std::{
    cell::OnceCell,
    collections::HashMap,
    env::VarError,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use anyhow::{anyhow, Context, Result};
use chrono::TimeZone;
use git2::Repository;
use itertools::Itertools;
use jj_cli::{
    cli_util::{check_stale_working_copy, short_operation_hash, WorkingCopyFreshness},
    git_util::{self, is_colocated_git_workspace},
    revset_util,
};
use jj_lib::{
    backend::{BackendError, ChangeId, CommitId},
    commit::Commit,
    default_index::{AsCompositeIndex, DefaultReadonlyIndex},
    file_util::relative_path,
    git,
    git_backend::GitBackend,
    gitignore::GitIgnoreFile,
    hex_util::to_reverse_hex,
    id_prefix::IdPrefixContext,
    object_id::ObjectId,
    op_heads_store,
    op_store::WorkspaceId,
    operation::Operation,
    repo::{ReadonlyRepo, Repo, RepoLoaderError, StoreFactories},
    repo_path::{RepoPath, RepoPathUiConverter},
    revset::{
        self, DefaultSymbolResolver, Revset, RevsetAliasesMap, RevsetEvaluationError,
        RevsetExpression, RevsetExtensions, RevsetIteratorExt, RevsetParseContext,
        RevsetResolutionError, RevsetWorkspaceContext, SymbolResolverExtension,
    },
    rewrite,
    settings::UserSettings,
    transaction::Transaction,
    view::View,
    working_copy::{CheckoutStats, SnapshotOptions},
    workspace::{self, Workspace, WorkspaceLoader},
};
use thiserror::Error;

use super::WorkerSession;
use crate::{
    config::{read_config, GGSettings},
    messages::{self, RevId},
};

/// jj-dependent state, available when a workspace is open
pub struct WorkspaceSession<'a> {
    pub(crate) session: &'a mut WorkerSession,

    // workspace-level data, initialised once
    workspace: Workspace,
    path_converter: RepoPathUiConverter,
    extensions: RevsetExtensions,
    is_large: bool,

    // reloadable data
    pub settings: UserSettings,
    pub aliases_map: RevsetAliasesMap,

    // operation-specific data, containing a repo view and derived extras
    operation: SessionOperation,
    is_colocated: bool,
}

/// state derived from a specific operation
pub struct SessionOperation {
    pub repo: Arc<ReadonlyRepo>,
    pub wc_id: CommitId,
    ref_index: OnceCell<Rc<RefIndex>>,
    prefix_context: OnceCell<Rc<IdPrefixContext>>,
}

#[derive(Debug, Error)]
pub enum RevsetError {
    #[error(transparent)]
    Resolution(#[from] RevsetResolutionError),
    #[error(transparent)]
    Evaluation(#[from] RevsetEvaluationError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<BackendError> for RevsetError {
    fn from(value: BackendError) -> Self {
        RevsetError::Other(anyhow!(value))
    }
}

impl WorkerSession {
    pub fn load_directory(&mut self, cwd: &Path) -> Result<WorkspaceSession> {
        let loader = WorkspaceLoader::init(find_workspace_dir(cwd))?;

        let (settings, aliases_map) = read_config(loader.repo_path())?;

        let workspace = loader.load(
            &settings,
            &StoreFactories::default(),
            &workspace::default_working_copy_factories(),
        )?;

        let operation = load_at_head(&settings, &workspace)?;

        let index_store = workspace.repo_loader().index_store();
        let index = index_store
            .get_index_at_op(&operation.repo.operation(), workspace.repo_loader().store())?;
        let is_large =
            if let Some(default_index) = index.as_any().downcast_ref::<DefaultReadonlyIndex>() {
                let stats = default_index.as_composite().stats();
                stats.num_commits as i64 >= settings.query_large_repo_heuristic()
            } else {
                true
            };

        let is_colocated = is_colocated_git_workspace(&workspace, &operation.repo);

        let path_converter = RepoPathUiConverter::Fs {
            cwd: workspace.workspace_root().clone(),
            base: workspace.workspace_root().clone(),
        };

        Ok(WorkspaceSession {
            session: self,
            is_large,
            settings,
            workspace,
            path_converter,
            aliases_map,
            extensions: Default::default(),
            operation,
            is_colocated,
        })
    }
}

impl WorkspaceSession<'_> {
    pub fn id(&self) -> &WorkspaceId {
        &self.workspace.workspace_id()
    }

    pub fn wc_id(&self) -> &CommitId {
        &self.operation.wc_id
    }

    // XXX maybe: hunt down uses and make nonpub
    pub fn repo(&self) -> &ReadonlyRepo {
        self.operation.repo.as_ref()
    }

    pub fn view(&self) -> &View {
        self.operation.repo.view()
    }

    pub fn get_commit(&self, id: &CommitId) -> Result<Commit> {
        Ok(self.operation.repo.store().get_commit(&id)?)
    }

    pub fn git_repo(&self) -> Result<Option<Repository>> {
        match self.operation.git_backend() {
            Some(backend) => Ok(Some(backend.open_git_repo()?)),
            None => Ok(None),
        }
    }

    pub fn load_at_head(&mut self) -> Result<bool> {
        let head = load_at_head(&self.settings, &self.workspace)?;
        if head.repo.op_id() != self.operation.repo.op_id() {
            self.operation = head;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /***********************************************************/
    /* Functions for evaluating revset expressions             */
    /* unfortunately parse_context and resolver are not cached */
    /***********************************************************/

    pub fn evaluate_revset_expr<'op>(
        &'op self,
        revset_expr: Rc<RevsetExpression>,
    ) -> Result<Box<dyn Revset + 'op>, RevsetError> {
        let resolved_expression =
            revset_expr.resolve_user_expression(self.operation.repo.as_ref(), &self.resolver())?;
        let revset = resolved_expression.evaluate(self.operation.repo.as_ref())?;
        Ok(revset)
    }

    pub fn evaluate_revset_str<'op>(
        &'op self,
        revset_str: &str,
    ) -> Result<Box<dyn Revset + 'op>, RevsetError> {
        let revset_expr = parse_revset(&self.parse_context(), revset_str)?;
        self.evaluate_revset_expr(revset_expr)
    }

    pub fn evaluate_revset_commits<'op>(
        &'op self,
        ids: &[messages::CommitId],
    ) -> Result<Box<dyn Revset + 'op>, RevsetError> {
        let expr = RevsetExpression::commits(
            ids.iter()
                .map(|id| CommitId::try_from_hex(id.hex.as_str()).expect("frontend-validated id"))
                .collect(),
        );
        self.evaluate_revset_expr(expr)
    }

    pub fn evaluate_revset_changes<'op>(
        &'op self,
        ids: &[messages::ChangeId],
    ) -> Result<Box<dyn Revset + 'op>, RevsetError> {
        let mut expr = RevsetExpression::none();
        for id in ids.iter() {
            expr = expr.union(&RevsetExpression::symbol(id.hex.clone()))
        }
        self.evaluate_revset_expr(expr)
    }

    pub fn evaluate_immutable(&self) -> Result<Box<dyn Revset + '_>> {
        let expr = revset_util::parse_immutable_expression(&self.parse_context())?;
        let revset = self.evaluate_revset_expr(expr)?;
        Ok(revset)
    }

    fn resolve_optional<'op, 'set: 'op, T: AsRef<dyn Revset + 'set>>(
        &'op self,
        revset: T,
    ) -> Result<Option<Commit>, RevsetError> {
        let mut iter = revset
            .as_ref()
            .iter()
            .commits(self.operation.repo.store())
            .fuse();
        match (iter.next(), iter.next()) {
            (Some(commit), None) => Ok(Some(commit?)),
            (None, _) => Ok(None),
            (Some(_), Some(_)) => Err(RevsetError::Other(anyhow!(
                r#"Revset "{:?}" resolved to more than one revision"#,
                revset.as_ref()
            ))),
        }
    }

    fn resolve_single<'op, 'set: 'op, T: AsRef<dyn Revset + 'set>>(
        &'op self,
        revset: T,
    ) -> Result<Commit, RevsetError> {
        match self.resolve_optional(revset)? {
            Some(commit) => Ok(commit),
            None => Err(RevsetError::Other(anyhow!(
                "Revset didn't resolve to any revisions"
            ))),
        }
    }

    // policy: some commands try to operate on a change in order to preserve visual identity, but
    // can fall back to operating on the commit described by the change at the time of the gesture
    pub fn resolve_optional_id(&self, id: &RevId) -> Result<Option<Commit>, RevsetError> {
        let change_revset = match self.evaluate_revset_str(&id.change.hex) {
            Ok(revset) => revset,
            Err(RevsetError::Resolution(RevsetResolutionError::NoSuchRevision { .. })) => {
                return Ok(None)
            }
            Err(err) => return Err(err),
        };

        let mut change_iter = change_revset
            .as_ref()
            .iter()
            .commits(self.operation.repo.store())
            .fuse();
        match (change_iter.next(), change_iter.next()) {
            (Some(commit), None) => Ok(Some(commit?)),
            (None, _) => Ok(None),
            (Some(_), Some(_)) => {
                let commit_revset = self.evaluate_revset_commits(&[id.commit.clone()])?;
                let mut commit_iter = commit_revset
                    .as_ref()
                    .iter()
                    .commits(self.operation.repo.store())
                    .fuse();
                match commit_iter.next() {
                    Some(commit) => Ok(Some(commit?)),
                    None => Ok(None),
                }
            }
        }
    }

    // policy: most commands prefer to operate on a change and will fail if the change has been evolved; however,
    // if it's become divergent, they will fall back to the known commit so that divergences can be resolved
    pub fn resolve_single_change(&self, id: &RevId) -> Result<Commit, RevsetError> {
        let revset = self.evaluate_revset_str(&id.change.hex)?;
        let mut iter = revset
            .as_ref()
            .iter()
            .commits(self.operation.repo.store())
            .fuse();
        let optional_change = match (iter.next(), iter.next()) {
            (Some(commit), None) => Some(commit?),
            (None, _) => None,
            (Some(_), Some(_)) => Some(self.resolve_single_commit(&id.commit)?),
        };

        match optional_change {
            Some(commit) => {
                let resolved_id = commit.id();
                if resolved_id == self.wc_id() || resolved_id.hex().starts_with(&id.commit.prefix) {
                    Ok(commit)
                } else {
                    Err(RevsetError::Other(anyhow!(
                        r#""{}" didn't resolve to the expected commit {}"#,
                        id.change.prefix,
                        id.commit.prefix
                    )))
                }
            }
            None => Err(RevsetError::Other(anyhow!(
                r#""{}" didn't resolve to any revisions"#,
                id.change.prefix
            ))),
        }
    }

    // not-really-policy: sometimes we only have a commit, not a change. this is a compromise and will ideally be eliminated
    pub fn resolve_single_commit(&self, id: &messages::CommitId) -> Result<Commit, RevsetError> {
        let expr = RevsetExpression::commit(
            CommitId::try_from_hex(&id.hex).expect("frontend-validated id"),
        );
        let revset = self.evaluate_revset_expr(expr)?;
        self.resolve_single(revset)
    }

    pub fn resolve_multiple<'op, 'set: 'op, T: AsRef<dyn Revset + 'set>>(
        &'op self,
        revset: T,
    ) -> Result<Vec<Commit>, RevsetError> {
        let commits = revset
            .as_ref()
            .iter()
            .commits(self.operation.repo.store())
            .collect::<Result<Vec<Commit>, BackendError>>()?;
        Ok(commits)
    }

    pub fn resolve_multiple_commits(
        &self,
        ids: &[messages::CommitId],
    ) -> Result<Vec<Commit>, RevsetError> {
        let revset = self.evaluate_revset_commits(ids)?;
        let commits = self.resolve_multiple(revset)?;
        Ok(commits)
    }

    // XXX ideally this would apply the same policy as resolve_single_change
    pub fn resolve_multiple_changes(
        &self,
        ids: impl IntoIterator<Item = RevId>,
    ) -> Result<Vec<Commit>, RevsetError> {
        let revset =
            self.evaluate_revset_changes(&ids.into_iter().map(|id| id.change).collect_vec())?;
        let commits = self.resolve_multiple(revset)?;
        Ok(commits)
    }

    /*************************************************************
     * Functions for creating temporary per-request derived data *
     *************************************************************/

    fn parse_context(&self) -> RevsetParseContext {
        let workspace_context = RevsetWorkspaceContext {
            path_converter: &self.path_converter,
            workspace_id: self.workspace.workspace_id(),
        };
        let now = if let Some(timestamp) = self.settings.commit_timestamp() {
            chrono::Local
                .timestamp_millis_opt(timestamp.timestamp.0)
                .unwrap()
        } else {
            chrono::Local::now()
        };
        RevsetParseContext::new(
            &self.aliases_map,
            self.settings.user_email(),
            now.into(),
            &self.extensions,
            Some(workspace_context),
        )
    }

    fn prefix_context(&self) -> &Rc<IdPrefixContext> {
        self.operation.prefix_context.get_or_init(|| {
            let mut prefix_context = IdPrefixContext::default();

            let revset_string: String = self
                .settings
                .config()
                .get_string("revsets.short-prefixes")
                .unwrap_or_else(|_| self.settings.default_revset());

            if !revset_string.is_empty() {
                let disambiguation_revset: Rc<RevsetExpression> =
                    parse_revset(&self.parse_context(), &revset_string)
                        .expect("init prefix context: parse revsets.short-prefixes");
                prefix_context = prefix_context.disambiguate_within(disambiguation_revset);
            };

            Rc::new(prefix_context)
        })
    }

    fn resolver(&self) -> DefaultSymbolResolver {
        DefaultSymbolResolver::new(
            self.operation.repo.as_ref(),
            &([] as [Box<dyn SymbolResolverExtension>; 0]),
        )
        .with_id_prefix_context(&self.prefix_context())
    }

    pub fn ref_index(&self) -> &Rc<RefIndex> {
        self.operation
            .ref_index
            .get_or_init(|| Rc::new(build_ref_index(self.operation.repo.as_ref())))
    }

    /************************************
     * IPC-message formatting functions *
     ************************************/

    pub fn format_config(&self) -> Result<messages::RepoConfig> {
        let absolute_path = self.workspace.workspace_root().into();

        let git_remotes = match self.git_repo()? {
            Some(repo) => repo
                .remotes()?
                .iter()
                .flatten()
                .map(|s| s.to_owned())
                .collect(),
            None => vec![],
        };

        let default_query = self.settings.default_revset();

        let latest_query = self
            .session
            .latest_query
            .as_ref()
            .unwrap_or_else(|| &default_query)
            .clone();

        Ok(messages::RepoConfig::Workspace {
            absolute_path,
            git_remotes,
            default_query,
            latest_query,
            status: self.format_status(),
            theme_override: self.settings.ui_theme_override(),
            mark_unpushed_branches: self.settings.ui_mark_unpushed_branches(),
        })
    }

    pub fn format_status(&self) -> messages::RepoStatus {
        messages::RepoStatus {
            operation_description: self
                .operation
                .repo
                .operation()
                .store_operation()
                .metadata
                .description
                .clone(),
            working_copy: self.format_commit_id(&self.operation.wc_id),
        }
    }

    pub fn format_commit_id(&self, id: &CommitId) -> messages::CommitId {
        let prefix_len = self
            .prefix_context()
            .shortest_commit_prefix_len(self.operation.repo.as_ref(), id);

        let hex = id.hex();
        let mut prefix = hex.clone();
        let rest = prefix.split_off(prefix_len);
        messages::CommitId { hex, prefix, rest }
    }

    pub fn format_change_id(&self, id: &ChangeId) -> messages::ChangeId {
        let prefix_len = self
            .prefix_context()
            .shortest_change_prefix_len(self.operation.repo.as_ref(), id);

        let hex = to_reverse_hex(&id.hex()).expect("format change id as reverse hex");
        let mut prefix = hex.clone();
        let rest = prefix.split_off(prefix_len);
        messages::ChangeId { hex, prefix, rest }
    }

    pub fn format_id(&self, commit: &Commit) -> RevId {
        RevId {
            commit: self.format_commit_id(commit.id()),
            change: self.format_change_id(commit.change_id()),
        }
    }

    pub fn format_header(
        &self,
        commit: &Commit,
        known_immutable: Option<bool>,
    ) -> Result<messages::RevHeader> {
        let index = self.ref_index();
        let branches = index.get(commit.id()).iter().cloned().collect();

        let is_immutable = known_immutable
            .map(|x| Result::Ok(x))
            .unwrap_or_else(|| self.check_immutable(vec![commit.id().clone()]))?;

        Ok(messages::RevHeader {
            id: self.format_id(commit),
            description: commit.description().into(),
            author: commit.author().try_into()?,
            has_conflict: commit.has_conflict()?,
            is_working_copy: *commit.id() == self.operation.wc_id,
            is_immutable,
            refs: branches,
            parent_ids: commit
                .parent_ids()
                .iter()
                .map(|commit_id| self.format_commit_id(commit_id))
                .collect(),
        })
    }

    pub fn format_path<T: AsRef<RepoPath>>(&self, repo_path: T) -> messages::TreePath {
        let base_path = self.workspace.workspace_root();
        let relative_path = relative_path(base_path, &repo_path.as_ref().to_fs_path(base_path));
        messages::TreePath {
            repo_path: repo_path.as_ref().as_internal_file_string().to_owned(),
            relative_path: relative_path.into(),
        }
    }

    pub fn check_immutable(&self, ids: impl IntoIterator<Item = CommitId>) -> Result<bool> {
        let check_revset = RevsetExpression::commits(ids.into_iter().collect());

        let immutable_revset = revset_util::parse_immutable_expression(&self.parse_context())?;
        let intersection_revset = check_revset.intersection(&immutable_revset);

        // note: slow! jj has added a caching contains_fn to revsets, but this codepath is used in one-offs where
        // nothing is cached. this should be checked at some point to ensure we aren't calling it unnecessarily
        let immutable_revs = self.evaluate_revset_expr(intersection_revset)?;
        let first = immutable_revs.iter().next();

        Ok(first.is_some())
    }

    /*********************************************************************
     * Transaction functions - these are very similar to cli_util        *
     * Ideally in future the code can be extracted to not depend on TUI. *
     *********************************************************************/

    pub fn start_transaction(&mut self) -> Result<Transaction> {
        self.import_and_snapshot(true)?;
        Ok(self.operation.repo.start_transaction(&self.settings))
    }

    pub fn finish_transaction(
        &mut self,
        mut tx: Transaction,
        description: impl Into<String>,
    ) -> Result<Option<messages::RepoStatus>> {
        if !tx.mut_repo().has_changes() {
            return Ok(None);
        }

        tx.mut_repo().rebase_descendants(&self.settings)?;

        let old_repo = tx.base_repo().clone();

        let maybe_old_wc_commit = old_repo
            .view()
            .get_wc_commit_id(self.workspace.workspace_id())
            .map(|commit_id| tx.base_repo().store().get_commit(commit_id))
            .transpose()?;
        let maybe_new_wc_commit = tx
            .repo()
            .view()
            .get_wc_commit_id(self.workspace.workspace_id())
            .map(|commit_id| tx.repo().store().get_commit(commit_id))
            .transpose()?;
        if self.is_colocated {
            let git_repo = self
                .operation
                .git_backend()
                .ok_or(anyhow!("colocated, but git backend not found"))?
                .open_git_repo()?;
            if let Some(wc_commit) = &maybe_new_wc_commit {
                git::reset_head(tx.mut_repo(), &git_repo, wc_commit)?;
            }
            git::export_refs(tx.mut_repo())?;
        }

        self.operation =
            SessionOperation::new(tx.commit(description), self.workspace.workspace_id());

        // XXX do this only if loaded at head, which is currently always true, but won't be once we have undo-redo
        if let Some(new_commit) = &maybe_new_wc_commit {
            self.update_working_copy(maybe_old_wc_commit.as_ref(), new_commit)?;
        }

        Ok(Some(self.format_status()))
    }

    // XXX does this need to do any operation merging in case of other writers?
    pub fn import_and_snapshot(&mut self, force: bool) -> Result<bool> {
        if !(force
            || self
                .settings
                .query_auto_snapshot()
                .unwrap_or(!self.is_large))
        {
            return Ok(false);
        }

        if self.is_colocated {
            self.import_git_head()?;
        }

        let updated_working_copy = self.snapshot_working_copy()?;

        if self.is_colocated {
            self.import_git_refs()?;
        }

        Ok(updated_working_copy)
    }

    fn snapshot_working_copy(&mut self) -> Result<bool> {
        let workspace_id = self.workspace.workspace_id().to_owned();
        let get_wc_commit = |repo: &ReadonlyRepo| -> Result<Option<_>, _> {
            repo.view()
                .get_wc_commit_id(&workspace_id)
                .map(|id| repo.store().get_commit(id))
                .transpose()
        };
        let repo = self.operation.repo.clone();
        let Some(wc_commit) = get_wc_commit(&repo)? else {
            return Ok(false); // The workspace has been deleted
        };

        let base_ignores = self.operation.base_ignores()?;

        // Compare working-copy tree and operation with repo's, and reload as needed.
        let mut locked_ws = self.workspace.start_working_copy_mutation()?;
        let old_op_id = locked_ws.locked_wc().old_operation_id().clone();
        let (repo, wc_commit) = match check_stale_working_copy(
            locked_ws.locked_wc(),
            &wc_commit,
            &repo,
        )? {
            WorkingCopyFreshness::Fresh => (repo, wc_commit),
            WorkingCopyFreshness::Updated(wc_operation) => {
                let repo = repo.reload_at(&wc_operation)?;
                let wc_commit = if let Some(wc_commit) = get_wc_commit(&repo)? {
                    wc_commit
                } else {
                    return Ok(false);
                };
                (repo, wc_commit)
            }
            WorkingCopyFreshness::WorkingCopyStale => {
                return Err(anyhow!(
                    "The working copy is stale (not updated since operation {}). Run `jj workspace update-stale` to update it.",
                    short_operation_hash(&old_op_id)
                ));
            }
            WorkingCopyFreshness::SiblingOperation => {
                return Err(anyhow!(
                    "The repo was loaded at operation {}, which seems to be a sibling of the working copy's operation {}",
                    short_operation_hash(repo.op_id()),
                    short_operation_hash(&old_op_id)
                ));
            }
        };

        let new_tree_id = locked_ws.locked_wc().snapshot(SnapshotOptions {
            base_ignores,
            fsmonitor_settings: self.settings.fsmonitor_settings()?,
            progress: None,
            max_new_file_size: self.settings.max_new_file_size()?,
        })?;

        let did_anything = new_tree_id != *wc_commit.tree_id();

        if did_anything {
            let mut tx = repo.start_transaction(&self.settings);
            let mut_repo = tx.mut_repo();
            let commit = mut_repo
                .rewrite_commit(&self.settings, &wc_commit)
                .set_tree_id(new_tree_id)
                .write()?;
            mut_repo.set_wc_commit(workspace_id.clone(), commit.id().clone())?;

            mut_repo.rebase_descendants(&self.settings)?;

            if self.is_colocated {
                git::export_refs(mut_repo)?;
            }

            self.operation =
                SessionOperation::new(tx.commit("snapshot working copy"), &workspace_id);
        }

        locked_ws.finish(self.operation.repo.op_id().clone())?;

        Ok(did_anything)
    }

    fn update_working_copy(
        &mut self,
        maybe_old_commit: Option<&Commit>,
        new_commit: &Commit,
    ) -> Result<Option<CheckoutStats>> {
        let old_tree_id = maybe_old_commit.map(|commit| commit.tree_id().clone());

        Ok(if Some(new_commit.tree_id()) != old_tree_id.as_ref() {
            Some(self.workspace.check_out(
                self.operation.repo.op_id().clone(),
                old_tree_id.as_ref(),
                new_commit,
            )?)
        } else {
            let locked_ws = self.workspace.start_working_copy_mutation()?;
            locked_ws.finish(self.operation.repo.op_id().clone())?;
            None
        })
    }

    fn import_git_head(&mut self) -> Result<()> {
        let mut tx = self.operation.repo.start_transaction(&self.settings);
        git::import_head(tx.mut_repo())?;
        if !tx.mut_repo().has_changes() {
            return Ok(());
        }

        let new_git_head = tx.mut_repo().view().git_head().clone();
        if let Some(new_git_head_id) = new_git_head.as_normal() {
            let workspace_id = self.workspace.workspace_id().to_owned();

            if let Some(old_wc_commit_id) =
                self.operation.repo.view().get_wc_commit_id(&workspace_id)
            {
                tx.mut_repo()
                    .record_abandoned_commit(old_wc_commit_id.clone());
            }

            let new_git_head_commit = tx.mut_repo().store().get_commit(new_git_head_id)?;
            tx.mut_repo()
                .check_out(workspace_id.clone(), &self.settings, &new_git_head_commit)?;

            let mut locked_ws = self.workspace.start_working_copy_mutation()?;

            locked_ws.locked_wc().reset(&new_git_head_commit)?;
            tx.mut_repo().rebase_descendants(&self.settings)?;

            self.operation = SessionOperation::new(tx.commit("import git head"), &workspace_id);

            locked_ws.finish(self.operation.repo.op_id().clone())?;
        } else {
            self.finish_transaction(tx, "import git head")?;
        }
        Ok(())
    }

    fn import_git_refs(&mut self) -> Result<()> {
        let git_settings = self.settings.git_settings();
        let mut tx = self.operation.repo.start_transaction(&self.settings);
        // Automated import shouldn't fail because of reserved remote name.
        git::import_some_refs(tx.mut_repo(), &git_settings, |ref_name| {
            !git::is_reserved_git_remote_ref(ref_name)
        })?;
        if !tx.mut_repo().has_changes() {
            return Ok(());
        }

        tx.mut_repo().rebase_descendants(&self.settings)?;

        self.finish_transaction(tx, "import git refs")?;
        Ok(())
    }

    /*************************************************************************************************/
    /* Rebase functions - the idea is to have several composable rebase ops that use these utilities */
    /* arguably they should be in a Transaction-wrapper struct, but i'm not yet sure whether to      */
    /* complicate the interface of trait Mutation                                                    */
    /*************************************************************************************************/

    pub fn disinherit_children(
        &self,
        tx: &mut Transaction,
        target: &Commit,
    ) -> Result<HashMap<CommitId, CommitId>> {
        // find all children of target
        let children_expr = RevsetExpression::commit(target.id().clone()).children();
        let children: Vec<_> = children_expr
            .evaluate_programmatic(self.operation.repo.as_ref())?
            .iter()
            .commits(self.operation.repo.store())
            .try_collect()?;

        // rebase each child, and then auto-rebase their descendants
        let mut rebased_commit_ids = HashMap::new();
        for child_commit in children {
            let new_child_parent_ids = child_commit
                .parent_ids()
                .iter()
                .flat_map(|c| {
                    if c == target.id() {
                        target.parent_ids().to_vec()
                    } else {
                        vec![c.clone()]
                    }
                })
                .collect_vec();

            // some of the new parents may be ancestors of others
            let new_child_parents_expression =
                RevsetExpression::commits(new_child_parent_ids.clone()).minus(
                    &RevsetExpression::commits(new_child_parent_ids.clone())
                        .parents()
                        .ancestors(),
                );
            let new_child_parents: Vec<CommitId> = new_child_parents_expression
                .evaluate_programmatic(tx.base_repo().as_ref())?
                .iter()
                .collect();

            rebased_commit_ids.insert(
                child_commit.id().clone(),
                rewrite::rebase_commit(
                    &self.settings,
                    tx.mut_repo(),
                    child_commit,
                    new_child_parents,
                )?
                .id()
                .clone(),
            );
        }
        rebased_commit_ids.extend(
            tx.mut_repo()
                .rebase_descendants_return_map(&self.settings)?,
        );

        Ok(rebased_commit_ids)
    }
}

impl SessionOperation {
    pub fn new(repo: Arc<ReadonlyRepo>, id: &WorkspaceId) -> SessionOperation {
        let wc_id = repo
            .view()
            .get_wc_commit_id(id)
            .expect("No working copy found for workspace")
            .clone();

        SessionOperation {
            repo,
            wc_id,
            ref_index: OnceCell::default(),
            prefix_context: OnceCell::default(),
        }
    }

    fn git_backend(&self) -> Option<&GitBackend> {
        self.repo.store().backend_impl().downcast_ref()
    }

    pub fn base_ignores(&self) -> Result<Arc<GitIgnoreFile>> {
        fn get_excludes_file_path(config: &gix::config::File) -> Option<PathBuf> {
            // TODO: maybe use path() and interpolate(), which can process non-utf-8
            // path on Unix.
            if let Some(value) = config.string("core.excludesFile") {
                std::str::from_utf8(&value)
                    .ok()
                    .map(jj_cli::git_util::expand_git_path)
            } else {
                xdg_config_home().ok().map(|x| x.join("git").join("ignore"))
            }
        }

        fn xdg_config_home() -> Result<PathBuf, VarError> {
            if let Ok(x) = std::env::var("XDG_CONFIG_HOME") {
                if !x.is_empty() {
                    return Ok(PathBuf::from(x));
                }
            }
            std::env::var("HOME").map(|x| Path::new(&x).join(".config"))
        }

        let mut git_ignores = GitIgnoreFile::empty();
        if let Some(git_backend) = self.git_backend() {
            let git_repo = git_backend.git_repo();
            if let Some(excludes_file_path) = get_excludes_file_path(&git_repo.config_snapshot()) {
                git_ignores = git_ignores.chain_with_file("", excludes_file_path)?;
            }
            git_ignores = git_ignores
                .chain_with_file("", git_backend.git_repo_path().join("info").join("exclude"))?;
        } else if let Ok(git_config) = gix::config::File::from_globals() {
            if let Some(excludes_file_path) = get_excludes_file_path(&git_config) {
                git_ignores = git_ignores.chain_with_file("", excludes_file_path)?;
            }
        }
        Ok(git_ignores)
    }
}

fn find_workspace_dir(cwd: &Path) -> &Path {
    cwd.ancestors()
        .find(|path| path.join(".jj").is_dir())
        .unwrap_or(cwd)
}

fn parse_revset(
    parse_context: &RevsetParseContext,
    revision: &str,
) -> Result<Rc<RevsetExpression>, RevsetError> {
    let expression = revset::parse(revision, parse_context).context("parse revset")?;
    let expression = revset::optimize(expression);
    Ok(expression)
}

/*************************/
/* from commit_templater */
/*************************/

#[derive(Default)]
pub struct RefIndex {
    index: HashMap<CommitId, Vec<messages::StoreRef>>,
}

impl RefIndex {
    fn insert<'a>(
        &mut self,
        ids: impl IntoIterator<Item = &'a CommitId>,
        r#ref: messages::StoreRef,
    ) {
        for id in ids {
            let ref_names = self.index.entry(id.clone()).or_default();
            ref_names.push(r#ref.clone());
        }
    }

    fn get(&self, id: &CommitId) -> &[messages::StoreRef] {
        if let Some(names) = self.index.get(id) {
            names
        } else {
            &[]
        }
    }
}

fn build_ref_index(repo: &ReadonlyRepo) -> RefIndex {
    let potential_remotes = git_util::get_git_repo(repo.store())
        .ok()
        .and_then(|git_repo| git_repo.remotes().ok())
        .map(|remotes| remotes.len())
        .unwrap_or(0);

    let mut index = RefIndex::default();

    for (branch_name, branch_target) in repo.view().branches() {
        let local_target = branch_target.local_target;
        let remote_refs = branch_target.remote_refs;
        if local_target.is_present() {
            index.insert(
                local_target.added_ids(),
                messages::StoreRef::LocalBranch {
                    branch_name: branch_name.to_owned(),
                    has_conflict: local_target.has_conflict(),
                    is_synced: remote_refs.iter().all(|&(_, remote_ref)| {
                        !remote_ref.is_tracking() || remote_ref.target == *local_target
                    }),
                    tracking_remotes: remote_refs
                        .iter()
                        .filter(|&(_, remote_ref)| remote_ref.is_tracking())
                        .map(|&(remote_name, _)| remote_name.to_owned())
                        .collect(),
                    available_remotes: remote_refs.len(),
                    potential_remotes,
                },
            );
        }
        for &(remote_name, remote_ref) in &remote_refs {
            index.insert(
                remote_ref.target.added_ids(),
                messages::StoreRef::RemoteBranch {
                    branch_name: branch_name.to_owned(),
                    remote_name: remote_name.to_owned(),
                    has_conflict: remote_ref.target.has_conflict(),
                    is_synced: remote_ref.target == *local_target,
                    is_tracked: remote_ref.is_tracking(),
                    is_absent: local_target.is_absent(),
                },
            );
        }
    }

    for (tag_name, tag_target) in repo.view().tags() {
        index.insert(
            tag_target.added_ids(),
            messages::StoreRef::Tag {
                tag_name: tag_name.clone(),
            },
        );
    }

    index
}

fn load_at_head(settings: &UserSettings, workspace: &Workspace) -> Result<SessionOperation> {
    let loader = workspace.repo_loader();

    let op = op_heads_store::resolve_op_heads(
        loader.op_heads_store().as_ref(),
        loader.op_store(),
        |op_heads| {
            let base_repo = loader.load_at(&op_heads[0])?;
            // might want to set some tags
            let mut tx = base_repo.start_transaction(settings);
            for other_op_head in op_heads.into_iter().skip(1) {
                tx.merge_operation(other_op_head)?;
                tx.mut_repo().rebase_descendants(settings)?;
            }
            Ok::<Operation, RepoLoaderError>(
                tx.write("resolve concurrent operations")
                    .leave_unpublished()
                    .operation()
                    .clone(),
            )
        },
    )?;

    let repo: Arc<ReadonlyRepo> = workspace
        .repo_loader()
        .load_at(&op)
        .context("load op head")?;

    Ok(SessionOperation::new(repo, workspace.workspace_id()))
}
