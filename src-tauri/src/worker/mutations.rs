use crate::messages::{DescribeRevision, MutationResult};

use super::WorkspaceSession;

pub fn describe_revision(ws: &mut WorkspaceSession, mutation: DescribeRevision) -> MutationResult {
    println!("{mutation:?}");
    MutationResult::Failure {
        message: "Not implemented".to_owned(),
    }
}
