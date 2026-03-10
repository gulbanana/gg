# ![icon](res/icons/24x24.png) GG - Gui for JJ

![screenshot](res/screenshot.png)

GG is a GUI for the version control system [Jujutsu](https://github.com/jj-vcs/jj). It takes advantage of Jujutsu's composable primitives to present an interactive view of your repository. The big idea: what if you were always in the middle of an interactive rebase, but this was actually a good thing?

## Installation
GG is a desktop or web application with a keyboard & mouse interface, written in [Tauri](https://tauri.app/). Binaries are available for several platforms on the [releases page](https://github.com/gulbanana/gg/releases). Use the `.dmg` or `.app.tar.gz` on MacOS, and the `.msi` or `.exe` on Windows. (GG does work on Linux, but isn't as well-tested.)

You can also build from source using `cargo install --locked gg-cli`. This non-app build doesn't support as many OS integration features, but it's easier to package and detaches from your shell by default.

### Setup 
Put `gg` on your path and run it from a Jujutsu workspace, pass the workspace directory as an argument or launch it from a GUI and use the Repository->Open menu item. Tips:
- `gg` or `gg gui` will launch a native application, `gg web` will open a web browser.
- On MacOS, try adding `/Applications/gg.app/Contents/MacOS/` to your PATH environment variable. On Windows, add `C:\Program Files\gg\`.
- When using the app build, `gg &` on MacOS/Linux or `start gg` on Windows will run in the background without blocking your shell.
- `gg --help` will display some possible command-line arguments.

### Configuration
GG uses `jj config`; `revset-aliases.immutable_heads()` is particularly important, as it determines how much history you can edit. GG has some additional settings of its own, with defaults and documentation [here](src/config/gg.toml).

## Features
GG doesn't require [JJ](https://jj-vcs.github.io/jj/latest/install-and-setup/) installed, but you'll want it for tasks GG doesn't cover. What it *does* cover:
- Use the left pane to query and browse the log. Click to select revisions, shift-click for multiple selection, double-click to edit (if mutable) or create a new child (if immutable).
- Use the right pane to inspect and edit revisions - set descriptions, issue commands, view their parents and changes.
- Drag revisions around to rebase them; move them into or out of a revision's parents to add merges and move entire subtrees. Or just abandon them entirely.
- Drag files around to squash them into new revisions or throw away changes (restoring from parents).
- Drag bookmarks around to set or delete them. 
- Right click on any of the above for more contextual actions.
- Push and fetch git changes using the bottom bar.
- View diffs or resolve conflicts in an external tool (if you have one configured). 
- Undo anything with ⟲ in the bottom right corner.

More detail is available in [the changelog](CHANGELOG.md).

### Future Features
There's no roadmap as such, but items on [the to-do list](doc/TODO.md) may or may not be implemented in future.

### Known Issues
GG is lightly maintained and may have bugs. In theory it can't corrupt a repository thanks to the operation log, but it never hurts to make backups. 

If your repo is "too large" some features will be disabled for performance. See [the default config](src/config/gg.toml) for details.
