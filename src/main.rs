#![cfg_attr(feature = "app", windows_subsystem = "windows")]

mod gui;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use std::path::PathBuf;

#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::Parser;
use gg_cli::config::read_config;
use gg_cli::web;
use gg_cli::{RunOptions, askpass};
use jj_lib::settings::UserSettings;

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

        /// Open a browser automatically (overrides gg.web.launch-browser config).
        #[arg(long, conflicts_with = "no_launch")]
        launch: bool,

        /// Don't open a browser automatically (overrides gg.web.launch-browser config).
        #[arg(long, conflicts_with = "launch")]
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

    /// Start with immutability checks disabled.
    #[arg(long, global = true)]
    ignore_immutable: bool,

    #[cfg(not(feature = "app"))]
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
                port,
                launch,
                no_launch,
                ..
            }) => web::WebOptions {
                port: *port,
                launch: *launch,
                no_launch: *no_launch,
            },
            _ => web::WebOptions::default(),
        }
    }
}

enum LaunchMode {
    Gui,
    Web,
}

fn default_mode(settings: &UserSettings) -> LaunchMode {
    match settings.get_string("gg.default-mode").ok().as_deref() {
        Some("web") => LaunchMode::Web,
        _ => LaunchMode::Gui,
    }
}

fn main() -> Result<()> {
    // may be executed as a git authenticator, which overrides everything else
    if let Some(result) = askpass::run_askpass() {
        return result;
    }

    // reattach console on windows for CLI output (--help, errors, etc.)
    // only needed for "app" builds which have windows_subsystem = "windows"
    // non-app builds are console apps and already have a console
    #[cfg(all(windows, feature = "app"))]
    {
        windows::reattach_console();
    }

    let args = Args::parse();

    // cargo run/install: act like a CLI that spawns a GUI in the background
    #[cfg(not(feature = "app"))]
    if !args.foreground {
        spawn_app()
    } else {
        run_app(args)
    }

    #[cfg(feature = "app")]
    {
        run_app(args)
    }
}

#[cfg(not(feature = "app"))]
fn spawn_app() -> Result<()> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio, exit};

    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(&exe);

    cmd.args(std::env::args().skip(1)); // forward all original arguments
    cmd.arg("--foreground");
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit()); // forward logs until startup is complete

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // create pgroup so child survives SIGHUP
        cmd.process_group(0);
    }

    #[cfg(windows)]
    {
        use ::windows::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};
        use std::os::windows::process::CommandExt;

        // create pgroup and suppress console window
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP.0 | DETACHED_PROCESS.0);
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
    let (settings, _, _) = read_config(args.workspace().as_deref())?;
    let mode = args.mode().unwrap_or_else(|| default_mode(&settings));
    let context = tauri::generate_context!();

    // When spawned as a child process, foreground flag is set by the parent
    #[cfg(not(feature = "app"))]
    let is_child = args.foreground;
    #[cfg(feature = "app")]
    let is_child = false;

    let options = RunOptions {
        context,
        settings,
        workspace: args.workspace(),
        debug: args.debug,
        is_child,
        ignore_immutable: args.ignore_immutable,
        enable_askpass: true,
    };

    match mode {
        LaunchMode::Gui => gui::run_gui(options),
        LaunchMode::Web => web::run_web(options, args.web_options()),
    }
}
