# Implementation Plan: Askpass-Based Git Authentication

## Background

GG's current git authentication system is broken because jj-lib migrated from libgit2 to git subprocess. The existing `AuthContext` in `src/worker/git_util.rs` sets up `RemoteCallbacks` with `get_password` and `get_username_password` callbacks, but these callbacks are never called by jj-lib's subprocess implementation.

When git needs credentials, it uses native mechanisms:
1. `GIT_ASKPASS` environment variable
2. `SSH_ASKPASS` environment variable  
3. Git credential helpers
4. Direct terminal access via `/dev/tty`

The result is that authentication fails immediately, then git prompts on the terminal asynchronously after the mutation has returned to the frontend.

## Solution Overview

Implement an askpass-based authentication system that integrates with GG's existing request/retry flow:

1. **First attempt**: Git runs, askpass helper records what credentials are needed, returns failure. Mutation returns `MutationResult::InputRequired`.
2. **Retry with input**: User provides credentials via frontend dialog. Mutation retries with `InputResponse`. Askpass helper provides cached credentials. Git succeeds.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     GG Main Process                              │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Worker Thread - GitFetch/GitPush Mutation               │   │
│  │                                                          │   │
│  │  1. AuthContext::new(input) - starts IPC server          │   │
│  │  2. Set GIT_ASKPASS, SSH_ASKPASS, GG_ASKPASS_SOCKET      │   │
│  │  3. Call fetcher.fetch() - spawns git subprocess         │   │
│  │  4. On error: auth_ctx.into_result() returns InputRequired│   │
│  │  5. On retry: cached input provided to askpass           │   │
│  └──────────────────────────────────────────────────────────┘   │
│                          │                                       │
│                          │ IPC (Unix socket / Named pipe)        │
│                          ▼                                       │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  IPC Server (in AuthContext)                              │   │
│  │                                                           │   │
│  │  - Listens on $TMPDIR/gg-askpass-{uuid}.sock             │   │
│  │  - Receives: { prompt: "Password for ..." }               │   │
│  │  - If input available: respond with credential            │   │
│  │  - If not: record requirement, respond "unavailable"      │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                          ▲
                          │ Connects to socket
                          │
┌─────────────────────────────────────────────────────────────────┐
│  gg askpass <prompt>                                             │
│                                                                  │
│  1. Read GG_ASKPASS_SOCKET from environment                      │
│  2. Connect to IPC socket                                        │
│  3. Send prompt string                                           │
│  4. Receive response                                             │
│  5. If credential: print to stdout, exit 0                       │
│  6. If unavailable: exit 1 (tells git auth failed)               │
└─────────────────────────────────────────────────────────────────┘
```

## Detailed Implementation Steps

### Step 1: Add `interprocess` Dependency

**File**: `Cargo.toml`

Add the `interprocess` crate for cross-platform local sockets:

```toml
[dependencies]
interprocess = "2"
```

This crate provides:
- `LocalSocketStream` - client-side socket connection
- `LocalSocketListener` - server-side socket listener
- Cross-platform: Unix domain sockets on macOS/Linux, named pipes on Windows

### Step 2: Add `gg askpass` Subcommand

**File**: `src/main.rs`

The main binary already has subcommand handling. Add a new hidden `askpass` subcommand.

**Current structure** (approximate):
```rust
#[derive(clap::Subcommand, Debug)]
enum Subcommand {
    Gui { workspace: Option<PathBuf> },
    Web { workspace: Option<PathBuf> },
}
```

**Add**:
```rust
/// Internal: Handle askpass prompts from git (used by GIT_ASKPASS)
#[command(hide = true)]
Askpass {
    /// The prompt string from git/ssh
    prompt: String,
},
```

**Handler logic**:
```rust
Subcommand::Askpass { prompt } => {
    // 1. Get socket path from environment
    let socket_path = std::env::var("GG_ASKPASS_SOCKET")
        .expect("GG_ASKPASS_SOCKET not set");
    
    // 2. Connect to IPC socket
    let mut stream = LocalSocketStream::connect(socket_path)?;
    
    // 3. Send prompt (length-prefixed or newline-terminated)
    writeln!(stream, "{}", prompt)?;
    stream.flush()?;
    
    // 4. Read response
    let mut response = String::new();
    BufReader::new(&mut stream).read_line(&mut response)?;
    let response = response.trim();
    
    // 5. Handle response
    if response.starts_with("OK:") {
        // Print credential to stdout (git reads this)
        println!("{}", &response[3..]);
        std::process::exit(0);
    } else {
        // "UNAVAILABLE" - exit non-zero to signal auth failure
        std::process::exit(1);
    }
}
```

### Step 3: Extend `AuthContext` with IPC Server

**File**: `src/worker/git_util.rs`

The existing `AuthContext` structure:

```rust
pub struct AuthContext {
    input: Option<InputResponse>,
    requirements: RefCell<Vec<AuthRequirement>>,
}
```

**Extend to**:

```rust
use interprocess::local_socket::{LocalSocketListener, LocalSocketStream, NameTypeSupport};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use uuid::Uuid;

pub struct AuthContext {
    input: Option<InputResponse>,
    requirements: RefCell<Vec<AuthRequirement>>,
    socket_path: PathBuf,
    listener: LocalSocketListener,
}

impl AuthContext {
    pub fn new(input: Option<InputResponse>) -> std::io::Result<Self> {
        // Generate unique socket path
        let socket_name = format!("gg-askpass-{}", Uuid::new_v4());
        let socket_path = if cfg!(windows) {
            // Named pipe path on Windows
            PathBuf::from(format!(r"\\.\pipe\{}", socket_name))
        } else {
            // Unix socket in temp directory
            std::env::temp_dir().join(format!("{}.sock", socket_name))
        };
        
        // Create listener
        let listener = LocalSocketListener::bind(&socket_path)?;
        
        Ok(Self {
            input,
            requirements: RefCell::new(vec![]),
            socket_path,
            listener,
        })
    }
    
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
    
    /// Handle a single askpass request. Call this in a loop or thread.
    pub fn handle_askpass_request(&self) -> std::io::Result<()> {
        // Accept connection (blocking)
        let mut stream = self.listener.accept()?;
        
        // Read prompt
        let mut reader = BufReader::new(&stream);
        let mut prompt = String::new();
        reader.read_line(&mut prompt)?;
        let prompt = prompt.trim();
        
        // Parse prompt and check if we have credentials
        let response = self.process_prompt(prompt);
        
        // Send response
        writeln!(stream, "{}", response)?;
        stream.flush()?;
        
        Ok(())
    }
    
    fn process_prompt(&self, prompt: &str) -> String {
        // Parse the prompt to determine what's being asked
        // Git prompts look like:
        //   "Username for 'https://github.com':"
        //   "Password for 'https://user@github.com':"
        //   "Password for 'https://github.com':"
        
        if let Some(input) = &self.input {
            // Check if we have the requested credential
            if prompt.starts_with("Username for") {
                if let Some(username) = input.fields.get("Username") {
                    return format!("OK:{}", username);
                }
            } else if prompt.starts_with("Password for") || prompt.contains("assword") {
                if let Some(password) = input.fields.get("Password") {
                    return format!("OK:{}", password);
                }
            }
        }
        
        // Don't have credentials - record the requirement
        let requirement = self.parse_prompt_to_requirement(prompt);
        self.requirements.borrow_mut().push(requirement);
        
        "UNAVAILABLE".to_string()
    }
    
    fn parse_prompt_to_requirement(&self, prompt: &str) -> AuthRequirement {
        // Extract URL from prompt
        // "Username for 'https://github.com':" -> url = "https://github.com"
        let url = prompt
            .split('\'')
            .nth(1)
            .unwrap_or("unknown")
            .to_string();
        
        if prompt.starts_with("Username for") {
            // Need both username and password
            AuthRequirement::UsernamePassword { url }
        } else if prompt.starts_with("Password for") {
            // Extract username if present in URL
            // "Password for 'https://user@github.com':"
            if let Some(user_part) = url.split('@').next() {
                if let Some(username) = user_part.strip_prefix("https://") {
                    return AuthRequirement::Password {
                        url: url.clone(),
                        username: username.to_string(),
                    };
                }
            }
            // No username in URL, need both
            AuthRequirement::UsernamePassword { url }
        } else {
            // Unknown prompt type, assume username+password
            AuthRequirement::UsernamePassword { url }
        }
    }
}

impl Drop for AuthContext {
    fn drop(&mut self) {
        // Clean up socket file on Unix
        if !cfg!(windows) {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}
```

### Step 4: Run IPC Server During Git Operations

**File**: `src/worker/git_util.rs`

The current `with_callbacks` method needs to also handle askpass requests. Since git operations are blocking and may trigger multiple askpass calls, we need to handle IPC in a separate thread or use non-blocking I/O.

**Approach**: Spawn a thread that handles askpass requests while the git operation runs.

```rust
impl AuthContext {
    pub fn with_callbacks<T>(&self, f: impl FnOnce(RemoteCallbacks) -> T) -> T {
        // Set up traditional callbacks (for future jj-lib compatibility)
        let mut callbacks = RemoteCallbacks::default();
        
        let mut get_ssh_keys = Self::get_ssh_keys;
        callbacks.get_ssh_keys = Some(&mut get_ssh_keys);
        
        let mut get_password = |url: &str, username: &str| self.get_password(url, username);
        callbacks.get_password = Some(&mut get_password);
        
        let mut get_username_password = |url: &str| self.get_username_password(url);
        callbacks.get_username_password = Some(&mut get_username_password);
        
        // Spawn askpass handler thread
        let socket_path = self.socket_path.clone();
        let input = self.input.clone();
        let requirements = self.requirements.clone(); // Need to make this Arc<Mutex<...>>
        
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();
        
        let handle = std::thread::spawn(move || {
            // Create a non-blocking listener or poll with timeout
            // Handle requests until stop_flag is set
            while !stop_flag_clone.load(Ordering::Relaxed) {
                // Try to accept with timeout
                // ... handle request ...
            }
        });
        
        // Run the git operation
        let result = f(callbacks);
        
        // Stop the handler thread
        stop_flag.store(true, Ordering::Relaxed);
        let _ = handle.join();
        
        result
    }
}
```

**Alternative simpler approach**: Use `set_nonblocking(true)` on the listener and poll in a loop, or accept that the git operation will block and askpass requests arrive synchronously.

Given that git prompts are sequential (username, then password), a simpler synchronous approach may work:

```rust
pub fn with_callbacks<T>(&self, f: impl FnOnce(RemoteCallbacks) -> T) -> T {
    // Set listener to non-blocking
    self.listener.set_nonblocking(true)?;
    
    // ... set up callbacks ...
    
    let result = f(callbacks);
    
    // After git operation, the askpass requests have already been handled
    // because git waits for askpass to complete before continuing
    
    result
}
```

Actually, the key insight is: **git blocks waiting for askpass response**. So askpass handling happens synchronously from git's perspective. The IPC server needs to be available when askpass connects, but since we're in the same process, we need threading.

**Recommended approach**: Use a dedicated thread for the IPC server that runs for the lifetime of `AuthContext`.

### Step 5: Set Environment Variables in Mutations

**File**: `src/worker/mutations.rs`

In `GitFetch::execute()` (around line 1437):

**Current code**:
```rust
let git_settings = git::GitSettings::from_settings(&ws.data.workspace_settings)?;
let mut auth_ctx = AuthContext::new(self.input);

for (remote_name, pattern) in &remote_patterns {
    let mut fetcher = git::GitFetch::new(tx.repo_mut(), &git_settings)?;
    // ...
    if let Err(err) = auth_ctx.with_callbacks(|cb| {
        fetcher.fetch(...)
    }) {
        return Ok(auth_ctx.into_result(err));
    }
}
```

**Modified code**:
```rust
let git_settings = git::GitSettings::from_settings(&ws.data.workspace_settings)?;
let auth_ctx = AuthContext::new(self.input)?; // Now returns Result

// Set environment variables for askpass
let exe_path = std::env::current_exe()?;
std::env::set_var("GIT_ASKPASS", &exe_path);
std::env::set_var("SSH_ASKPASS", &exe_path);
std::env::set_var("SSH_ASKPASS_REQUIRE", "force");
std::env::set_var("GG_ASKPASS_SOCKET", auth_ctx.socket_path());
std::env::set_var("DISPLAY", ":0"); // Required for SSH_ASKPASS

for (remote_name, pattern) in &remote_patterns {
    let mut fetcher = git::GitFetch::new(tx.repo_mut(), &git_settings)?;
    // ...
    if let Err(err) = auth_ctx.with_callbacks(|cb| {
        fetcher.fetch(...)
    }) {
        // Clean up env vars
        std::env::remove_var("GIT_ASKPASS");
        std::env::remove_var("SSH_ASKPASS");
        std::env::remove_var("SSH_ASKPASS_REQUIRE");
        std::env::remove_var("GG_ASKPASS_SOCKET");
        std::env::remove_var("DISPLAY");
        
        return Ok(auth_ctx.into_result(err));
    }
}

// Clean up env vars on success too
std::env::remove_var("GIT_ASKPASS");
// ... etc
```

**Consider**: Extract env var setup/cleanup into a helper or RAII guard.

### Step 6: Apply Same Changes to `GitPush`

**File**: `src/worker/mutations.rs`

The `GitPush` mutation (search for `impl Mutation for GitPush`) needs identical changes:
- Create `AuthContext` with IPC server
- Set environment variables
- Clean up on completion

### Step 7: Thread Safety for Requirements Collection

**File**: `src/worker/git_util.rs`

Currently `requirements` uses `RefCell` which is not thread-safe. Since the IPC handler runs in a separate thread, change to:

```rust
use std::sync::{Arc, Mutex};

pub struct AuthContext {
    input: Option<InputResponse>,
    requirements: Arc<Mutex<Vec<AuthRequirement>>>,
    socket_path: PathBuf,
    listener: LocalSocketListener,
    handler_thread: Option<JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
}
```

Update all `self.requirements.borrow_mut()` to `self.requirements.lock().unwrap()`.

### Step 8: Add UUID Dependency (if not present)

**File**: `Cargo.toml`

Check if `uuid` is already a dependency. If not, add:

```toml
[dependencies]
uuid = { version = "1", features = ["v4"] }
```

## IPC Protocol

Simple text-based protocol:

**Request** (askpass → server):
```
<prompt string>\n
```

**Response** (server → askpass):
```
OK:<credential>\n
```
or
```
UNAVAILABLE\n
```

## Testing Plan

1. **Unit tests for prompt parsing**:
   - Test `parse_prompt_to_requirement()` with various git/ssh prompt formats
   - `"Username for 'https://github.com':"` → `UsernamePassword { url: "https://github.com" }`
   - `"Password for 'https://user@github.com':"` → `Password { url: ..., username: "user" }`

2. **Integration test for IPC**:
   - Create `AuthContext` with input
   - Connect to socket, send prompt
   - Verify correct response

3. **Manual testing**:
   - Configure a private repo that requires authentication
   - Try to fetch without credentials → should get `InputRequired`
   - Retry with credentials → should succeed

## Files to Modify

| File | Changes |
|------|---------|
| `Cargo.toml` | Add `interprocess`, possibly `uuid` |
| `src/main.rs` | Add `askpass` subcommand |
| `src/worker/git_util.rs` | Extend `AuthContext` with IPC server |
| `src/worker/mutations.rs` | Set env vars in `GitFetch` and `GitPush` |

## Edge Cases

1. **Socket path too long**: Unix socket paths have a ~104 character limit. Use short UUID format and temp dir.

2. **Multiple concurrent git operations**: Each mutation creates its own `AuthContext` with unique socket, so no conflicts.

3. **Git prompts multiple times**: Git may ask for username, then password. The IPC server handles each request sequentially.

4. **SSH key passphrase**: SSH may prompt `"Enter passphrase for key '/path/to/key':"`. Parse and handle similarly.

5. **Host verification**: SSH may prompt `"Are you sure you want to continue connecting (yes/no)?"`. For now, this will fail - future work could add support.

## Future Enhancements

1. **Multiple input fields**: Currently limited to single `InputRequest`. Frontend changes needed for sequential prompts.

2. **Timeout support**: Add configurable timeout for user response.

3. **Host key verification**: Handle SSH host authenticity prompts with yes/no dialog.
