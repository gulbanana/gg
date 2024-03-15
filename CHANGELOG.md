# GG Changelog

## Unreleased

### Added
- Added git remotes to the status bar, with push & fetch commands.

## [0.15.2](releases/tag/v0.15.2)

### Fixed
- Right click -> Abandon revision... again.

## [0.15.1](releases/tag/v0.15.1)

### Fixed
- Several buttons had stopped working due to IPC changes:
  * The Squash/Restore buttons on the right pane.
  * Right click -> Abandon revision.
  * Right click -> Squash into parent.
  * Right click -> Restore from parent.

## [0.15.0](releases/tag/v0.15.0)
Initial experimental release. This version is compatible with Jujutsu 0.15.

### Added
- Open, reload and snapshot repositories.
- Graph-based log displaying summaries, author and status.
- Log queries in Jujutsu's [revset language](https://martinvonz.github.io/jj/latest/revsets/).
- Revision view with file-level change details and editing commands.
- Drag and drop to move, remove and recombine revisions/files/branches.
- Context menus for common operations.
- Transactional operations with single-level undo.
- Light and dark themes.
- Codesigned binaries for MacOS and Windows.
- Completely untested binaries for Linux.
