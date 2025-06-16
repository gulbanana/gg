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

use anyhow::{Context, Result, anyhow};
use chrono::TimeZone;
use git2::Repository;
use itertools::Itertools;
use jj_cli::{cli_util::short_operation_hash, git_util::is_colocated_git_workspace, revset_util};
use jj_lib::{
    backend::{BackendError, ChangeId, CommitId},
    commit::Commit,
    conflicts::ConflictMarkerStyle,
    default_index::{AsCompositeIndex, DefaultReadonlyIndex},
    file_util, git,
    git_backend::GitBackend,
    gitignore::GitIgnoreFile,
    id_prefix::{IdPrefixContext, IdPrefixIndex},
    matchers::EverythingMatcher,
    object_id::ObjectId,
    op_heads_store,
    operation::Operation,
    ref_name::WorkspaceName,
    repo::{ReadonlyRepo, Repo, RepoLoaderError, StoreFactories},
    repo_path::{RepoPath, RepoPathUiConverter},
    revset::{
        self, DefaultSymbolResolver, Revset, RevsetAliasesMap, RevsetDiagnostics,
        RevsetEvaluationError, RevsetExpression, RevsetExtensions, RevsetIteratorExt,
        RevsetParseContext, RevsetResolutionError, RevsetWorkspaceContext, SymbolResolverExtension,
        UserRevsetExpression,
    },
    rewrite::{self, RebaseOptions, RebasedCommit},
    settings::{HumanByteSize, UserSettings},
    transaction::Transaction,
    view::View,
    working_copy::{CheckoutOptions, CheckoutStats, SnapshotOptions, WorkingCopyFreshness},
    workspace::{self, DefaultWorkspaceLoaderFactory, Workspace, WorkspaceLoaderFactory},
};
use thiserror::Error;

use super::WorkerSession;
use crate::{
    config::{GGSettings, read_config},
    messages::{self, RevId},
};

/// jj-dependent state, available when a workspace is open
pub struct WorkspaceSession<'a> {
    pub(crate) session: &'a mut WorkerSession,

    // workspace-level data, initialised once
    pub workspace: Workspace,
    pub data: WorkspaceData,
    is_large: bool, // this is based on the head operation and thus derived from the rest of the data

    // operation-specific data, containing a repo view and derived extras
    operation: SessionOperation,
    is_colocated: bool,
}

pub struct WorkspaceData {
    path_converter: RepoPathUiConverter,
    extensions: RevsetExtensions,
    pub settings: UserSettings,
    pub aliases_map: RevsetAliasesMap,
}

/// state derived from a specific operation
pub struct SessionOperation {
    pub repo: Arc<ReadonlyRepo>,
    pub wc_id: CommitId,
    ref_index: OnceCell<Rc<RefIndex>>,
    prefix_context: IdPrefixContext,
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
        let factory = DefaultWorkspaceLoaderFactory;
        let loader = factory.create(find_workspace_dir(cwd))?;

        let (settings, aliases_map) = read_config(Some(loader.repo_path()))?;

        let workspace = loader.load(
            &settings,
            &StoreFactories::default(),
            &workspace::default_working_copy_factories(),
        )?;

        let path_converter = RepoPathUiConverter::Fs {
            cwd: workspace.workspace_root().to_owned(),
            base: workspace.workspace_root().to_owned(),
        };

        let data: WorkspaceData = WorkspaceData {
            settings,
            path_converter,
            aliases_map,
            extensions: Default::default(),
        };

        let operation = load_at_head(&workspace, &data)?;

        let index_store = workspace.repo_loader().index_store();
        let index = index_store
            .get_index_at_op(operation.repo.operation(), workspace.repo_loader().store())?;
        let is_large =
            if let Some(default_index) = index.as_any().downcast_ref::<DefaultReadonlyIndex>() {
                let stats = default_index.as_composite().stats();
                stats.num_commits as i64 >= data.settings.query_large_repo_heuristic()
            } else {
                true
            };

        let is_colocated = is_colocated_git_workspace(&workspace, &operation.repo);

        Ok(WorkspaceSession {
            session: self,
            workspace,
            data,
            is_large,
            operation,
            is_colocated,
        })
    }
}

impl WorkspaceSession<'_> {
    pub fn name(&self) -> &WorkspaceName {
        self.workspace.workspace_name()
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
        Ok(self.operation.repo.store().get_commit(id)?)
    }

    pub fn git_repo(&self) -> Result<Option<Repository>> {
        match self.operation.git_backend() {
            Some(backend) => Ok(Some(Repository::open(backend.git_repo_path())?)),
            None => Ok(None),
        }
    }

    pub fn load_at_head(&mut self) -> Result<bool> {
        let head = load_at_head(&self.workspace, &self.data)?;
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
        revset_expr: Rc<UserRevsetExpression>,
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
        let mut diagnostics = RevsetDiagnostics::new(); // XXX pass this down, then include it in the Result
        let expr =
            revset_util::parse_immutable_heads_expression(&mut diagnostics, &self.parse_context())?;
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
                return Ok(None);
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
            .collect::<Result<Vec<Commit>, RevsetEvaluationError>>()?;
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

    pub fn parse_context(&self) -> RevsetParseContext<'_> {
        self.data.parse_context(self.workspace.workspace_name())
    }

    // the prefix context caches this itself, but the way it does so is not convenient for us - you need a fallible method and the &dyn Repo
    fn prefix_index(&self) -> IdPrefixIndex<'_> {
        self.operation
            .prefix_context
            .populate(self.repo())
            .expect("prefix context disambiguate_within()")
    }

    fn resolver(&self) -> DefaultSymbolResolver {
        DefaultSymbolResolver::new(
            self.operation.repo.as_ref(),
            &([] as [Box<dyn SymbolResolverExtension>; 0]),
        )
        .with_id_prefix_context(&self.operation.prefix_context)
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

        let default_query = self
            .data
            .settings
            .get_string("revsets.log")
            .unwrap_or_default();

        let latest_query = self
            .session
            .latest_query
            .as_ref()
            .unwrap_or(&default_query)
            .clone();

        Ok(messages::RepoConfig::Workspace {
            absolute_path,
            git_remotes,
            default_query,
            latest_query,
            status: self.format_status(),
            theme_override: self.data.settings.ui_theme_override(),
            mark_unpushed_branches: self.data.settings.ui_mark_unpushed_bookmarks(),
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
            .prefix_index()
            .shortest_commit_prefix_len(self.operation.repo.as_ref(), id);

        let hex = id.hex();
        let mut prefix = hex.clone();
        let rest = prefix.split_off(prefix_len);
        messages::CommitId { hex, prefix, rest }
    }

    pub fn format_change_id(&self, id: &ChangeId) -> messages::ChangeId {
        let prefix_len = self
            .prefix_index()
            .shortest_change_prefix_len(self.operation.repo.as_ref(), id);

        let hex = &id.reverse_hex();
        let mut prefix = hex.clone();
        let rest = prefix.split_off(prefix_len);
        messages::ChangeId {
            hex: hex.clone(),
            prefix,
            rest,
        }
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
        let branches = index.get(commit.id()).to_vec();

        let is_immutable = known_immutable
            .map(Result::Ok)
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

    pub fn format_path<T: AsRef<RepoPath>>(&self, repo_path: T) -> Result<messages::TreePath> {
        let base_path = self.workspace.workspace_root();
        let relative_path =
            file_util::relative_path(base_path, &repo_path.as_ref().to_fs_path(base_path)?);
        Ok(messages::TreePath {
            repo_path: repo_path.as_ref().as_internal_file_string().to_owned(),
            relative_path: relative_path.into(),
        })
    }

    pub fn check_immutable(&self, ids: impl IntoIterator<Item = CommitId>) -> Result<bool> {
        let check_revset = RevsetExpression::commits(ids.into_iter().collect());

        let mut diagnostics = RevsetDiagnostics::new();
        let immutable_revset =
            revset_util::parse_immutable_heads_expression(&mut diagnostics, &self.parse_context())?;
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
        Ok(self.operation.repo.start_transaction())
    }

    pub fn finish_transaction(
        &mut self,
        mut tx: Transaction,
        description: impl Into<String>,
    ) -> Result<Option<messages::RepoStatus>> {
        if !tx.repo().has_changes() {
            return Ok(None);
        }

        tx.repo_mut().rebase_descendants()?;

        let old_repo = tx.base_repo().clone();

        let maybe_old_wc_commit = old_repo
            .view()
            .get_wc_commit_id(self.workspace.workspace_name())
            .map(|commit_id| tx.base_repo().store().get_commit(commit_id))
            .transpose()?;
        let maybe_new_wc_commit = tx
            .repo()
            .view()
            .get_wc_commit_id(self.workspace.workspace_name())
            .map(|commit_id| tx.repo().store().get_commit(commit_id))
            .transpose()?;
        if self.is_colocated {
            if let Some(wc_commit) = &maybe_new_wc_commit {
                git::reset_head(tx.repo_mut(), wc_commit)?;
            }
            git::export_refs(tx.repo_mut())?;
        }

        self.operation = SessionOperation::new(self.name(), &self.data, tx.commit(description)?);

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
                .data
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
        let workspace_name = self.workspace.workspace_name().to_owned();
        let get_wc_commit = |repo: &ReadonlyRepo| -> Result<Option<_>, _> {
            repo.view()
                .get_wc_commit_id(&workspace_name)
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
        let (repo, wc_commit) = match WorkingCopyFreshness::check_stale(
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

        let HumanByteSize(mut max_new_file_size) = self
            .data
            .settings
            .get_value_with("snapshot.max-new-file-size", TryInto::try_into)?;
        if max_new_file_size == 0 {
            max_new_file_size = u64::MAX;
        }
        let (new_tree_id, _) = locked_ws.locked_wc().snapshot(&SnapshotOptions {
            base_ignores,
            fsmonitor_settings: self.data.settings.fsmonitor_settings()?,
            progress: None,
            max_new_file_size,
            start_tracking_matcher: &EverythingMatcher,
            conflict_marker_style: ConflictMarkerStyle::default(),
        })?;

        let did_anything = new_tree_id != *wc_commit.tree_id();

        if did_anything {
            let mut tx = repo.start_transaction();
            let mut_repo = tx.repo_mut();
            let commit = mut_repo
                .rewrite_commit(&wc_commit)
                .set_tree_id(new_tree_id)
                .write()?;
            mut_repo.set_wc_commit(workspace_name.clone(), commit.id().clone())?;

            mut_repo.rebase_descendants()?;

            if self.is_colocated {
                git::export_refs(mut_repo)?;
            }

            self.operation = SessionOperation::new(
                &workspace_name,
                &self.data,
                tx.commit("snapshot working copy")?,
            );
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
                &CheckoutOptions {
                    conflict_marker_style: ConflictMarkerStyle::default(),
                },
            )?)
        } else {
            let locked_ws = self.workspace.start_working_copy_mutation()?;
            locked_ws.finish(self.operation.repo.op_id().clone())?;
            None
        })
    }

    fn import_git_head(&mut self) -> Result<()> {
        let mut tx = self.operation.repo.start_transaction();
        git::import_head(tx.repo_mut())?;
        if !tx.repo().has_changes() {
            return Ok(());
        }

        let new_git_head = tx.repo().view().git_head().clone();
        if let Some(new_git_head_id) = new_git_head.as_normal() {
            let workspace_name = self.workspace.workspace_name().to_owned();

            if let Some(old_wc_commit_id) =
                self.operation.repo.view().get_wc_commit_id(&workspace_name)
            {
                let old_wc_commit = tx.repo().store().get_commit(old_wc_commit_id)?;
                tx.repo_mut().record_abandoned_commit(&old_wc_commit);
            }

            let new_git_head_commit = tx.repo().store().get_commit(new_git_head_id)?;
            tx.repo_mut()
                .check_out(workspace_name.clone(), &new_git_head_commit)?;

            let mut locked_ws = self.workspace.start_working_copy_mutation()?;

            locked_ws.locked_wc().reset(&new_git_head_commit)?;
            tx.repo_mut().rebase_descendants()?;

            self.operation =
                SessionOperation::new(&workspace_name, &self.data, tx.commit("import git head")?);

            locked_ws.finish(self.operation.repo.op_id().clone())?;
        } else {
            self.finish_transaction(tx, "import git head")?;
        }
        Ok(())
    }

    fn import_git_refs(&mut self) -> Result<()> {
        let git_settings = self.data.settings.git_settings()?;
        let mut tx = self.operation.repo.start_transaction();
        // Automated import shouldn't fail because of reserved remote name.
        let stats = jj_lib::git::import_refs(tx.repo_mut(), &git_settings)?;
        if !tx.repo().has_changes() {
            return Ok(());
        }

        tx.repo_mut().rebase_descendants()?;

        self.finish_transaction(tx, format!("import git refs: {:?}", stats))?;
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
            .evaluate(self.operation.repo.as_ref())?
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
            let new_child_parents: Result<Vec<CommitId>, _> = new_child_parents_expression
                .evaluate(tx.base_repo().as_ref())?
                .iter()
                .collect();

            rebased_commit_ids.insert(
                child_commit.id().clone(),
                rewrite::rebase_commit(tx.repo_mut(), child_commit, new_child_parents?)?
                    .id()
                    .clone(),
            );
        }
        {
            let mut mapping = HashMap::new();
            tx.repo_mut().rebase_descendants_with_options(
                &RebaseOptions::default(),
                |old_commit, rebased| {
                    mapping.insert(
                        old_commit.id().clone(),
                        match rebased {
                            RebasedCommit::Rewritten(new_commit) => new_commit.id().clone(),
                            RebasedCommit::Abandoned { parent_id } => parent_id,
                        },
                    );
                },
            )?;
            rebased_commit_ids.extend(mapping);
        }

        Ok(rebased_commit_ids)
    }
}

impl WorkspaceData {
    // unfortunately not cached as it borrows from everything
    fn parse_context<'a>(&'a self, name: &'a WorkspaceName) -> RevsetParseContext<'a> {
        let workspace_context = RevsetWorkspaceContext {
            path_converter: &self.path_converter,
            workspace_name: name,
        };
        let now = if let Some(timestamp) = self.settings.commit_timestamp() {
            chrono::Local
                .timestamp_millis_opt(timestamp.timestamp.0)
                .unwrap()
        } else {
            chrono::Local::now()
        };
        RevsetParseContext {
            aliases_map: &self.aliases_map,
            local_variables: HashMap::new(),
            user_email: self.settings.user_email(),
            date_pattern_context: now.into(),
            extensions: &self.extensions,
            workspace: Some(workspace_context),
        }
    }
}

impl SessionOperation {
    pub fn new(
        id: &WorkspaceName,
        data: &WorkspaceData,
        repo: Arc<ReadonlyRepo>,
    ) -> SessionOperation {
        let wc_id = repo
            .view()
            .get_wc_commit_id(id)
            .expect("No working copy found for workspace")
            .clone();

        let revset_string: String = data
            .settings
            .get_string("revsets.short-prefixes")
            .unwrap_or_else(|_| data.settings.get_string("revsets.log").unwrap_or_default());

        // guarantee that an index can be populated - we will unwrap later
        let prefix_context =
            IdPrefixContext::default().disambiguate_within(if !revset_string.is_empty() {
                parse_revset(&data.parse_context(id), &revset_string)
                    .expect("init prefix context: parse revsets.short-prefixes")
            } else {
                RevsetExpression::all()
            });

        SessionOperation {
            repo,
            wc_id,
            ref_index: OnceCell::default(),
            prefix_context,
        }
    }

    fn git_backend(&self) -> Option<&GitBackend> {
        self.repo.store().backend_impl().downcast_ref()
    }

    // XXX out of snyc with jj-cli version
    pub fn base_ignores(&self) -> Result<Arc<GitIgnoreFile>> {
        fn get_excludes_file_path(config: &gix::config::File) -> Option<PathBuf> {
            // TODO: maybe use path() and interpolate(), which can process non-utf-8
            // path on Unix.
            if let Some(value) = config.string("core.excludesFile") {
                std::str::from_utf8(&value)
                    .ok()
                    .map(file_util::expand_home_path)
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
) -> Result<Rc<UserRevsetExpression>, RevsetError> {
    let mut diagnostics = RevsetDiagnostics::new(); // XXX move this up and include it in errors
    let expression =
        revset::parse(&mut diagnostics, revision, parse_context).context("parse revset")?;
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
    let potential_remotes = git::get_git_backend(repo.store())
        .ok()
        .map(|git_backend| git_backend.git_repo().remote_names().len())
        .unwrap_or(0);

    let mut index = RefIndex::default();

    for (branch_name, branch_target) in repo.view().bookmarks() {
        let local_target = branch_target.local_target;
        let remote_refs = branch_target.remote_refs;
        if local_target.is_present() {
            index.insert(
                local_target.added_ids(),
                messages::StoreRef::LocalBookmark {
                    branch_name: branch_name.as_str().to_owned(),
                    has_conflict: local_target.has_conflict(),
                    is_synced: remote_refs.iter().all(|&(_, remote_ref)| {
                        !remote_ref.is_tracked() || remote_ref.target == *local_target
                    }),
                    tracking_remotes: remote_refs
                        .iter()
                        .filter(|&(_, remote_ref)| remote_ref.is_tracked())
                        .map(|&(remote_name, _)| remote_name.as_str().to_owned())
                        .collect(),
                    available_remotes: remote_refs.len(),
                    potential_remotes,
                },
            );
        }
        for &(remote_name, remote_ref) in &remote_refs {
            index.insert(
                remote_ref.target.added_ids(),
                messages::StoreRef::RemoteBookmark {
                    branch_name: branch_name.as_str().to_owned(),
                    remote_name: remote_name.as_str().to_owned(),
                    has_conflict: remote_ref.target.has_conflict(),
                    is_synced: remote_ref.target == *local_target,
                    is_tracked: remote_ref.is_tracked(),
                    is_absent: local_target.is_absent(),
                },
            );
        }
    }

    for (tag_name, tag_target) in repo.view().tags() {
        index.insert(
            tag_target.added_ids(),
            messages::StoreRef::Tag {
                tag_name: tag_name.as_str().to_owned(),
            },
        );
    }

    index
}

fn load_at_head(workspace: &Workspace, data: &WorkspaceData) -> Result<SessionOperation> {
    let loader = workspace.repo_loader();

    let op = op_heads_store::resolve_op_heads(
        loader.op_heads_store().as_ref(),
        loader.op_store(),
        |op_heads| {
            let base_repo = loader.load_at(&op_heads[0])?;
            // might want to set some tags
            let mut tx = base_repo.start_transaction();
            for other_op_head in op_heads.into_iter().skip(1) {
                tx.merge_operation(other_op_head)?;
                tx.repo_mut().rebase_descendants()?;
            }
            Ok::<Operation, RepoLoaderError>(
                tx.write("resolve concurrent operations")?
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

    Ok(SessionOperation::new(
        workspace.workspace_name(),
        data,
        repo,
    ))
}
