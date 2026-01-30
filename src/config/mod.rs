#[cfg(all(test, not(feature = "ts-rs")))]
pub mod tests;

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{Result, anyhow};
use jj_cli::config::{ConfigEnv, config_from_environment, default_config_layers};
use jj_lib::{
    config::{ConfigGetError, ConfigLayer, ConfigNamePathBuf, ConfigSource, StackedConfig},
    revset::RevsetAliasesMap,
    settings::UserSettings,
};

use crate::LaunchMode;

pub trait GGSettings {
    fn default_mode(&self) -> LaunchMode;
    fn query_log_page_size(&self) -> usize;
    fn query_large_repo_heuristic(&self) -> i64;
    fn query_auto_snapshot(&self) -> Option<bool>;
    fn ui_theme_override(&self) -> Option<String>;
    fn ui_mark_unpushed_bookmarks(&self) -> bool;
    fn ui_track_recent_workspaces(&self) -> bool;
    #[allow(dead_code)]
    fn ui_recent_workspaces(&self) -> Vec<String>;
    fn web_default_port(&self) -> u16;
    fn web_client_timeout(&self) -> Duration;
    fn web_launch_browser(&self) -> bool;
}

impl GGSettings for UserSettings {
    fn default_mode(&self) -> LaunchMode {
        match self.get_string("gg.default-mode").ok().as_deref() {
            Some("gui") => LaunchMode::Gui,
            Some("web") => LaunchMode::Web,
            _ => LaunchMode::Gui,
        }
    }

    fn query_log_page_size(&self) -> usize {
        self.get_int("gg.queries.log-page-size").unwrap_or(1000) as usize
    }

    fn query_large_repo_heuristic(&self) -> i64 {
        self.get_int("gg.queries.large-repo-heuristic")
            .unwrap_or(100000)
    }

    fn query_auto_snapshot(&self) -> Option<bool> {
        self.get_bool("gg.queries.auto-snapshot").ok()
    }

    fn ui_theme_override(&self) -> Option<String> {
        self.get_string("gg.ui.theme-override").ok()
    }

    fn ui_mark_unpushed_bookmarks(&self) -> bool {
        self.get_bool("gg.ui.mark-unpushed-bookmarks").unwrap_or(
            self.get_bool("gg.ui.mark-unpushed-branches")
                .unwrap_or(true),
        )
    }

    fn ui_track_recent_workspaces(&self) -> bool {
        self.get_bool("gg.ui.track-recent-workspaces")
            .unwrap_or(true)
    }

    fn ui_recent_workspaces(&self) -> Vec<String> {
        self.get_value("gg.ui.recent-workspaces")
            .ok()
            .and_then(|v| v.as_array().cloned())
            .map(|values| {
                values
                    .into_iter()
                    .filter_map(|value| value.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn web_default_port(&self) -> u16 {
        self.get_int("gg.web.default-port").unwrap_or(0) as u16
    }

    fn web_client_timeout(&self) -> Duration {
        self.get_string("gg.web.client-timeout")
            .ok()
            .and_then(|s| humantime::parse_duration(&s).ok())
            .unwrap_or(Duration::from_secs(600))
    }

    fn web_launch_browser(&self) -> bool {
        self.get_bool("gg.web.launch-browser").unwrap_or(true)
    }
}

pub fn read_config(
    repo_path: Option<&Path>,
) -> Result<(UserSettings, RevsetAliasesMap, HashMap<String, String>)> {
    let mut layers = vec![];
    let mut config_env = ConfigEnv::from_environment();

    let default_layers = default_config_layers();
    let gg_layer = ConfigLayer::parse(ConfigSource::Default, include_str!("../config/gg.toml"))?;
    layers.extend(default_layers);
    layers.push(gg_layer);

    let mut raw_config = config_from_environment(layers);
    config_env.reload_user_config(&mut raw_config)?;
    if let Some(repo_path) = repo_path {
        config_env.reset_repo_path(repo_path);
        config_env.reload_repo_config(&mut raw_config)?;
    }

    let config = config_env.resolve_config(&raw_config)?;
    let aliases_map = build_aliases_map(&config)?;
    let query_choices = read_revset_query_choices(&config);
    let workspace_settings = UserSettings::from_config(config)?;

    Ok((workspace_settings, aliases_map, query_choices))
}

pub fn read_revset_query_choices(stacked_config: &StackedConfig) -> HashMap<String, String> {
    let table_name = ConfigNamePathBuf::from_iter(["gg", "revsets"]);
    let mut choices = HashMap::new();

    for layer in stacked_config.layers() {
        let table = match layer.look_up_table(&table_name) {
            Ok(Some(table)) => table,
            Ok(None) => continue,
            Err(_) => continue,
        };
        for (key, item) in table.iter() {
            if let Some(value) = item.as_str() {
                choices.insert(key.to_string(), value.to_string());
            }
        }
    }
    choices
}

pub fn build_aliases_map(stacked_config: &StackedConfig) -> Result<RevsetAliasesMap> {
    let table_name = ConfigNamePathBuf::from_iter(["revset-aliases"]);
    let mut aliases_map = RevsetAliasesMap::new();
    // Load from all config layers in order. 'f(x)' in default layer should be
    // overridden by 'f(a)' in user.
    for layer in stacked_config.layers() {
        let table = match layer.look_up_table(&table_name) {
            Ok(Some(table)) => table,
            Ok(None) => continue,
            Err(item) => {
                return Err(ConfigGetError::Type {
                    name: table_name.to_string(),
                    error: format!("Expected a table, but is {}", item.type_name()).into(),
                    source_path: layer.path.clone(),
                }
                .into());
            }
        };
        for (decl, item) in table.iter() {
            let r = item
                .as_str()
                .ok_or_else(|| format!("Expected a string, but is {}", item.type_name()))
                .and_then(|v| aliases_map.insert(decl, v).map_err(|e| e.to_string()));
            if let Err(s) = r {
                return Err(anyhow!("Failed to load `{table_name}.{decl}`: {s}"));
            }
        }
    }
    Ok(aliases_map)
}
