#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;
use std::env;
use std::process;

#[derive(Parser, Debug)]
#[command(version, author)]
struct Args {
    #[arg(
        index(1),
        help = "Open this directory (instead of the current working directory)."
    )]
    workspace: Option<std::path::PathBuf>,
    
    #[arg(short, long, help = "Enable debug logging.")]
    debug: bool,
    
    #[arg(long, help = "Run in foreground (blocks the shell). Internal use only.", hide = true)]
    foreground: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    // If --foreground is set, run the GUI directly
    if args.foreground {
        let gg_args = gg::Args {
            workspace: args.workspace,
            debug: args.debug,
        };
        return gg::run(gg_args);
    }
    
    // Otherwise, spawn the GUI in the background and exit
    spawn_in_background(args)
}

#[cfg(unix)]
fn spawn_in_background(args: Args) -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt;
    
    // Get the path to the current executable
    let exe = env::current_exe()?;
    
    // Build command to re-exec ourselves with --foreground
    let mut cmd = process::Command::new(&exe);
    cmd.arg("--foreground");
    
    if let Some(workspace) = args.workspace {
        cmd.arg(workspace);
    }
    if args.debug {
        cmd.arg("--debug");
    }
    
    // On Unix, we fork and let the parent exit, child continues in background
    unsafe {
        use std::ffi::CString;
        
        match libc::fork() {
            -1 => {
                return Err(anyhow::anyhow!("Failed to fork"));
            }
            0 => {
                // Child process: create new session and exec
                libc::setsid();
                
                // Redirect stdin/stdout/stderr to /dev/null
                let devnull = CString::new("/dev/null").unwrap();
                let null_fd = libc::open(devnull.as_ptr(), libc::O_RDWR);
                if null_fd != -1 {
                    libc::dup2(null_fd, 0);
                    libc::dup2(null_fd, 1);
                    libc::dup2(null_fd, 2);
                    libc::close(null_fd);
                }
                
                // Execute the command
                let err = cmd.exec();
                eprintln!("Failed to exec: {}", err);
                process::exit(1);
            }
            _ => {
                // Parent process: exit successfully
                process::exit(0);
            }
        }
    }
}

#[cfg(windows)]
fn spawn_in_background(args: Args) -> anyhow::Result<()> {
    use std::os::windows::process::CommandExt;
    
    // Get the path to the current executable
    let exe = env::current_exe()?;
    
    // Build command to re-exec ourselves with --foreground
    let mut cmd = process::Command::new(&exe);
    cmd.arg("--foreground");
    
    if let Some(workspace) = args.workspace {
        cmd.arg(workspace);
    }
    if args.debug {
        cmd.arg("--debug");
    }
    
    // On Windows, spawn with DETACHED_PROCESS flag
    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    
    match cmd.spawn() {
        Ok(_) => {
            // Successfully spawned, exit immediately
            process::exit(0);
        }
        Err(err) => {
            Err(anyhow::anyhow!("Failed to spawn GG: {}", err))
        }
    }
}
