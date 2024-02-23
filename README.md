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
