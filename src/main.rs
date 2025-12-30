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
use jj_lib::settings::UserSettings;

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

        /// Port to bind to (0 = random).
        #[arg(short, long)]
        port: Option<u16>,

        /// Don't open a browser automatically.
        #[arg(long)]
        no_launch: bool,
    },
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

impl Args {
    fn mode(&self) -> Option<LaunchMode> {
        match &self.command {
            Some(Subcommand::Gui { .. }) => Some(LaunchMode::Gui),
            Some(Subcommand::Web { .. }) => Some(LaunchMode::Web),
            None => None,
        }
    }

    fn workspace(&self) -> Option<PathBuf> {
        match &self.command {
            Some(Subcommand::Gui { workspace }) | Some(Subcommand::Web { workspace, .. }) => {
                workspace.clone()
            }
            None => self.workspace.clone(),
        }
    }

    fn web_options(&self) -> web::WebOptions {
        match &self.command {
            Some(Subcommand::Web {
                port, no_launch, ..
            }) => web::WebOptions {
                port: *port,
                no_launch: *no_launch,
            },
            _ => web::WebOptions::default(),
        }
    }
}

pub struct RunOptions {
    pub context: tauri::Context<tauri::Wry>,
    pub settings: UserSettings,
    pub workspace: Option<PathBuf>,
    pub debug: bool,
    pub is_child: bool,
}

fn main() -> Result<()> {
    // may be executed as a git authenticator, which overrides everything else
    if let Some(result) = git_util::run_askpass() {
        return result;
    }

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
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio, exit};

    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(&exe);

    cmd.args(std::env::args().skip(1)); // forward all original arguments
    cmd.arg("--foreground");
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit()); // forward logs until startup is complete

    #[cfg(windows)]
    {
        use ::windows::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};
        use std::os::windows::process::CommandExt;

        // suppress console window
        cmd.creation_flags(DETACHED_PROCESS.0 | CREATE_NEW_PROCESS_GROUP.0);
    }

    // wait for startup (which is the only thing on stdout)
    match cmd.spawn() {
        Err(err) => Err(anyhow!("Startup error: {}", err)),
        Ok(mut child) => {
            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);
                let _ = reader.lines().next();
            }
            exit(0)
        }
    }
}

fn run_app(args: Args) -> Result<()> {
    let (settings, _) = read_config(args.workspace().as_deref())?;
    let mode = args.mode().unwrap_or_else(|| settings.default_mode());
    let context = tauri::generate_context!();

    // When spawned as a child process, foreground flag is set by the parent
    #[cfg(not(feature = "gui"))]
    let is_child = args.foreground;
    #[cfg(feature = "gui")]
    let is_child = false;

    let options = RunOptions {
        context,
        settings,
        workspace: args.workspace(),
        debug: args.debug,
        is_child,
    };

    match mode {
        LaunchMode::Gui => gui::run_gui(options),
        LaunchMode::Web => web::run_web(options, args.web_options()),
    }
}
