#![cfg_attr(feature = "gui", windows_subsystem = "windows")]

mod config;
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
            Some(Subcommand::Gui { workspace }) | Some(Subcommand::Web { workspace }) => {
                workspace.clone()
            }
            None => self.workspace.clone(),
        }
    }
}

struct RunOptions {
    context: tauri::Context<tauri::Wry>,
    settings: UserSettings,
    workspace: Option<PathBuf>,
    debug: bool,
    is_child: bool,
}

fn main() -> Result<()> {
    // before parsing args, attach a console on windows - will fail if not started from a shell, but that's fine
    #[cfg(windows)]
    {
        windows::reattach_console();
    }

    // Detect askpass mode BEFORE parsing args.
    // When git runs GIT_ASKPASS, it calls: /path/to/gg "prompt"
    // Without this check, the prompt would be interpreted as a workspace path.
    if std::env::var("GG_ASKPASS_SOCKET").is_ok() {
        // We're being called as an askpass helper - the first arg is the prompt
        let prompt = std::env::args().nth(1).unwrap_or_default();
        return run_askpass(&prompt);
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
        LaunchMode::Web => web::run_web(options),
    }
}

/// Handle askpass requests from git/ssh subprocesses.
/// Connects to the IPC socket specified in GG_ASKPASS_SOCKET,
/// sends the prompt, and prints the credential to stdout.
fn run_askpass(prompt: &str) -> Result<()> {
    use interprocess::local_socket::{GenericFilePath, Stream, ToFsName, traits::Stream as _};
    use std::io::{BufRead, BufReader, Write};

    let socket_path = std::env::var("GG_ASKPASS_SOCKET")
        .map_err(|_| anyhow!("GG_ASKPASS_SOCKET not set"))?;

    let name = socket_path
        .to_fs_name::<GenericFilePath>()
        .map_err(|e| anyhow!("invalid socket path: {}", e))?;

    let stream =
        Stream::connect(name).map_err(|e| anyhow!("failed to connect to askpass socket: {}", e))?;

    // Send prompt (newline-terminated)
    let mut writer = &stream;
    writeln!(writer, "{}", prompt)?;
    writer.flush()?;

    // Read response
    let mut response = String::new();
    BufReader::new(&stream).read_line(&mut response)?;
    let response = response.trim();

    if let Some(credential) = response.strip_prefix("OK:") {
        // Print credential to stdout (git reads this)
        println!("{}", credential);
        Ok(())
    } else {
        // "UNAVAILABLE" or error - exit non-zero to signal auth failure
        Err(anyhow!("credential unavailable"))
    }
}
