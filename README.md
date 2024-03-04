GG is an experimental GUI for [Jujutsu](https://github.com/martinvonz/jj). It doesn't have enough features to be useful yet, but feel free to try it out or repurpose the code elsewhere.

## Installation
Binaries are available for several platforms at https://github.com/gulbanana/gg/releases. To build from source, execute `npm install` followed by `npm run tauri build`.

## Development  
Recommended IDE setup: [VS Code](https://code.visualstudio.com/) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode).

Some useful commands:
* `npm run test` - execute unit tests.
* `npm run gen` - update the IPC message types in src/messages from src-tauri/messages.rs.
* `npm run tauri dev` - launch a debug build with automatic reloading.
* `npm run tauri build -- --debug` - create a distributable app with debug symbols.
* `npm run tauri build -- --target universal-apple-darwin` - create a fat binary for MacOS.

## Features
- Left pane: query and browse the change log. Click to select commits, double-click to edit (if mutable) or create a new child (if immutable).
- Right pane: view commit details, set description and authors. 
- Use command buttons and menus to execute an equivalent of `jj new`, `edit`, `duplicate` and `abandon`.

### Roadmap
GG doesn't do any of these things yet, but they may be added eventually:

### Known Issues
Log queries will be slower if your repo contains many commits outside the set determined by `immutable_heads()`. Immutability checking in the log view can be disabled by setting `gg.check-immutable = false`.