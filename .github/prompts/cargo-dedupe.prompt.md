---
agent: agent
---
In **src-tauri/**, run `cargo tree -d` to see duplicate crate dependencies. Some of these will be due to a dependency version used by jj-lib or jj-cli differing from what's in Cargo.toml. Update Cargo.toml to match jj's versions.