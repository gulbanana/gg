use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use anyhow::{Result, Context};

#[cfg(unix)]
use std::os::unix::fs as unix_fs;

/// Get the path where the .app bundle should be created
fn get_bundle_path() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    Ok(home.join("Applications").join("GG.app"))
}

/// Check if we're currently running from within the bundle
fn is_running_from_bundle() -> bool {
    if let Ok(exe_path) = std::env::current_exe() {
        // Check if path contains .app/Contents/MacOS/
        exe_path.to_string_lossy().contains(".app/Contents/MacOS/")
    } else {
        false
    }
}

/// Create the Info.plist content
fn create_info_plist(bundle_version: &str, executable_name: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>GG</string>
    <key>CFBundleExecutable</key>
    <string>{}</string>
    <key>CFBundleIconFile</key>
    <string>icon.icns</string>
    <key>CFBundleIdentifier</key>
    <string>au.gulbanana.gg</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>GG</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>{}</string>
    <key>CFBundleVersion</key>
    <string>{}</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>Copyright © 2024 Thomas Castiglione</string>
</dict>
</plist>
"#, executable_name, bundle_version, bundle_version)
}

/// Create or update the .app bundle
fn create_bundle() -> Result<PathBuf> {
    let bundle_path = get_bundle_path()?;
    let current_exe = std::env::current_exe()
        .context("Failed to get current executable path")?;
    
    // Create bundle directory structure
    let contents_dir = bundle_path.join("Contents");
    let macos_dir = contents_dir.join("MacOS");
    let resources_dir = contents_dir.join("Resources");
    
    fs::create_dir_all(&macos_dir)
        .context("Failed to create MacOS directory")?;
    fs::create_dir_all(&resources_dir)
        .context("Failed to create Resources directory")?;
    
    // Create Info.plist
    let version = env!("CARGO_PKG_VERSION");
    let plist_content = create_info_plist(version, "gg");
    fs::write(contents_dir.join("Info.plist"), plist_content)
        .context("Failed to write Info.plist")?;
    
    // Create symlink to the actual binary
    let bundle_exe = macos_dir.join("gg");
    
    // Remove existing symlink/file if it exists
    let _ = fs::remove_file(&bundle_exe);
    
    // Create symlink
    unix_fs::symlink(&current_exe, &bundle_exe)
        .context("Failed to create symlink to executable")?;
    
    // Try to copy icon if available
    // Look for icon in resources relative to the binary location
    if let Ok(exe_dir) = current_exe.parent()
        .and_then(|p| p.parent()) // Go up from bin/ to installation root
        .ok_or_else(|| anyhow::anyhow!("Could not determine executable directory"))
    {
        let possible_icon_paths = vec![
            exe_dir.join("res").join("icons").join("icon.icns"),
            exe_dir.join("share").join("gg").join("icons").join("icon.icns"),
        ];
        
        for icon_path in possible_icon_paths {
            if icon_path.exists() {
                let dest_icon = resources_dir.join("icon.icns");
                let _ = fs::copy(&icon_path, &dest_icon);
                break;
            }
        }
    }
    
    log::info!("Created app bundle at {}", bundle_path.display());
    
    Ok(bundle_path)
}

/// Launch the app via the bundle
fn launch_via_bundle(bundle_path: &Path) -> Result<()> {
    log::info!("Launching via bundle at {}", bundle_path.display());
    
    // Collect arguments to pass, but filter out --foreground if present
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|arg| arg != "--foreground" && arg != "-f")
        .collect();
    
    let mut cmd = Command::new("open");
    cmd.arg(bundle_path);
    
    // Pass arguments to the bundle via --args
    if !args.is_empty() {
        cmd.arg("--args");
        cmd.args(&args);
    }
    
    cmd.spawn()
        .context("Failed to launch bundle with 'open' command")?;
    
    Ok(())
}

/// Handle macOS-specific launch logic
/// Returns true if the app was re-launched via bundle (caller should exit)
pub fn handle_launch(foreground: bool) -> Result<bool> {
    // If already running from bundle, continue normally
    if is_running_from_bundle() {
        log::debug!("Already running from bundle");
        return Ok(false);
    }
    
    // If foreground mode, skip bundling
    if foreground {
        log::debug!("Foreground mode enabled, skipping bundle");
        return Ok(false);
    }
    
    // Create/update bundle and re-launch
    let bundle_path = create_bundle()?;
    launch_via_bundle(&bundle_path)?;
    
    // Signal to caller that we should exit
    Ok(true)
}
