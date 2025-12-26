#![cfg_attr(feature = "gui", windows_subsystem = "windows")]

mod config;
mod git_util;
mod gui;
#[cfg(all(target_os = "macos", not(feature = "gui")))]
mod macos;
mod messages;
mod web;
#[cfg(windows)]
mod windows;
mod worker;

use std::fmt::Display;
use std::path::PathBuf;

#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::{Parser, ValueEnum};
use config::{GGSettings, read_config};

#[derive(Clone, Debug, Default, PartialEq, Eq, ValueEnum)]
enum LaunchMode {
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

#[derive(Parser, Debug)]
#[command(version, author, args_conflicts_with_subcommands = true)]
struct Args {
    #[command(subcommand)]
    command: Option<Subcommand>,

    /// Open this directory (instead of the current working directory).
    #[arg(index(1))]
    workspace: Option<PathBuf>,

    /// Enable debug logging.
    #[arg(short, long, global = true)]
    debug: bool,

    #[cfg(not(feature = "gui"))]
    #[arg(
        long,
        global = true,
        help = "Run in foreground (don't spawn a background process).",
        hide = true
    )]
    foreground: bool,
}

#[derive(clap::Subcommand, Debug)]
enum Subcommand {
    /// Launch GG in GUI mode (default).
    Gui {
        /// Open this directory (instead of the current working directory).
        workspace: Option<PathBuf>,
    },
    /// Launch GG in web mode.
    Web {
        /// Open this directory (instead of the current working directory).
        workspace: Option<PathBuf>,
    },
}

impl Args {
    fn mode(&self) -> Option<LaunchMode> {
        match &self.command {
            Some(Subcommand::Gui { .. }) => Some(LaunchMode::Gui),
            Some(Subcommand::Web { .. }) => Some(LaunchMode::Web),
            _ => None,
        }
    }

    fn workspace(&self) -> Option<PathBuf> {
        match &self.command {
            Some(Subcommand::Gui { workspace }) | Some(Subcommand::Web { workspace }) => {
                workspace.clone()
            }
            None => self.workspace.clone(),
        }
    }
}

fn main() -> Result<()> {
    // before parsing args, attach a console on windows - will fail if not started from a shell, but that's fine
    #[cfg(windows)]
    {
        windows::reattach_console();
    }

    let args = Args::parse();

    // cargo run/install: act like a CLI that spawns a GUI in the background
    #[cfg(not(feature = "gui"))]
    if !args.foreground {
        spawn_app()
    } else {
        run_app(args)
    }

    #[cfg(feature = "gui")]
    {
        run_app(args)
    }
}

#[cfg(not(feature = "gui"))]
fn spawn_app() -> Result<()> {
    use std::process::{Command, exit};

    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(&exe);

    cmd.args(std::env::args().skip(1)); // forward all original arguments
    cmd.arg("--foreground");

    #[cfg(windows)]
    {
        use ::windows::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};
        use std::os::windows::process::CommandExt;

        // Spawn with DETACHED_PROCESS flag so the child runs independently
        cmd.creation_flags(DETACHED_PROCESS.0 | CREATE_NEW_PROCESS_GROUP.0);

        match cmd.spawn() {
            Err(err) => Err(anyhow!("Failed to spawn GG: {}", err)),
            Ok(_) => exit(0),
        }
    }

    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::process::CommandExt;

        // safety: fork() is ok here because:
        // 1. We're in a single-threaded context (early in main before any threads spawn)
        // 2. We only use async-signal-safe functions in the child before exec
        // 3. The child immediately execs, replacing itself entirely
        // 4. The parent exits without doing anything else
        unsafe {
            match libc::fork() {
                -1 => Err(anyhow!("fork() failed")),

                // child: detach from terminal, redirect stdio and exec
                0 => {
                    if libc::setsid() == -1 {
                        eprintln!("Warning: setsid() failed");
                    }

                    let devnull = CString::new("/dev/null")?;
                    let null_fd = libc::open(devnull.as_ptr(), libc::O_RDWR);
                    if null_fd != -1 {
                        if libc::dup2(null_fd, 0) == -1
                            || libc::dup2(null_fd, 1) == -1
                            || libc::dup2(null_fd, 2) == -1
                        {
                            eprintln!("Warning: failed to redirect stdio");
                        }
                        libc::close(null_fd);
                    }

                    let err = cmd.exec();
                    Err(anyhow!("exec() failed: {}", err))
                }

                // parent: we're done
                _ => exit(0),
            }
        }
    }
}

fn run_app(args: Args) -> Result<()> {
    let (settings, _) = read_config(args.workspace().as_deref())?;
    let mode = args.mode().unwrap_or_else(|| settings.default_mode());
    let context = tauri::generate_context!();

    match mode {
        LaunchMode::Gui => gui::run_gui(args.workspace(), args.debug, settings, context),
        LaunchMode::Web => web::run_web(args.workspace(), settings, context),
    }
}
