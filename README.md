![screenshot](src-tauri/resources/screenshot.png)

GG is an experimental GUI for [Jujutsu](https://github.com/martinvonz/jj). The idea is to take advantage of Jujutsu's clean [architecture](https://martinvonz.github.io/jj/latest/technical/architecture/) to present am interactive view of your repository. What if you were always in the middle of an interactive rebase, but this was a good thing?

## Installation
Binaries are available for several platforms on the [releases page](https://github.com/gulbanana/gg/releases). To build from source, run `npm install` followed by `npm run tauri build`.

## Features
GG doesn't depend on (JJ)[https://martinvonz.github.io/jj/latest/install-and-setup/] to run, but you'll need it for tasks GG doesn't cover. Those tasks:
- Use the left pane to query and browse the log. Click to select revisions, double-click to edit (if mutable) or create a new child (if immutable).
- Use the right pane to inspect and edit revisions - set descriptions, issue commands, view their changes and parents. 
- Right-click revisions, changes and branches to do some useful things. Drag them around to change history. 
- Undo anything with ⟲ in the bottom right corner.

### Future Features
There's no roadmap as such, but items on [the to-do list](TODO.md) may or may not be implemented in future. 

### Known Issues
Log queries will be slower if your repo contains many commits outside the set determined by `immutable_heads()`. Immutability checking in the log view can be disabled by setting `gg.check-immutable = false`.

## Development  
Recommended IDE setup: [VS Code](https://code.visualstudio.com/) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode).

Some useful commands:
* `npm run test` - execute unit tests.
* `npm run gen` - update the IPC message types in src/messages from src-tauri/messages.rs.
* `npm run tauri dev` - launch a debug build with automatic reloading.
* `npm run tauri build -- --target universal-apple-darwin` - create a fat binary for MacOS.

[DESIGN.md](DESIGN.md) has some basic information about how GG works.