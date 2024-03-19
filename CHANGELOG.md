# GG Changelog

## [0.15.3](releases/tag/v0.15.3)

### Added
- Git remotes in the status bar, with push & fetch commands.
- "Create branch" command on revisions. 
- Display edges to commits that aren't in the queried revset, by drawing a line to nowhere.
- Detect changes made by other Jujutsu clients and merge the operation log automatically.
- Window title includes the workspace path (when one is open).
- New config option gg.queries.log-page-size for tuning performance on large repositories.
- Miscellaneous design improvements.

### Fixed 
- GG now understands divergent changes, and can act on commits that have a shared change id. 
  Note that if you do anything to such commits other than abandoning them, you're likely to 
  create even more divergent commits!
- The AppImage build wasn't picking up the working directory correctly. This is fixed, and 
  you can also specify a workspace to open on the commandline as an alternative.

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
