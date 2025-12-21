# Shell Behavior Implementation

## Overview

GG now supports two modes of operation to provide consistent shell behavior across all platforms:

1. **Default mode (`gg`)**: Spawns a detached child process running the GUI, then exits immediately. The shell returns to prompt.
2. **Foreground mode (`gg --foreground`)**: Runs the GUI directly in the current process. The shell waits until the GUI is closed. Ctrl+C kills the process.

## Implementation Details

### Cross-Platform Process Spawning

#### Unix (Linux/macOS)
- Uses `fork()` to create a child process
- Child calls `setsid()` to create a new session and detach from terminal
- Child re-execs itself with `--foreground` flag
- Parent process exits immediately
- Child's stdin/stdout/stderr are redirected to `/dev/null`

#### Windows
- Uses `CreateProcess` with special flags:
  - `DETACHED_PROCESS`: Detaches from parent console
  - `CREATE_NEW_PROCESS_GROUP`: Creates new process group
  - `CREATE_NO_WINDOW`: Prevents console window flash
- Child's stdin/stdout/stderr are redirected to null
- Parent process exits immediately after spawning child

### Windows Console Handling

In foreground mode on Windows:
1. Application uses console subsystem (not GUI subsystem)
2. `AttachConsole(ATTACH_PARENT_PROCESS)` is called to attach to parent shell's console
3. After GUI initialization, `FreeConsole()` is called to release the console
4. This ensures:
   - Shell waits for the process (because it's attached)
   - No orphaned console window when launched from Explorer

### Command-Line Arguments

- `--foreground`: Run in foreground mode (blocks shell)
- `--debug`: Enable debug logging
- `<workspace>`: Open specific directory

## Testing Checklist

| Platform | Shell | `gg` | `gg --foreground` |
|----------|-------|------|-------------------|
| Windows | PowerShell | Returns immediately ✓ | Blocks until close ✓ |
| Windows | cmd | Returns immediately ✓ | Blocks until close ✓ |
| Windows | Git Bash | Returns immediately ✓ | Blocks until close ✓ |
| Linux | bash/zsh | Returns immediately ✓ | Blocks until close ✓ |
| macOS | bash/zsh | Returns immediately ✓ | Blocks until close ✓ |

## Code Structure

### Files Modified

1. **src/main.rs**
   - Removed `#![cfg_attr(not(test), windows_subsystem = "windows")]`
   - Added `--foreground` flag to `Args` struct
   - Implemented `spawn_detached_child()` for Unix and Windows
   - Extracted GUI initialization into `run_gui()` function
   - Added logic to check foreground flag and spawn detached process

2. **src/windows.rs**
   - Replaced `reattach_console()` with `setup_foreground_console()`
   - Added `free_console()` function
   - Updated imports for `FreeConsole`

3. **Cargo.toml**
   - Added `Win32_System_Threading` feature to windows dependency
   - Added `libc` dependency for Unix platforms

## Technical Notes

### Why Not Use `windows_subsystem = "windows"`?

The `windows_subsystem = "windows"` attribute makes the executable a GUI subsystem app, which:
- Doesn't create a console window (good for desktop shortcuts)
- Cannot block shells in foreground mode (bad for CLI usage)

By using console subsystem and dynamically managing the console:
- We can block shells when needed (foreground mode)
- We can prevent console windows (via `FreeConsole()`)
- Both PowerShell and cmd work correctly

### Fork vs Spawn on Unix

On Unix, we use `fork()` + `setsid()` + `exec()` instead of just `spawn()` because:
- `setsid()` properly detaches from the controlling terminal
- Prevents signals (like Ctrl+C) from reaching the child
- Child becomes a session leader with no controlling terminal

### PowerShell Differences

PowerShell has different console handle inheritance than cmd:
- PowerShell waits for child processes that have inherited console handles
- Even with `DETACHED_PROCESS`, PowerShell may wait if handles aren't redirected
- Redirecting stdin/stdout/stderr to null solves this issue
