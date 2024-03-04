use anyhow::Result;
use jj_lib::{backend::CommitId, object_id::ObjectId, repo::Repo};

use crate::{
    gui_util::{SessionOperation, WorkspaceSession},
    messages::{DescribeRevision, MutationResult},
};

pub fn describe_revision(
    ws: &WorkspaceSession,
    op: &SessionOperation,
    mutation: DescribeRevision,
) -> Result<MutationResult> {
    let id = CommitId::try_from_hex(&mutation.commit_id.hex)?;
    let commit = op.repo.store().get_commit(&id)?;

    if !op.check_rewritable([&commit]) {
        Ok(MutationResult::Failed {
            message: format!("Commit {} is immutable", short_commit_hash(commit.id())),
        })
    } else if mutation.new_description == commit.description() {
        Ok(MutationResult::Unchanged)
    } else {
        let mut tx = op.start_transaction(ws);
        let mut commit_builder = tx
            .mut_repo()
            .rewrite_commit(&ws.settings, &commit)
            .set_description(mutation.new_description);
        commit_builder.write()?;

        if !tx.mut_repo().has_changes() {
            return Ok(MutationResult::Unchanged);
        }

        let new_repo =
            op.finish_transaction(ws, tx, format!("describe commit {}", commit.id().hex()))?;
        Ok(MutationResult::Updated {
            new_status: todo!(),
        })
    }
}

pub fn short_commit_hash(commit_id: &CommitId) -> String {
    commit_id.hex()[0..12].to_string()
}
