pub mod messages;
pub mod web;
pub mod worker;

// internal modules exposed for the binary, not part of the public API
#[doc(hidden)]
pub mod config;
#[doc(hidden)]
pub mod git_util;

use std::fmt::Display;
use std::path::PathBuf;

use clap::ValueEnum;
use jj_lib::settings::UserSettings;

use crate::config::read_config;

#[derive(Clone, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum LaunchMode {
    #[default]
    Gui,
    Web,
}

impl Display for LaunchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LaunchMode::Gui => write!(f, "gui"),
            LaunchMode::Web => write!(f, "web"),
        }
    }
}

pub struct RunOptions {
    pub context: tauri::Context<tauri::Wry>,
    pub settings: UserSettings,
    pub workspace: Option<PathBuf>,
    pub debug: bool,
    pub is_child: bool,
    pub ignore_immutable: bool,
}

impl RunOptions {
    pub fn new(workspace: PathBuf) -> Self {
        let (settings, _, _) = read_config(Some(workspace.as_ref())).unwrap();
        RunOptions {
            context: tauri::generate_context!(),
            workspace: Some(workspace),
            settings,
            debug: false,
            is_child: false,
            ignore_immutable: false,
        }
    }
}
