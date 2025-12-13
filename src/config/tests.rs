use super::*;
use jj_lib::config::ConfigLayer;

/// Create isolated test settings with only gg.toml defaults, no user config.
fn settings_with_gg_defaults() -> UserSettings {
    let mut config = StackedConfig::empty();

    let jj_defaults = ConfigLayer::parse(
        ConfigSource::Default,
        r#"
            [user]
            name = "Test User"
            email = "test@example.com"

            [operation]
            hostname = "test-host"
            username = "test-user"

            [signing]
            behavior = "drop"
            "#,
    )
    .unwrap();
    config.add_layer(jj_defaults);

    // gg defaults from gg.toml
    let gg_layer =
        ConfigLayer::parse(ConfigSource::Default, include_str!("../config/gg.toml")).unwrap();
    config.add_layer(gg_layer);

    UserSettings::from_config(config).unwrap()
}

/// Create test settings with custom TOML layered on top of defaults.
fn settings_with_overrides(toml: &str) -> UserSettings {
    let mut config = StackedConfig::empty();

    let jj_defaults = ConfigLayer::parse(
        ConfigSource::Default,
        r#"
            [user]
            name = "Test User"
            email = "test@example.com"

            [operation]
            hostname = "test-host"
            username = "test-user"

            [signing]
            behavior = "drop"
            "#,
    )
    .unwrap();
    config.add_layer(jj_defaults);

    let gg_layer =
        ConfigLayer::parse(ConfigSource::Default, include_str!("../config/gg.toml")).unwrap();
    config.add_layer(gg_layer);

    let test_layer = ConfigLayer::parse(ConfigSource::User, toml).unwrap();
    config.add_layer(test_layer);

    UserSettings::from_config(config).unwrap()
}

#[test]
fn track_recent_workspaces_default_is_true() {
    let settings = settings_with_gg_defaults();
    assert!(settings.ui_track_recent_workspaces());
}

#[test]
fn track_recent_workspaces_can_be_disabled() {
    let settings = settings_with_overrides(
        r#"
            [gg.ui]
            track-recent-workspaces = false
            "#,
    );
    assert!(!settings.ui_track_recent_workspaces());
}

#[test]
fn mark_unpushed_bookmarks_default_is_true() {
    let settings = settings_with_gg_defaults();
    assert!(settings.ui_mark_unpushed_bookmarks());
}

#[test]
fn mark_unpushed_bookmarks_can_be_disabled() {
    let settings = settings_with_overrides(
        r#"
            [gg.ui]
            mark-unpushed-bookmarks = false
            "#,
    );
    assert!(!settings.ui_mark_unpushed_bookmarks());
}

#[test]
fn mark_unpushed_bookmarks_legacy_name_fallback() {
    // Test that the legacy name works when the new name is not configured.
    // We create settings without gg.toml to test the fallback behavior.
    let mut config = StackedConfig::empty();

    let jj_defaults = ConfigLayer::parse(
        ConfigSource::Default,
        r#"
            [user]
            name = "Test User"
            email = "test@example.com"

            [operation]
            hostname = "test-host"
            username = "test-user"

            [signing]
            behavior = "drop"

            [gg.ui]
            mark-unpushed-branches = false
            "#,
    )
    .unwrap();
    config.add_layer(jj_defaults);

    let settings = UserSettings::from_config(config).unwrap();
    // Legacy name should be respected when new name is not set
    assert!(!settings.ui_mark_unpushed_bookmarks());
}

#[test]
fn mark_unpushed_bookmarks_new_name_takes_precedence() {
    let settings = settings_with_overrides(
        r#"
            [gg.ui]
            mark-unpushed-branches = false
            mark-unpushed-bookmarks = true
            "#,
    );
    assert!(settings.ui_mark_unpushed_bookmarks());
}

#[test]
fn log_page_size_default() {
    let settings = settings_with_gg_defaults();
    assert_eq!(settings.query_log_page_size(), 1000);
}

#[test]
fn log_page_size_can_be_overridden() {
    let settings = settings_with_overrides(
        r#"
            [gg.queries]
            log-page-size = 500
            "#,
    );
    assert_eq!(settings.query_log_page_size(), 500);
}

#[test]
fn large_repo_heuristic_default() {
    let settings = settings_with_gg_defaults();
    assert_eq!(settings.query_large_repo_heuristic(), 100000);
}

#[test]
fn auto_snapshot_default_is_none() {
    let settings = settings_with_gg_defaults();
    assert!(settings.query_auto_snapshot().is_none());
}

#[test]
fn auto_snapshot_can_be_enabled() {
    let settings = settings_with_overrides(
        r#"
            [gg.queries]
            auto-snapshot = true
            "#,
    );
    assert_eq!(settings.query_auto_snapshot(), Some(true));
}

#[test]
fn theme_override_default_is_none() {
    let settings = settings_with_gg_defaults();
    assert!(settings.ui_theme_override().is_none());
}

#[test]
fn theme_override_can_be_set() {
    let settings = settings_with_overrides(
        r#"
            [gg.ui]
            theme-override = "dark"
            "#,
    );
    assert_eq!(settings.ui_theme_override(), Some("dark".to_string()));
}

#[test]
fn recent_workspaces_default_is_empty() {
    let settings = settings_with_gg_defaults();
    assert!(settings.ui_recent_workspaces().is_empty());
}

#[test]
fn recent_workspaces_returns_configured_paths() {
    let settings = settings_with_overrides(
        r#"
            [gg.ui]
            recent-workspaces = ["/path/one", "/path/two"]
            "#,
    );
    assert_eq!(
        settings.ui_recent_workspaces(),
        vec!["/path/one".to_string(), "/path/two".to_string()]
    );
}
