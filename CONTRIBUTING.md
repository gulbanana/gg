# ![icon](res/icons/24x24.png) GG - Gui for JJ

Contributions are welcome. GG has a pretty specific vision, so we might not accept every suggestion, but we do aim to build a GUI that's useful for a wide range of Jujutsu workflows. [DESIGN.md](doc/DESIGN.md) has some basic information about how GG works and why.

We have high standards for maintainability; don't submit AI-generated code without verifying its quality and your understanding.

Contributors should sign the Google CLA so that GG code can be reused in JJ. This isn't a hard requirement, but if you haven't signed then your code may be replaced or removed. 

## Initial Setup
1. Run the first frontend build: `npm install && npm run build`. Future builds will be done automatically by `cargo tauri` or `cargo publish`.
2. (Optional) Install the Tauri CLI: `cargo install tauri-cli --version "^2.0.0" --locked`. This allows you to use `cargo tauri` instead of `npm run tauri`.
3. (Linux) Install system dependencies (on Debian-likes, `apt install libpango1.0-dev libatk1.0-dev libgdk-pixbuf2.0-dev libgtk-3-dev libjavascriptcoregtk-4.1-dev libwebkit2gtk-4.1-dev`).

## Useful Commands
* `cargo check`, `npm run check` - execute type checkers.
* `cargo test`, `npm run test` - execute unit tests.
* `cargo gen` - update the IPC message types in app/messages from src/messages.rs.
* `cargo tauri dev` - launch a debug build with automatic reloading.
* `cargo tauri dev -- -- --debug` - run locally with --debug. Yes, both `--` are necessary.
* `cargo tauri build` - create a standard release build. Require codesigning setup.
* `cargo tauri build --target universal-apple-darwin` - create a fat binary for MacOS.

## Development Tools
Only `cargo` and some `npm` equivalent are *required*, but this IDE setup works well: [VS Code](https://code.visualstudio.com/) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode).