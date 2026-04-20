use super::*;

#[test]
fn parse_default_theme() {
    let theme = default_theme();
    // light variant has expected tokens
    assert_eq!(theme.light.get("colors-primary"), Some(&"#007AFF".to_string()));
    assert_eq!(theme.light.get("colors-background"), Some(&"#ffffff".to_string()));
    assert!(theme.light.contains_key("text-familyUi"));
    assert!(theme.light.contains_key("shadows-shadowSm"));
    assert!(theme.light.contains_key("components-buttonHeight"));

    // dark variant has expected tokens
    assert_eq!(theme.dark.get("colors-primary"), Some(&"#8aadf4".to_string()));
    assert!(theme.dark.contains_key("components-buttonBackground"));
}

#[test]
fn resolve_references() {
    let theme = default_theme();
    // components.buttonBackground should resolve to colors.primary
    assert_eq!(
        theme.light.get("components-buttonBackground"),
        theme.light.get("colors-primary"),
    );
    // components.buttonForeground should resolve to colors.primaryContent
    assert_eq!(
        theme.light.get("components-buttonForeground"),
        theme.light.get("colors-primaryContent"),
    );
}

#[test]
fn resolve_cross_section_refs() {
    let theme = default_theme();
    // components.buttonShadow references shadows.shadowSm
    assert_eq!(
        theme.light.get("components-buttonShadow"),
        theme.light.get("shadows-shadowSm"),
    );
    // components.buttonFont references text.familyUi
    assert_eq!(
        theme.light.get("components-buttonFont"),
        theme.light.get("text-familyUi"),
    );
}

#[test]
fn unresolved_ref_kept_literally() {
    let toml = r##"
[light.colors]
primary = "#ff0000"

[light.text]
[light.shadows]

[light.components]
buttonBackground = "$colors.nonexistent"

[dark.colors]
primary = "#ff0000"
[dark.text]
[dark.shadows]
[dark.components]
"##;
    let theme = parse_theme(toml).unwrap();
    assert_eq!(
        theme.light.get("components-buttonBackground"),
        Some(&"$colors.nonexistent".to_string()),
    );
}

#[test]
fn parse_all_bundled_themes() {
    // all bundled themes should parse without error
    parse_theme(include_str!("../../res/themes/default.toml")).unwrap();
    parse_theme(include_str!("../../res/themes/macos.toml")).unwrap();
    parse_theme(include_str!("../../res/themes/material.toml")).unwrap();
    parse_theme(include_str!("../../res/themes/fluent.toml")).unwrap();
}

#[test]
fn five_surface_tiers() {
    let theme = default_theme();
    // verify all 5 surface tiers exist
    assert!(theme.light.contains_key("colors-background"));
    assert!(theme.light.contains_key("colors-surfaceDeep"));
    assert!(theme.light.contains_key("colors-surface"));
    assert!(theme.light.contains_key("colors-surfaceAlt"));
    assert!(theme.light.contains_key("colors-surfaceStrong"));
}

#[test]
fn graph_node_tokens() {
    let theme = default_theme();
    assert!(theme.light.contains_key("colors-immutableStroke"));
    assert!(theme.light.contains_key("colors-immutableFill"));
    assert!(theme.light.contains_key("colors-mutableStroke"));
    assert!(theme.light.contains_key("colors-mutableFill"));
    assert!(theme.light.contains_key("colors-workingCopyStroke"));
    assert!(theme.light.contains_key("colors-workingCopyFill"));
    assert!(theme.light.contains_key("colors-conflictStroke"));
    assert!(theme.light.contains_key("colors-conflictFill"));
}
