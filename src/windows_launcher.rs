use std::process::Command;
use anyhow::{Result, Context};

/// Launch the GUI binary in the background
pub fn launch_gui_background() -> Result<()> {
    let current_exe = std::env::current_exe()
        .context("Failed to get current executable path")?;
    
    // Determine the GUI executable path
    // If we're gg.exe, look for gg-gui.exe in the same directory
    let gui_exe = if let Some(parent) = current_exe.parent() {
        parent.join("gg-gui.exe")
    } else {
        return Err(anyhow::anyhow!("Could not determine executable directory"));
    };
    
    if !gui_exe.exists() {
        return Err(anyhow::anyhow!(
            "GUI executable not found at {}. Make sure gg-gui.exe is in the same directory as gg.exe",
            gui_exe.display()
        ));
    }
    
    // Collect arguments to pass to GUI
    let args: Vec<String> = std::env::args().skip(1).collect();
    
    log::info!("Launching GUI in background: {}", gui_exe.display());
    
    // Launch with CREATE_NO_WINDOW flag via Windows API
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const DETACHED_PROCESS: u32 = 0x00000008;
        
        Command::new(&gui_exe)
            .args(&args)
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .context("Failed to launch GUI binary")?;
    }
    
    #[cfg(not(windows))]
    {
        // Fallback for non-Windows (shouldn't be called)
        Command::new(&gui_exe)
            .args(&args)
            .spawn()
            .context("Failed to launch GUI binary")?;
    }
    
    Ok(())
}

/// Handle Windows-specific launch logic
/// Returns true if GUI was launched in background (caller should exit)
/// foreground parameter: if true, don't launch background GUI
pub fn handle_launch(foreground: bool) -> Result<bool> {
    // Determine if we're the launcher (gg.exe) or the GUI (gg-gui.exe)
    let current_exe = std::env::current_exe()
        .context("Failed to get current executable path")?;
    
    let is_launcher = current_exe
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n == "gg.exe")
        .unwrap_or(false);
    
    // If we're the GUI binary, just continue normally
    if !is_launcher {
        log::debug!("Running as GUI binary");
        return Ok(false);
    }
    
    // If foreground mode, continue as GUI in this process
    if foreground {
        log::debug!("Foreground mode enabled, running GUI in launcher process");
        return Ok(false);
    }
    
    // Launch GUI in background and exit
    log::debug!("Launching GUI in background");
    launch_gui_background()?;
    Ok(true)
}
