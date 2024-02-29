use anyhow::Result;
use jj_lib::{backend::CommitId, object_id::ObjectId, repo::Repo};

use crate::{
    gui_util::WorkspaceSession,
    messages::{DescribeRevision, MutationResult},
};

pub fn describe_revision(
    ws: &mut WorkspaceSession,
    mutation: DescribeRevision,
) -> Result<MutationResult> {
    let id = CommitId::try_from_hex(&mutation.commit_id.hex)?;
    let commit = ws.operation.repo.store().get_commit(&id)?;

    if !ws.check_rewritable([&commit]) {
        Ok(MutationResult::Failed {
            message: format!("Commit {} is immutable", short_commit_hash(commit.id())),
        })
    } else if mutation.new_description == commit.description() {
        Ok(MutationResult::Unchanged)
    } else {
        let mut tx = ws.start_transaction();
        let commit_builder = tx
            .mut_repo()
            .rewrite_commit(&ws.settings, &commit)
            .set_description(mutation.new_description);
        commit_builder.write()?;

        if !tx.mut_repo().has_changes() {
            return Ok(MutationResult::Unchanged);
        }

        let new_status =
            ws.finish_transaction(tx, format!("describe commit {}", commit.id().hex()))?;

        Ok(MutationResult::Updated { new_status })
    }
}

pub fn short_commit_hash(commit_id: &CommitId) -> String {
    commit_id.hex()[0..12].to_string()
}
