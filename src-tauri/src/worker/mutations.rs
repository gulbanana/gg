use crate::messages::{DescribeRevision, MutationResult};

pub fn describe_revision(mutation: DescribeRevision) -> MutationResult {
    println!("{mutation:?}");
    MutationResult::Failure {
        message: "Not implemented".to_owned(),
    }
}
