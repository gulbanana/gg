# GG Changelog

## [0.23.0](releases/tag/v0.23.0) 
This version is based on Jujutsu 0.23 and the recently-released Tauri 2.0.

## [0.20.0](releases/tag/v0.20.0) 
This version is based on Jujutsu 0.20.

### Fixed
- `gg.queries.log-page-size` setting was not being respected.
- Removed &lt;CR&gt; character which rendered as a circle in the author display on some Linux systems.
- Improved button/control font display on Linux.
- Fixed a panic attempting to display delete/delete conflicts in the right pane.

## [0.18.0](releases/tag/v0.18.0) 
This version is based on Jujutsu 0.18.

## [0.17.0](releases/tag/v0.17.0) 
This version is compatible with Jujutsu 0.17.

## [0.16.0](releases/tag/v0.16.0) 
This version is compatible with Jujutsu 0.16.

### Added
- File diffs displayed in the revision pane; also, the file list is now keyboard-selectable.
- Backout command, which creates the changes necessary to undo a revision in the working copy.
- Consistent author/timestamp formatting, with tooltips for more detail.

### Fixed
- Right-pane scrollbar wasn't responding to clicks.
- Various design improvements. 

## [0.15.3](releases/tag/v0.15.3)

### Added
- Relatively comprehensive branch management - create, delete, rename, forget, push and fetch.
- Display Git remotes in the status bar, with commands to push or fetch all their branches.
- Display Git tags (readonly; they aren't really a Jujutsu concept).
- Display edges to commits that aren't in the queried revset, by drawing a line to nowhere.
- Detect changes made by other Jujutsu clients and merge the operation log automatically.
- Improved keyboard support and focus behaviour.
- Window title includes the workspace path (when one is open).
- On Windows, the taskbar icon has a jump list with links to recent workspaces.
- New config options:
  * `gg.queries.log-page-size` for tuning performance on large repositories.
  * `gg.ui.mark-unpushed-branches` to control whether local-only branches are called out.

### Fixed 
- GG now understands divergent changes, and can act on commits that have a shared change id. 
  Note that if you do anything to such commits other than abandoning them, you're likely to 
  create even more divergent commits!
- The AppImage build wasn't picking up the working directory correctly. This is fixed, and 
  you can also specify a workspace to open on the commandline as an alternative.
- Watchman support (core.fsmonitor) was not enabled.
- Various design improvements.

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
