use anyhow::Result;
use jj_lib::{backend::CommitId, object_id::ObjectId};

use crate::{
    gui_util::WorkspaceSession,
    messages::{DescribeRevision, MutationResult},
};

pub fn describe_revision(
    ws: &mut WorkspaceSession,
    mutation: DescribeRevision,
) -> Result<MutationResult> {
    let mut tx = ws.start_transaction()?;

    let commit = ws.evaluate_revision(&mutation.change_id.hex)?;

    if !ws.check_rewritable([&commit]) {
        Ok(MutationResult::Failed {
            message: format!("Commit {} is immutable", short_commit_hash(commit.id())),
        })
    } else if mutation.new_description == commit.description() {
        Ok(MutationResult::Unchanged)
    } else {
        let commit_builder = tx
            .mut_repo()
            .rewrite_commit(&ws.settings, &commit)
            .set_description(mutation.new_description);

        commit_builder.write()?;

        match ws.finish_transaction(tx, format!("describe commit {}", commit.id().hex()))? {
            Some(new_status) => Ok(MutationResult::Updated { new_status }),
            None => Ok(MutationResult::Unchanged),
        }
    }
}

pub fn short_commit_hash(commit_id: &CommitId) -> String {
    commit_id.hex()[0..12].to_string()
}
