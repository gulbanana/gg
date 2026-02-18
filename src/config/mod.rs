#[cfg(all(test, not(feature = "ts-rs")))]
pub mod tests;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use anyhow::{Result, anyhow};
use jj_cli::config::{ConfigEnv, config_from_environment, default_config_layers};
use jj_cli::ui::Ui;
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
        self.get_string("gg.ui.theme-override")
            .ok()
            .filter(|s| !s.is_empty())
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

/// Native GG keys that have dynamic defaults and can't appear in gg.toml.
const EXTRA_NATIVE_KEYS: &[&str] = &["queries.auto-snapshot", "ui.mark-unpushed-branches"];

/// Collect all leaf key paths from gg.toml under `[gg]` to identify native GG settings.
fn native_keys() -> HashSet<String> {
    fn collect_leaves(table: &toml_edit::Table, prefix: &str, keys: &mut HashSet<String>) {
        for (key, item) in table.iter() {
            let path = if prefix.is_empty() {
                key.to_string()
            } else {
                format!("{prefix}.{key}")
            };
            if let Some(sub) = item.as_table() {
                collect_leaves(sub, &path, keys);
            } else {
                keys.insert(path);
            }
        }
    }

    let doc: toml_edit::DocumentMut = include_str!("../config/gg.toml")
        .parse()
        .expect("bundled gg.toml is valid TOML");
    let mut keys = HashSet::new();
    if let Some(gg) = doc.get("gg").and_then(|v| v.as_table()) {
        collect_leaves(gg, "", &mut keys);
    }
    for extra in EXTRA_NATIVE_KEYS {
        keys.insert(extra.to_string());
    }
    keys
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
        config_env
            .reload_repo_config(&Ui::null(), &mut raw_config)
            .map_err(|e| anyhow!("{e:?}"))?;
    }

    let mut config = config_env.resolve_config(&raw_config)?;
    let aliases_map = build_aliases_map(&config)?;
    let query_choices = read_preset_choices(&config);

    if let Some(overrides) = extract_overrides(&config) {
        config.add_layer(overrides);
    }

    let workspace_settings = UserSettings::from_config(config)?;

    Ok((workspace_settings, aliases_map, query_choices))
}

/// Scans all config layers for `gg.*` keys that aren't GG's own settings and
/// collects them into a high-priority layer with the `gg.` prefix stripped.
/// This lets users write `gg.user.name = "bar"` to override `user.name` only
/// within GG, without affecting the jj CLI.
fn extract_overrides(config: &StackedConfig) -> Option<ConfigLayer> {
    let gg_table_name = ConfigNamePathBuf::from_iter(["gg"]);
    let native_keys = native_keys();
    let mut doc = toml_edit::DocumentMut::new();
    let mut has_overrides = false;

    // iterate low→high priority so later layers naturally overwrite earlier ones
    for layer in config.layers() {
        let table = match layer.look_up_table(&gg_table_name) {
            Ok(Some(table)) => table,
            _ => continue,
        };
        collect_override_entries(
            table,
            &mut String::new(),
            &native_keys,
            &mut doc,
            &mut has_overrides,
        );
    }

    has_overrides.then(|| ConfigLayer::with_data(ConfigSource::CommandArg, doc))
}

/// Recursively walk a table under `gg`, classifying each leaf as native or override.
fn collect_override_entries(
    table: &dyn toml_edit::TableLike,
    prefix: &mut String,
    native_keys: &HashSet<String>,
    doc: &mut toml_edit::DocumentMut,
    has_overrides: &mut bool,
) {
    for (key, item) in table.iter() {
        let path_start = prefix.len();
        if !prefix.is_empty() {
            prefix.push('.');
        }
        prefix.push_str(key);

        if let Some(sub) = item.as_table() {
            collect_override_entries(sub, prefix, native_keys, doc, has_overrides);
        } else if !is_native_key(prefix, native_keys) {
            set_in_doc(doc, prefix, item);
            *has_overrides = true;
        }

        prefix.truncate(path_start);
    }
}

/// Returns true if a key path (relative to `gg.`) is a native GG setting.
fn is_native_key(path: &str, native_keys: &HashSet<String>) -> bool {
    path.starts_with("presets.") || native_keys.contains(path)
}

/// Ensure parent tables exist in the override doc and set the leaf value.
fn set_in_doc(doc: &mut toml_edit::DocumentMut, path: &str, item: &toml_edit::Item) {
    let parts: Vec<&str> = path.split('.').collect();
    let (parents, leaf) = parts.split_at(parts.len() - 1);
    let mut table = doc.as_table_mut();
    for &part in parents {
        if !table.contains_key(part) {
            table.insert(part, toml_edit::Item::Table(toml_edit::Table::new()));
        }
        table = table[part]
            .as_table_mut()
            .expect("parent path should be a table");
    }
    table.insert(leaf[0], item.clone());
}

pub fn read_preset_choices(stacked_config: &StackedConfig) -> HashMap<String, String> {
    let table_name = ConfigNamePathBuf::from_iter(["gg", "presets"]);
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
