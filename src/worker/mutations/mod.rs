mod change;
mod r#ref;
mod revision;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Error, Result};
use async_trait::async_trait;
use itertools::Itertools;
use jj_cli::{
    git_util::load_git_import_options,
    merge_tools::{self, ConflictResolveError, ExternalMergeTool, MergeEditor},
    ui::Ui,
};
use jj_lib::{
    backend::TreeValue,
    conflicts::{self, ConflictMarkerStyle, ConflictMaterializeOptions, MaterializedTreeValue},
    files::FileMergeHunkLevel,
    git::{
        self, GitBranchPushTargets, GitFetchRefExpression, GitPushOptions, GitSettings,
        GitSubprocessOptions, REMOTE_NAME_FOR_LOCAL_GIT_REPO,
    },
    merge::SameChange,
    merged_tree::MergedTree,
    object_id::ObjectId as ObjectIdTrait,
    op_walk,
    ref_name::WorkspaceNameBuf,
    ref_name::{GitRefNameBuf, RefNameBuf, RemoteName, RemoteNameBuf, RemoteRefSymbol},
    refs::{self, BookmarkPushAction, BookmarkPushUpdate, LocalAndRemoteRef},
    repo::Repo,
    repo_path::RepoPath,
    revset::{self, RevsetIteratorExt},
    rewrite::{self},
    store::Store,
    str_util::StringExpression,
    tree_merge::MergeOptions,
    workspace_store::{SimpleWorkspaceStore, WorkspaceStore as _},
};
use tokio::io::AsyncReadExt;

use super::{
    Mutation,
    git_util::{AuthContext, get_git_remote_names},
    gui_util::WorkspaceSession,
};

use crate::messages::mutations::{
    ExternalDiff, ExternalResolve, ForgetWorkspace, GitFetch, GitPush, GitRefspec, MutationOptions,
    MutationResult, RenameWorkspace, UndoOperation,
};

macro_rules! precondition {
    ($($args:tt)*) => {
        return Ok(MutationResult::PreconditionError { message: format!($($args)*) })
    }
}

#[async_trait(?Send)]
impl Mutation for ExternalDiff {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
        let commit = ws.resolve_change_id(&self.id)?;
        let parents = commit.parents().await?;
        let from_tree = rewrite::merge_commit_trees(ws.repo(), &parents).await?;
        let to_tree = commit.tree();

        let tool_args: jj_cli::config::CommandNameAndArgs =
            ws.data.workspace_settings.get("ui.diff-formatter")?;
        let tool = if let Some(name) = tool_args.as_str() {
            merge_tools::get_external_tool_config(&ws.data.workspace_settings, name)?
                .unwrap_or_else(|| ExternalMergeTool::with_program(name))
        } else {
            ExternalMergeTool::with_diff_args(&tool_args)
        };

        let repo_path = RepoPath::from_internal_string(&self.path.repo_path)?;
        let store = ws.repo().store().clone();
        let left_content = read_file_content(&store, &from_tree, repo_path).await?;
        let right_content = read_file_content(&store, &to_tree, repo_path).await?;
        let relative_path = repo_path.to_fs_path_unchecked(std::path::Path::new(""));

        std::thread::spawn(move || {
            let run = || -> Result<()> {
                let temp_dir = tempfile::Builder::new().prefix("jj-diff-").tempdir()?;

                let left_dir = temp_dir.path().join("left");
                let right_dir = temp_dir.path().join("right");
                let left_path = left_dir.join(&relative_path);
                let right_path = right_dir.join(&relative_path);

                if let Some(parent) = left_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if let Some(parent) = right_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&left_path, &left_content)?;
                std::fs::write(&right_path, &right_content)?;

                let left_rel = left_path
                    .strip_prefix(temp_dir.path())
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                let right_rel = right_path
                    .strip_prefix(temp_dir.path())
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                let patterns = HashMap::from([
                    ("left", left_rel),
                    ("right", right_rel),
                    ("width", "80".to_owned()),
                ]);

                let ui = Ui::null();
                merge_tools::invoke_external_diff(
                    &ui,
                    &mut std::io::sink(),
                    &tool,
                    temp_dir.path(),
                    &patterns,
                )?;
                Ok(())
            };
            if let Err(e) = run() {
                log::error!("external diff tool: {e:#}");
            }
        });

        Ok(MutationResult::Unchanged)
    }
}

#[async_trait(?Send)]
impl Mutation for ExternalResolve {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
        let commit = ws.resolve_change_id(&self.id)?;

        if ws.check_immutable(vec![commit.id().clone()])? {
            precondition!("Revision is immutable");
        }

        let tree = commit.tree();
        let repo_path = RepoPath::from_internal_string(&self.path.repo_path)?;

        let ui = Ui::null();
        let merge_editor = MergeEditor::from_settings(
            &ui,
            &ws.data.workspace_settings,
            ws.data.path_converter.clone(),
            ConflictMarkerStyle::Git,
        )
        .context("failed to load merge editor config")?;

        let (new_tree, _partial) = match merge_editor.edit_files(&ui, &tree, &[repo_path]) {
            Ok(result) => result,
            Err(ConflictResolveError::EmptyOrUnchanged) => {
                return Ok(MutationResult::Unchanged);
            }
            Err(err) => return Err(err.into()),
        };

        if new_tree.tree_ids() == tree.tree_ids() {
            return Ok(MutationResult::Unchanged);
        }

        let mut tx = ws.start_transaction().await?;
        tx.repo_mut()
            .rewrite_commit(&commit)
            .set_tree(new_tree)
            .write()
            .await?;

        match ws
            .finish_transaction(tx, format!("resolve conflicts in {}", commit.id().hex()))
            .await?
        {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for GitFetch {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        let git_repo = match ws.git_repo() {
            Some(git_repo) => git_repo,
            None => precondition!("No git backend"),
        };

        let mut remote_patterns = Vec::new();
        match &self.refspec {
            GitRefspec::AllBookmarks { remote_name } => {
                remote_patterns.push((remote_name.clone(), None));
            }
            GitRefspec::AllRemotes { bookmark_ref } => {
                let bookmark_name = bookmark_ref.as_bookmark()?;
                for remote_name in get_git_remote_names(&git_repo) {
                    remote_patterns.push((remote_name, Some(bookmark_name.to_owned())));
                }
            }
            GitRefspec::RemoteBookmark {
                remote_name,
                bookmark_ref,
            } => {
                let bookmark_name = bookmark_ref.as_bookmark()?;
                remote_patterns.push((remote_name.clone(), Some(bookmark_name.to_owned())));
            }
        }

        // accumulate input requirements
        let mut auth_ctx = AuthContext::new(self.input);
        let progress_sender = ws.sink();
        let git_settings = GitSettings::from_settings(&ws.data.workspace_settings)?;
        let remote_settings = ws.data.workspace_settings.remote_settings()?;
        let import_options = load_git_import_options(&Ui::null(), &git_settings, &remote_settings)
            .map_err(|e| Error::new(e.error))?;

        for (remote_name, pattern) in &remote_patterns {
            let bookmark_expr = pattern
                .clone()
                .map(StringExpression::exact)
                .unwrap_or_else(StringExpression::all);
            let refspecs = git::expand_fetch_refspecs(
                RemoteName::new(remote_name),
                GitFetchRefExpression {
                    bookmark: bookmark_expr,
                    tag: StringExpression::none(),
                },
            )?;

            let result = auth_ctx.with_callbacks(
                Some(progress_sender.clone()),
                ws.session.enable_askpass,
                |cb, env| {
                    let mut subprocess_options = git_settings.to_subprocess_options();
                    subprocess_options.environment = env;

                    let mut fetcher =
                        git::GitFetch::new(tx.repo_mut(), subprocess_options, &import_options)?;

                    fetcher
                        .fetch(RemoteName::new(remote_name), refspecs, cb, None, None)
                        .context("failed to fetch")?;

                    fetcher.import_refs().context("failed to import refs")?;

                    Ok(())
                },
            );

            if let Err(err) = result {
                return Ok(auth_ctx.into_result(err));
            }
        }

        match ws
            .finish_transaction(tx, "fetch from git remote(s)".to_string())
            .await?
        {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for GitPush {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
        let mut tx = ws.start_transaction().await?;

        // determine bookmarks to push, recording the old and new commits
        let mut remote_bookmark_updates: Vec<(&str, Vec<(RefNameBuf, refs::BookmarkPushUpdate)>)> =
            Vec::new();
        let remote_bookmark_refs: Vec<_> = match &self.refspec {
            GitRefspec::AllBookmarks { remote_name } => {
                let remote_name_ref = RemoteNameBuf::from(remote_name);
                let mut bookmark_updates = Vec::new();
                for (bookmark_name, targets) in ws.view().local_remote_bookmarks(&remote_name_ref) {
                    if !targets.remote_ref.is_tracked() {
                        continue;
                    }

                    match classify_bookmark_push(bookmark_name.as_str(), remote_name, targets) {
                        Err(message) => return Ok(MutationResult::PreconditionError { message }),
                        Ok(None) => (),
                        Ok(Some(update)) => {
                            bookmark_updates.push((bookmark_name.to_owned(), update))
                        }
                    }
                }
                remote_bookmark_updates.push((remote_name, bookmark_updates));

                ws.view()
                    .remote_bookmarks(&remote_name_ref)
                    .map(|(name, remote_ref)| (name.to_owned(), remote_ref))
                    .collect()
            }
            GitRefspec::AllRemotes { bookmark_ref } => {
                let bookmark_name = bookmark_ref.as_bookmark()?;
                let bookmark_name_ref = RefNameBuf::from(bookmark_name);

                let mut remote_bookmark_refs = Vec::new();
                for (remote_name, group) in ws
                    .view()
                    .all_remote_bookmarks()
                    .filter_map(|(remote_ref_symbol, remote_ref)| {
                        if remote_ref.is_tracked()
                            && remote_ref_symbol.name == bookmark_name_ref
                            && remote_ref_symbol.remote != REMOTE_NAME_FOR_LOCAL_GIT_REPO
                        {
                            Some((remote_ref_symbol.remote, remote_ref))
                        } else {
                            None
                        }
                    })
                    .chunk_by(|(remote_name, _)| *remote_name)
                    .into_iter()
                {
                    let mut bookmark_updates = Vec::new();
                    for (_, remote_ref) in group {
                        let targets = LocalAndRemoteRef {
                            local_target: ws.view().get_local_bookmark(&bookmark_name_ref),
                            remote_ref,
                        };
                        match classify_bookmark_push(bookmark_name, remote_name.as_str(), targets) {
                            Err(message) => {
                                return Ok(MutationResult::PreconditionError { message });
                            }
                            Ok(None) => (),
                            Ok(Some(update)) => {
                                bookmark_updates.push((RefNameBuf::from(bookmark_name), update))
                            }
                        }
                        remote_bookmark_refs.push((RefNameBuf::from(bookmark_name), remote_ref));
                    }
                    remote_bookmark_updates.push((remote_name.as_str(), bookmark_updates));
                }

                remote_bookmark_refs
            }
            GitRefspec::RemoteBookmark {
                remote_name,
                bookmark_ref,
            } => {
                let bookmark_name = bookmark_ref.as_bookmark()?;
                let bookmark_name_ref = RefNameBuf::from(bookmark_name);
                let local_target = ws.view().get_local_bookmark(&bookmark_name_ref);
                let remote_name_ref = RemoteNameBuf::from(remote_name);
                let remote_ref_symbol = RemoteRefSymbol {
                    name: &bookmark_name_ref,
                    remote: &remote_name_ref,
                };
                let remote_ref = ws.view().get_remote_bookmark(remote_ref_symbol);

                match classify_bookmark_push(
                    bookmark_name,
                    remote_name,
                    LocalAndRemoteRef {
                        local_target,
                        remote_ref,
                    },
                ) {
                    Err(message) => return Ok(MutationResult::PreconditionError { message }),
                    Ok(None) => (),
                    Ok(Some(update)) => {
                        remote_bookmark_updates
                            .push((remote_name, vec![(RefNameBuf::from(bookmark_name), update)]));
                    }
                }

                vec![(
                    RefNameBuf::from(bookmark_name),
                    ws.view().get_remote_bookmark(remote_ref_symbol),
                )]
            }
        };

        // check for conflicts
        let mut new_heads = vec![];
        for (_, bookmark_updates) in &mut remote_bookmark_updates {
            for (_, update) in bookmark_updates {
                if let Some(new_target) = &update.new_target {
                    new_heads.push(new_target.clone());
                }
            }
        }

        let mut old_heads = remote_bookmark_refs
            .into_iter()
            .flat_map(|(_, old_head)| old_head.target.added_ids())
            .cloned()
            .collect_vec();
        if old_heads.is_empty() {
            old_heads.push(ws.repo().store().root_commit_id().clone());
        }

        for commit in revset::walk_revs(ws.repo(), &new_heads, &old_heads)?
            .iter()
            .commits(ws.repo().store())
        {
            let commit = commit?;
            let mut reasons = vec![];
            if commit.description().is_empty() {
                reasons.push("it has no description");
            }
            if commit.author().name.is_empty()
                || commit.author().email.is_empty()
                || commit.committer().name.is_empty()
                || commit.committer().email.is_empty()
            {
                reasons.push("it has no author and/or committer set");
            }
            if commit.has_conflict() {
                reasons.push("it has conflicts");
            }
            if !reasons.is_empty() {
                precondition!(
                    "Won't push revision {} since {}",
                    ws.format_change_id(commit.id(), commit.change_id()).prefix,
                    reasons.join(" and ")
                );
            }
        }

        // check if there are any actual updates to push
        let has_updates = remote_bookmark_updates
            .iter()
            .any(|(_, updates)| !updates.is_empty());
        if !has_updates {
            match &self.refspec {
                GitRefspec::AllBookmarks { remote_name } => {
                    precondition!(
                        "No tracked bookmarks to push to remote '{remote_name}'. Track or push a bookmark first."
                    );
                }
                GitRefspec::AllRemotes { bookmark_ref } => {
                    let bookmark_name = bookmark_ref.as_bookmark()?;
                    precondition!("Bookmark '{bookmark_name}' is not tracked at any remote.");
                }
                GitRefspec::RemoteBookmark {
                    remote_name,
                    bookmark_ref,
                } => {
                    let bookmark_name = bookmark_ref.as_bookmark()?;
                    precondition!(
                        "Bookmark '{bookmark_name}' is not tracked at remote '{remote_name}'. Track it first."
                    );
                }
            }
        }

        // accumulate input requirements
        let mut auth_ctx = AuthContext::new(self.input);
        let event_sink = ws.sink();
        let subprocess_options = GitSubprocessOptions::from_settings(&ws.data.workspace_settings)?;

        // push to each remote
        for (remote_name, branch_updates) in remote_bookmark_updates.into_iter() {
            let targets = GitBranchPushTargets { branch_updates };

            let result = auth_ctx.with_callbacks(
                Some(event_sink.clone()),
                ws.session.enable_askpass,
                |cb, env| {
                    let mut subprocess_options = subprocess_options.clone();
                    subprocess_options.environment = env;

                    git::push_branches(
                        tx.repo_mut(),
                        subprocess_options,
                        RemoteName::new(remote_name),
                        &targets,
                        cb,
                        &GitPushOptions::default(),
                    )
                },
            );

            match result {
                Err(err) => return Ok(auth_ctx.into_result(err.into())),
                Ok(stats) if !stats.all_ok() => {
                    let format_refs = |refs: &[(_, Option<String>)]| {
                        refs.iter()
                            .map(|(ref_name, reason): &(GitRefNameBuf, _)| match reason {
                                Some(msg) if msg.as_str() != "stale info" => {
                                    format!("{} (reason: {})", ref_name.as_str(), msg)
                                }
                                _ => ref_name.as_str().to_string(),
                            })
                            .join(", ")
                    };

                    let mut message = String::new();

                    if !stats.rejected.is_empty() {
                        message += &format!(
                            "The following references unexpectedly moved on the remote: {}. Try fetching first.",
                            format_refs(&stats.rejected)
                        );
                    }

                    if !stats.remote_rejected.is_empty() {
                        if !message.is_empty() {
                            message += "\n\n";
                        }
                        message += &format!(
                            "The remote rejected the following updates: {}. Check if you have permission to push.",
                            format_refs(&stats.remote_rejected)
                        );
                    }

                    return Ok(MutationResult::PreconditionError { message });
                }
                Ok(_) => (),
            }
        }

        match ws
            .finish_transaction(
                tx,
                match &self.refspec {
                    GitRefspec::AllBookmarks { remote_name } => {
                        format!("push all tracked bookmarks to git remote {}", remote_name)
                    }
                    GitRefspec::AllRemotes { bookmark_ref } => {
                        format!(
                            "push {} to all tracked git remotes",
                            bookmark_ref.as_bookmark()?
                        )
                    }
                    GitRefspec::RemoteBookmark {
                        remote_name,
                        bookmark_ref,
                    } => {
                        format!(
                            "push {} to git remote {}",
                            bookmark_ref.as_bookmark()?,
                            remote_name
                        )
                    }
                },
            )
            .await?
        {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

// this is another case where it would be nice if we could reuse jj-cli's error messages
#[async_trait(?Send)]
impl Mutation for UndoOperation {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
        let head_op = op_walk::resolve_op_with_repo(ws.repo(), "@")?; // XXX this should be behind an abstraction, maybe reused in snapshot
        let mut parent_ops = head_op.parents();

        let Some(parent_op) = parent_ops.next().transpose()? else {
            precondition!("Cannot undo repo initialization");
        };

        if parent_ops.next().is_some() {
            precondition!("Cannot undo a merge operation");
        };

        let mut tx = ws.start_transaction().await?;
        let repo_loader = tx.base_repo().loader();
        let head_repo = repo_loader.load_at(&head_op).await?;
        let parent_repo = repo_loader.load_at(&parent_op).await?;
        tx.repo_mut().merge(&head_repo, &parent_repo).await?;
        let restored_view = tx.repo().view().store_view().clone();
        tx.repo_mut().set_view(restored_view);

        match ws
            .finish_transaction(tx, format!("undo operation {}", head_op.id().hex()))
            .await?
        {
            Some(new_status) => {
                let working_copy = ws.get_commit(ws.wc_id())?;
                let new_selection = Some(ws.format_header(&working_copy, None)?);
                Ok(MutationResult::Updated {
                    new_status,
                    new_selection,
                })
            }
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for ForgetWorkspace {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
        let workspace_name: WorkspaceNameBuf = self.name.into();

        let Some(wc_id) = ws.view().get_wc_commit_id(&workspace_name) else {
            precondition!("Workspace '{}' not found", workspace_name.as_symbol());
        };
        if *workspace_name == *ws.name() {
            precondition!("Cannot forget the current workspace");
        }

        let wc_commit = ws.get_commit(wc_id)?;

        let mut tx = ws.start_transaction().await?;
        tx.repo_mut().remove_wc_commit(&workspace_name).await?;

        // abandon the old WC commit if it's empty (same tree as parent merge)
        let parents = wc_commit.parents().await?;
        let parent_tree = rewrite::merge_commit_trees(tx.repo(), &parents).await?;
        if wc_commit.tree().tree_ids() == parent_tree.tree_ids() {
            tx.repo_mut().record_abandoned_commit(&wc_commit);
            tx.repo_mut().rebase_descendants().await?;
        }

        let workspace_store = SimpleWorkspaceStore::load(ws.workspace.repo_path())?;
        workspace_store.forget(&[&*workspace_name])?;

        match ws
            .finish_transaction(
                tx,
                format!("forget workspace '{}'", workspace_name.as_symbol()),
            )
            .await?
        {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

#[async_trait(?Send)]
impl Mutation for RenameWorkspace {
    async fn execute(
        self: Box<Self>,
        ws: &mut WorkspaceSession,
        _options: &MutationOptions,
    ) -> Result<MutationResult> {
        if self.new_name.is_empty() {
            precondition!("New workspace name cannot be empty");
        }

        let old_name: WorkspaceNameBuf = self.name.into();
        let new_name: WorkspaceNameBuf = self.new_name.into();

        if *old_name == *new_name {
            return Ok(MutationResult::Unchanged);
        }

        if ws.view().get_wc_commit_id(&old_name).is_none() {
            precondition!("Workspace '{}' not found", old_name.as_symbol());
        }
        if ws.view().get_wc_commit_id(&new_name).is_some() {
            precondition!("Workspace '{}' already exists", new_name.as_symbol());
        }
        if *old_name != *ws.name() {
            precondition!("Can only rename the current workspace");
        }

        match ws.rename_workspace(old_name, new_name).await? {
            Some(new_status) => Ok(MutationResult::Updated {
                new_status,
                new_selection: None,
            }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

async fn read_file_content(
    store: &Arc<Store>,
    tree: &MergedTree,
    path: &RepoPath,
) -> Result<Vec<u8>> {
    let entry = tree.path_value(path)?;
    match entry.into_resolved() {
        Ok(Some(TreeValue::File { id, .. })) => {
            let mut reader = store.read_file(path, &id).await?;
            let mut content = Vec::new();
            reader.read_to_end(&mut content).await?;
            Ok(content)
        }
        Ok(Some(_)) => Ok(Vec::new()),
        Ok(None) => Ok(Vec::new()),
        Err(_) => {
            // handle conflicts by materializing them
            match conflicts::materialize_tree_value(
                store,
                path,
                tree.path_value(path)?,
                tree.labels(),
            )
            .await?
            {
                MaterializedTreeValue::FileConflict(file) => {
                    let mut content = Vec::new();
                    conflicts::materialize_merge_result(
                        &file.contents,
                        &file.labels,
                        &mut content,
                        &ConflictMaterializeOptions {
                            marker_style: ConflictMarkerStyle::Git,
                            marker_len: None,
                            merge: MergeOptions {
                                hunk_level: FileMergeHunkLevel::Line,
                                same_change: SameChange::Accept,
                            },
                        },
                    )?;
                    Ok(content)
                }
                _ => Ok(Vec::new()),
            }
        }
    }
}

fn classify_bookmark_push(
    bookmark_name: &str,
    remote_name: &str,
    targets: LocalAndRemoteRef,
) -> Result<Option<BookmarkPushUpdate>, String> {
    let push_action = refs::classify_bookmark_push_action(targets);
    match push_action {
        BookmarkPushAction::AlreadyMatches => Ok(None),
        BookmarkPushAction::Update(update) => Ok(Some(update)),
        BookmarkPushAction::LocalConflicted => {
            Err(format!("Bookmark {} is conflicted.", bookmark_name))
        }
        BookmarkPushAction::RemoteConflicted => Err(format!(
            "Bookmark {}@{} is conflicted. Try fetching first.",
            bookmark_name, remote_name
        )),
        BookmarkPushAction::RemoteUntracked => Err(format!(
            "Non-tracking remote bookmark {}@{} exists. Try tracking it first.",
            bookmark_name, remote_name
        )),
    }
}

pub(crate) use precondition;

#[cfg(all(test, not(feature = "ts-rs")))]
mod tests {
    use super::*;
    use crate::{
        messages::{
            RevSet, StoreRef,
            mutations::{CreateRevision, DescribeRevision, MoveRef},
            queries::RevsResult,
        },
        worker::{
            WorkerSession, queries,
            tests::{mkrepo, query_by_id, revs},
        },
    };
    use anyhow::Result;
    use assert_matches::assert_matches;
    use jj_lib::ref_name::WorkspaceName;
    use jj_lib::str_util::StringMatcher;
    use std::fs;

    #[tokio::test]
    async fn immutability_of_bookmark() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let immutable_matcher = StringMatcher::Exact("immutable_bookmark".to_string());
        let (_, ref_at_start) = ws
            .view()
            .local_bookmarks_matching(&immutable_matcher)
            .next()
            .unwrap();
        let ref_at_start = ref_at_start.as_normal().unwrap().clone();
        assert_matches!(ws.check_immutable([ref_at_start.clone()]), Ok(true));

        let header = queries::query_revision(&ws, &revs::immutable_bookmark())?
            .expect("immutable_bookmark exists");
        let immutable_bm = header
            .refs
            .iter()
            .find(|r| {
                matches!(
                    r,
                    StoreRef::LocalBookmark {
                        bookmark_name,
                        ..
                        } if bookmark_name == "immutable_bookmark"
                )
            })
            .unwrap();

        let MutationResult::Updated {
            new_selection: Some(new_selection),
            ..
        } = CreateRevision {
            set: RevSet::singleton(revs::working_copy()),
        }
        .execute_unboxed(&mut ws)
        .await?
        else {
            panic!("Creating new revision didn't update the selection");
        };

        MoveRef {
            r#ref: immutable_bm.clone(),
            to_id: new_selection.id.clone(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        let (_, after_change) = ws
            .view()
            .local_bookmarks_matching(&immutable_matcher)
            .next()
            .unwrap();
        let after_change = after_change.as_normal().unwrap().clone();
        assert_ne!(ref_at_start, after_change);

        assert_matches!(ws.check_immutable([after_change]), Ok(true));

        Ok(())
    }

    #[tokio::test]
    async fn immutable_workspace_head() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let immutable_matcher = StringMatcher::Exact("immutable_bookmark".to_string());

        let header = queries::query_revision(&ws, &revs::immutable_bookmark())?
            .expect("immutable_bookmark exists");
        let immutable_bm = header
            .refs
            .iter()
            .find(|r| {
                matches!(
                    r,
                    StoreRef::LocalBookmark {
                        bookmark_name,
                        ..
                        } if bookmark_name == "immutable_bookmark"
                )
            })
            .unwrap();

        let working_copy = revs::working_copy();
        MoveRef {
            r#ref: immutable_bm.clone(),
            to_id: working_copy,
        }
        .execute_unboxed(&mut ws)
        .await?;

        let (_, after_change) = ws
            .view()
            .local_bookmarks_matching(&immutable_matcher)
            .next()
            .unwrap();
        let after_change = after_change.as_normal().unwrap().clone();

        // rev containing the bookmark is now immutable:
        assert_matches!(ws.check_immutable([after_change]), Ok(true));

        // checked-out rev is not immutable (because we made a new one):
        let current_ws_heads: Vec<jj_lib::backend::CommitId> = ws
            .repo()
            .view()
            .wc_commit_ids()
            .iter()
            .map(|(_, id)| id.clone())
            .collect();
        assert_matches!(ws.check_immutable(current_ws_heads), Ok(false));

        Ok(())
    }

    // XXX possibly this should be a session test using the ExecuteSnapshot event
    #[tokio::test]
    async fn snapshot_respects_auto_track_config() -> Result<()> {
        let repo = mkrepo();

        // Configure snapshot.auto-track to only track .txt files
        let config_path = repo.path().join(".jj/repo/config.toml");
        let config_content = r#"
[snapshot]
auto-track = "glob:*.txt"
"#;
        fs::write(&config_path, config_content).unwrap();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        // Write two new files, one tracked and one untracked
        fs::write(repo.path().join("tracked.txt"), "tracked content").unwrap();
        fs::write(repo.path().join("untracked.dat"), "untracked content").unwrap();

        // Trigger a snapshot by describing the revision
        DescribeRevision {
            id: revs::working_copy(),
            new_description: "test auto-track".to_owned(),
            reset_author: false,
        }
        .execute_unboxed(&mut ws)
        .await?;

        // Verify: only the .txt file should have been tracked
        let rev = query_by_id(&ws, revs::working_copy()).await?;
        match rev {
            RevsResult::Detail { changes, .. } => {
                assert_eq!(changes.len(), 1);
                assert_eq!(changes[0].path.repo_path, "tracked.txt");
            }
            _ => panic!("Expected working copy to exist"),
        }

        // Verify: the .dat file should exist, but be untracked
        assert!(repo.path().join("untracked.dat").exists());

        Ok(())
    }

    #[tokio::test]
    async fn forget_workspace_via_mutation() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        ws.add_workspace("second".to_owned(), repo.path().join("second-workspace"))
            .await?;
        assert!(
            ws.view()
                .get_wc_commit_id(WorkspaceName::new("second"))
                .is_some()
        );

        let result = ForgetWorkspace {
            name: "second".to_owned(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::Updated { .. });
        assert!(
            ws.view()
                .get_wc_commit_id(WorkspaceName::new("second"))
                .is_none()
        );

        Ok(())
    }

    #[tokio::test]
    async fn forget_workspace_abandons_empty_wc() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        ws.add_workspace("second".to_owned(), repo.path().join("second-workspace"))
            .await?;

        let count_before = queries::query_log(&ws, "all()", 100)?.rows.len();

        ForgetWorkspace {
            name: "second".to_owned(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        let count_after = queries::query_log(&ws, "all()", 100)?.rows.len();
        assert_eq!(
            count_before - 1,
            count_after,
            "empty WC commit should be abandoned"
        );

        Ok(())
    }

    #[tokio::test]
    async fn forget_workspace_rejects_current() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let result = ForgetWorkspace {
            name: ws.name().as_str().to_owned(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { message } if message.contains("current workspace"));

        Ok(())
    }

    #[tokio::test]
    async fn forget_workspace_rejects_nonexistent() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let result = ForgetWorkspace {
            name: "nonexistent".to_owned(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { message } if message.contains("not found"));

        Ok(())
    }

    #[tokio::test]
    async fn rename_workspace_via_mutation() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let old_name = ws.name().as_str().to_owned();

        let result = RenameWorkspace {
            name: old_name.clone(),
            new_name: "renamed".to_owned(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::Updated { .. });
        assert_eq!(ws.name().as_str(), "renamed");
        assert!(
            ws.view()
                .get_wc_commit_id(WorkspaceName::new(&old_name))
                .is_none()
        );
        assert!(
            ws.view()
                .get_wc_commit_id(WorkspaceName::new("renamed"))
                .is_some()
        );

        Ok(())
    }

    #[tokio::test]
    async fn rename_workspace_same_name_is_unchanged() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let name = ws.name().as_str().to_owned();

        let result = RenameWorkspace {
            name: name.clone(),
            new_name: name,
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::Unchanged);

        Ok(())
    }

    #[tokio::test]
    async fn rename_workspace_rejects_empty_name() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        let result = RenameWorkspace {
            name: ws.name().as_str().to_owned(),
            new_name: "".to_owned(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { message } if message.contains("cannot be empty"));

        Ok(())
    }

    #[tokio::test]
    async fn rename_workspace_rejects_duplicate_name() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        ws.add_workspace("other".to_owned(), repo.path().join("other-workspace"))
            .await?;

        let result = RenameWorkspace {
            name: ws.name().as_str().to_owned(),
            new_name: "other".to_owned(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { message } if message.contains("already exists"));

        Ok(())
    }

    #[tokio::test]
    async fn rename_workspace_rejects_non_current() -> Result<()> {
        let repo = mkrepo();

        let mut session = WorkerSession::default();
        let mut ws = session.load_workspace(repo.path()).await?;

        ws.add_workspace("other".to_owned(), repo.path().join("other-workspace"))
            .await?;

        let result = RenameWorkspace {
            name: "other".to_owned(),
            new_name: "renamed".to_owned(),
        }
        .execute_unboxed(&mut ws)
        .await?;

        assert_matches!(result, MutationResult::PreconditionError { message } if message.contains("current workspace"));

        Ok(())
    }
}
