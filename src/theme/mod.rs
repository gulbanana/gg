//! Theme loading and token resolution.
//!
//! A theme is a TOML file with `[light.*]` and `[dark.*]` sections, each
//! containing `colors`, `text`, `shadows`, and `components` tables. Values
//! in later sections can reference earlier ones with `$section.key` syntax
//! (e.g. `$colors.primary`), resolved at load time in a single pass.

#[cfg(all(test, not(feature = "ts-rs")))]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use log::warn;

const SECTIONS: &[&str] = &["colors", "text", "shadows", "components"];

/// Resolved theme tokens for one variant (light or dark), keyed by
/// `"section-tokenName"` → CSS value.
pub type ThemeTokens = HashMap<String, String>;

/// Both variants of a parsed theme.
pub struct Theme {
    pub light: ThemeTokens,
    pub dark: ThemeTokens,
}

/// Load and resolve a theme from a TOML string.
pub fn parse_theme(toml_str: &str) -> Result<Theme> {
    let doc: toml_edit::DocumentMut = toml_str.parse().context("invalid theme TOML")?;

    let light = resolve_variant(&doc, "light")?;
    let dark = resolve_variant(&doc, "dark")?;

    Ok(Theme { light, dark })
}

/// Load a theme from a file path.
pub fn load_theme(path: &Path) -> Result<Theme> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read theme file: {}", path.display()))?;
    parse_theme(&content)
}

/// The bundled default theme.
pub fn default_theme() -> Theme {
    parse_theme(include_str!("../../res/themes/default.toml"))
        .expect("bundled default theme is valid")
}

const BUNDLED_THEMES: &[(&str, &str)] = &[
    ("default.toml", include_str!("../../res/themes/default.toml")),
    ("macos.toml", include_str!("../../res/themes/macos.toml")),
    ("material.toml", include_str!("../../res/themes/material.toml")),
    ("fluent.toml", include_str!("../../res/themes/fluent.toml")),
];

/// Returns the path to the themes directory (`~/.config/gg/themes/`).
pub fn themes_dir() -> Option<PathBuf> {
    let home = etcetera::home_dir().ok()?;
    Some(home.join(".config").join("gg").join("themes"))
}

/// Load the theme from user settings, falling back to the bundled default.
pub fn load_from_settings(settings: &dyn crate::config::GGSettings) -> Theme {
    if let Some(path) = settings.ui_theme_file() {
        let expanded = expand_home(&path);
        match load_theme(&expanded) {
            Ok(t) => return t,
            Err(e) => warn!("failed to load theme file {path}: {e:#}"),
        }
    }
    default_theme()
}

/// Generate CSS rules that set `--gg-*` custom properties on `#shell` for
/// both light and dark variants (using `prefers-color-scheme`).
/// When `theme_override` is set, only that variant is emitted.
pub fn generate_css(settings: &dyn crate::config::GGSettings) -> String {
    let theme = load_from_settings(settings);
    let override_pref = settings.ui_theme_override();

    let mut css = String::new();

    let emit_variant = |tokens: &ThemeTokens, scheme: &str, css: &mut String| {
        for (key, value) in tokens {
            css.push_str(&format!("  --gg-{key}: {value};\n"));
        }
        css.push_str(&format!("  color-scheme: {scheme};\n"));
    };

    match override_pref.as_deref() {
        Some("dark") => {
            css.push_str("#shell {\n");
            emit_variant(&theme.dark, "dark", &mut css);
            css.push_str("}\n");
        }
        Some("light") => {
            css.push_str("#shell {\n");
            emit_variant(&theme.light, "light", &mut css);
            css.push_str("}\n");
        }
        _ => {
            // light by default, dark via media query
            css.push_str("#shell {\n");
            emit_variant(&theme.light, "light", &mut css);
            css.push_str("}\n");
            css.push_str("@media (prefers-color-scheme: dark) {\n");
            css.push_str("  #shell {\n");
            for (key, value) in &theme.dark {
                css.push_str(&format!("    --gg-{key}: {value};\n"));
            }
            css.push_str("    color-scheme: dark;\n");
            css.push_str("  }\n");
            css.push_str("}\n");
        }
    }

    css
}

/// Wrap [`generate_css`] output in a `<style>` element for HTML injection.
pub fn generate_style_element(settings: &dyn crate::config::GGSettings) -> String {
    format!(
        "<style id=\"gg-theme\">\n{}</style>",
        generate_css(settings)
    )
}

/// Expand a leading `~` to the user's home directory.
fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = etcetera::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

/// Write bundled theme files to `~/.config/gg/themes/` if the directory
/// doesn't exist yet. Existing files are never overwritten.
pub fn ensure_bundled_themes() {
    let Some(dir) = themes_dir() else {
        return;
    };

    if dir.exists() {
        return;
    }

    if let Err(e) = std::fs::create_dir_all(&dir) {
        warn!("failed to create themes directory {}: {e}", dir.display());
        return;
    }

    for (name, content) in BUNDLED_THEMES {
        let path = dir.join(name);
        if !path.exists() {
            if let Err(e) = std::fs::write(&path, content) {
                warn!("failed to write bundled theme {}: {e}", path.display());
            }
        }
    }
}

fn resolve_variant(doc: &toml_edit::DocumentMut, variant: &str) -> Result<ThemeTokens> {
    let variant_table = doc
        .get(variant)
        .and_then(|v| v.as_table())
        .with_context(|| format!("missing [{variant}] section in theme"))?;

    let mut tokens = ThemeTokens::new();

    for &section in SECTIONS {
        let table = match variant_table.get(section).and_then(|v| v.as_table()) {
            Some(t) => t,
            None => continue,
        };

        for (key, item) in table.iter() {
            if let Some(value) = item.as_str() {
                let resolved = resolve_refs(value, &tokens);
                tokens.insert(format!("{section}-{key}"), resolved);
            }
        }
    }

    Ok(tokens)
}

/// Replace `$section.key` references with their resolved values.
/// Single-pass: only references to already-resolved tokens work.
fn resolve_refs(value: &str, tokens: &ThemeTokens) -> String {
    if !value.contains('$') {
        return value.to_string();
    }

    let mut result = String::with_capacity(value.len());
    let mut rest = value;

    while let Some(dollar_pos) = rest.find('$') {
        result.push_str(&rest[..dollar_pos]);
        rest = &rest[dollar_pos + 1..];

        // extract section.key
        if let Some(ref_end) = rest.find(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_') {
            let ref_name = &rest[..ref_end];
            if let Some(resolved) = lookup_ref(ref_name, tokens) {
                result.push_str(&resolved);
            } else {
                warn!("unresolved theme reference: ${ref_name}");
                result.push('$');
                result.push_str(ref_name);
            }
            rest = &rest[ref_end..];
        } else {
            // ref extends to end of string
            let ref_name = rest;
            if let Some(resolved) = lookup_ref(ref_name, tokens) {
                result.push_str(&resolved);
            } else {
                warn!("unresolved theme reference: ${ref_name}");
                result.push('$');
                result.push_str(ref_name);
            }
            rest = "";
        }
    }

    result.push_str(rest);
    result
}

/// Look up `"section.key"` → `"section-key"` in the tokens map.
fn lookup_ref(ref_name: &str, tokens: &ThemeTokens) -> Option<String> {
    // ref_name is "colors.primary" → token key is "colors-primary"
    let key = ref_name.replacen('.', "-", 1);
    tokens.get(&key).cloned()
}
