use std::path::Path;

use anyhow::{Result, anyhow};
use jj_cli::{
    config::{ConfigEnv, config_from_environment, default_config_layers},
    ui::Ui,
};
use jj_lib::{
    config::{ConfigGetError, ConfigLayer, ConfigNamePathBuf, ConfigSource, StackedConfig},
    revset::RevsetAliasesMap,
    settings::UserSettings,
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
}

pub fn read_config(repo_path: Option<&Path>) -> Result<(UserSettings, RevsetAliasesMap)> {
    let mut layers = vec![];
    let mut config_env = ConfigEnv::from_environment(&Ui::null());

    let jj_default_layers = default_config_layers();
    let gg_default_layer =
        ConfigLayer::parse(ConfigSource::Default, include_str!("../config/gg.toml"))?;
    layers.extend(jj_default_layers);
    layers.push(gg_default_layer);
    
    let mut raw_config = config_from_environment(layers);
    config_env.reload_user_config(&mut raw_config)?;
    if let Some(repo_path) = repo_path {
        config_env.reset_repo_path(repo_path);
        config_env.reload_repo_config(&mut raw_config)?;
    }

    let config = config_env.resolve_config(&raw_config)?;
    let aliases_map = build_aliases_map(&config)?;
    let settings = UserSettings::from_config(config)?;

    Ok((settings, aliases_map))
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
