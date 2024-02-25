use anyhow::Result;
use jj_lib::{backend::CommitId, commit::Commit, object_id::ObjectId};

use crate::{
    gui_util::SessionOperation,
    messages::{DescribeRevision, MutationResult},
};

pub fn describe_revision(
    op: &SessionOperation,
    mutation: DescribeRevision,
) -> Result<MutationResult> {
    let commit: Commit = todo!(); // get from raw id, but revid doesn't have it yet

    if !op.check_rewritable([&commit]) {
        Ok(MutationResult::Failed {
            message: format!("Commit {} is immutable", short_commit_hash(commit.id())),
        })
    } else if mutation.new_description == commit.description() {
        Ok(MutationResult::Unchanged)
    } else {
        let mut tx = op.start_transaction();
        let mut commit_builder = tx
            .mut_repo()
            .rewrite_commit(&op.session.settings, &commit)
            .set_description(mutation.new_description);
        commit_builder.write()?;
        op.finish_transaction(tx, format!("describe commit {}", commit.id().hex()))?;
        Ok(MutationResult::Updated {
            new_status: todo!(),
        })
    }
}

pub fn short_commit_hash(commit_id: &CommitId) -> String {
    commit_id.hex()[0..12].to_string()
}
