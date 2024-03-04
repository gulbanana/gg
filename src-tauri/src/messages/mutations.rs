use super::*;

/// Common result type for mutating commands
#[derive(Serialize, Clone)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub enum MutationResult {
    Unchanged,
    Updated { new_status: RepoStatus },
    Failed { message: String },
}

/// Makes a revision the working copy
#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct CheckoutRevision {
    pub change_id: RevId,
}

/// Updates a revision's description
#[derive(Deserialize, Debug)]
#[cfg_attr(
    feature = "ts-rs",
    derive(TS),
    ts(export, export_to = "../src/messages/")
)]
pub struct DescribeRevision {
    pub change_id: RevId,
    pub new_description: String,
}
