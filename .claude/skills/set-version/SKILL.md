---
name: set-version
description: Updates GG's version number for all components. Use when asked to update the version or create releases.
---

<background-information>
GG has multiple components: a Rust crate, a Node package and a Tauri application. All of these need to be kept in sync, as does the documentation.
The versioning numbering scheme is 0.xx.y, where xx is the JJ version number and y is the number of a GG release for that JJ version. For example, the first GG release for JJ 0.37.0 was called GG 0.38.0, and the second was called GG 0.38.1.
</background-information>

Version update procedure
========================
1. Set package.version in `Cargo.toml`.
2. Run `cargo update`.
3. Set "version" in `package.json`.
4. Run `npm update`. 
5. Set version in `Tauri.toml`.
6. Update CHANGELOG.md.
7. `jj commit -m "version number"`

CHANGELOG.md update procedure
=============================
We follow the https://keepachangelog.com/ standard. The first "##" second-level header in CHANGELOG.md will be either a version number or "[Unreleased]". 
- If it's `[Unreleased]`, it should be replaced with this pattern: `[0.xx.y](releases/tag/v0.xx.y)`. This links to the tag that will later be created on GitHub.
- If it already matches the correct pattern, there's no need to update it further.
- If it's anything else, that's a problem - it means the release doesn't have changelog entries. Notify the user about this and stop!
In any case, if this is a .0 update (the first for a given JJ version), ensure that the first line after the second-level header is `This release is based on Jujutsu 0.xx.`.
