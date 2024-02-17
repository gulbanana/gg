use chrono::Local;
use serde::Serialize;
use ts_rs::TS;

/// Utility multiline-string type, used to abstract crlf/<br>/etc
#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
pub struct Text {
    pub lines: Vec<String>,
}

impl<'a, T> From<T> for Text
where
    T: Into<&'a str>,
{
    fn from(value: T) -> Self {
        Text {
            lines: value.into().split("\n").map(|l| l.to_owned()).collect(),
        }
    }
}

pub struct WSStatus {
    pub root_path: String,
    pub operation_description: String,
}

/// A change or commit id with a disambiguated prefix
#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
pub struct RevId {
    pub prefix: String,
    pub rest: String,
}

#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
pub struct RevHeader {
    pub change_id: RevId,
    pub commit_id: RevId,
    pub description: Text,
    pub author: String,
    pub email: String,
    pub timestamp: chrono::DateTime<Local>,
}

#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
pub struct RevDetail {
    pub header: RevHeader,
    pub diff: Vec<DiffPath>,
}

#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
#[serde(tag = "type")]
pub enum DiffPath {
    Added { relative_path: String },
    Deleted { relative_path: String },
    Modified { relative_path: String },
}
