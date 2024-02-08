use serde::Serialize;
use ts_rs::TS;

#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages.ts")]
pub struct LogChange {
    pub change_id: String,
    pub commit_id: String,
    pub description: String,
    pub email: String,
    pub timestamp: String,
}
