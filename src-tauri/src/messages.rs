use serde::Serialize;
use ts_rs::TS;

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
    pub email: String,
    pub timestamp: String,
}

#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
pub struct RevDetail {
    pub header: RevHeader,
    pub paths: Vec<ChangePath>,
}

#[derive(TS, Serialize)]
#[ts(export, export_to = "../src/messages/")]
pub struct ChangePath {
    pub relative_path: String,
}
