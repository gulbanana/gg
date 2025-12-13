# gg-cli

This crate provides a `cargo install`-able version of GG - Gui for JJ.

## For Users

To install GG via cargo:

```bash
cargo install --locked gg-cli
```

Then run `gg` from any directory to launch the GUI application. The GUI will automatically spawn in the background, freeing your shell immediately.

### Usage

```bash
# Launch GUI (non-blocking)
gg

# Launch GUI in a specific directory
gg /path/to/workspace

# Enable debug logging
gg --debug

# See all options
gg --help
```

## For Maintainers

### Prerequisites

Before building or publishing this crate, you must build the frontend assets:

```bash
# From the repository root
npm install
npm run build
```

This creates the `dist/` directory with the compiled frontend. The CLI crate references these pre-built assets and includes them in the published crate.

### Building

To build the CLI binary locally:

```bash
# From the repository root
cargo build -p gg-cli --release
```

The binary will be at `target/release/gg`.

### Publishing to crates.io

1. Ensure the frontend is built (see Prerequisites above)
2. Update the version in both `Cargo.toml` files (root and `cli/Cargo.toml`)
3. Commit the built `dist/` directory to the repository
4. Publish the CLI crate:

```bash
cd cli
cargo publish
```

**Important**: The `dist/` directory must be committed to the repository before publishing, as crates.io needs the pre-built frontend assets.

### Structure

- `cli/src/main.rs` - Simple entry point that calls into the main `gg` library
- `cli/build.rs` - Build script that verifies frontend assets exist and calls `tauri_build::build()`
- `cli/Tauri.toml` - Tauri configuration (paths adjusted for CLI subdirectory)
- `../dist/` - Pre-built frontend assets (referenced by `frontendDist` in Tauri.toml)

### Why This Structure?

The main `gg` crate is both a library and a binary. The CLI crate:
1. Depends on `gg` as a library to reuse all the application logic
2. Has its own `main.rs` that just calls `gg::run()`
3. References pre-built frontend assets so users only need `cargo` (not Node.js/npm)
4. Uses Tauri's build system to bundle the assets into the binary

This allows users to run `cargo install gg-cli` without needing to install Node.js, npm, or any frontend tooling.
