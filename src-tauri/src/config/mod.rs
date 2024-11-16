use std::path::Path;

use anyhow::{anyhow, Result};
use config::{Config, ConfigError};
use itertools::Itertools;
use jj_cli::config::LayeredConfigs;
use jj_lib::{
    revset::RevsetAliasesMap,
    settings::{ConfigResultExt, UserSettings},
};

pub trait GGSettings {
    fn query_log_page_size(&self) -> usize;
    fn query_large_repo_heuristic(&self) -> i64;
    fn query_auto_snapshot(&self) -> Option<bool>;
    fn ui_theme_override(&self) -> Option<String>;
    fn ui_mark_unpushed_bookmarks(&self) -> bool;
    #[allow(dead_code)]
    fn ui_recent_workspaces(&self) -> Vec<String>;
}

impl GGSettings for UserSettings {
    fn query_log_page_size(&self) -> usize {
        self.config()
            .get_int("gg.queries.log-page-size")
            .unwrap_or(1000) as usize
    }

    fn query_large_repo_heuristic(&self) -> i64 {
        self.config()
            .get_int("gg.queries.large-repo-heuristic")
            .unwrap_or(100000)
    }

    fn query_auto_snapshot(&self) -> Option<bool> {
        self.config().get_bool("gg.queries.auto-snapshot").ok()
    }

    fn ui_theme_override(&self) -> Option<String> {
        self.config().get_string("gg.ui.theme-override").ok()
    }

    fn ui_mark_unpushed_bookmarks(&self) -> bool {
        self.config()
            .get_bool("gg.ui.mark-unpushed-bookmarks")
            .unwrap_or(
                self.config()
                    .get_bool("gg.ui.mark-unpushed-branches")
                    .unwrap_or(true),
            )
    }

    fn ui_recent_workspaces(&self) -> Vec<String> {
        let paths: Result<Vec<String>, ConfigError> = self
            .config()
            .get_array("gg.ui.recent-workspaces")
            .unwrap_or(vec![])
            .into_iter()
            .map(|value| value.into_string())
            .collect();
        paths.unwrap_or(vec![])
    }
}

pub fn read_config(repo_path: &Path) -> Result<(UserSettings, RevsetAliasesMap)> {
    let defaults = Config::builder()
        .add_source(jj_cli::config::default_config())
        .add_source(config::File::from_str(
            include_str!("../config/gg.toml"),
            config::FileFormat::Toml,
        ))
        .build()?;

    let mut configs = LayeredConfigs::from_environment(defaults);
    configs.read_user_config()?;
    configs.read_repo_config(repo_path)?;

    let settings = build_settings(&configs);
    let aliases_map = build_aliases_map(&configs)?;

    Ok((settings, aliases_map))
}

fn build_settings(configs: &LayeredConfigs) -> UserSettings {
    let config = configs.merge();
    UserSettings::from_config(config)
}

fn build_aliases_map(layered_configs: &LayeredConfigs) -> Result<RevsetAliasesMap> {
    const TABLE_KEY: &str = "revset-aliases";
    let mut aliases_map = RevsetAliasesMap::new();
    // Load from all config layers in order. 'f(x)' in default layer should be
    // overridden by 'f(a)' in user.
    for (_, config) in layered_configs.sources() {
        let table = if let Some(table) = config.get_table(TABLE_KEY).optional()? {
            table
        } else {
            continue;
        };
        for (decl, value) in table.into_iter().sorted_by(|a, b| a.0.cmp(&b.0)) {
            value
                .into_string()
                .map_err(|e| anyhow!(e))
                .and_then(|v| aliases_map.insert(&decl, v).map_err(|e| anyhow!(e)))?;
        }
    }
    Ok(aliases_map)
}
