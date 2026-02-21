# GG Changelog

## Unreleased

### Added
- JJ config settings can now be overridden for GG only by prefixing a setting with `gg.`. For example, if you have...
  ```
  user.name = "JJ Author"
  gg.user.name = "GG Author"
  ```
  ...then revisions created by GG will be authored by "GG Author" instead of "JJ Author".
- If `ui.diff-formatter` is set to a non-builtin tool, an "open in diff tool" button will appear for each file in the changelist. If you don't want this, or want to use *different* external tool than you use with jj-cli, set `gg.ui.diff-formatter` - an empty string will disable external diffing. 
- If `ui.merge-editor` is set, or both merge-editor and diff-formatter are set, conflicted files will have an "resolve with merge tool" button instead. Sample config for a less cluttered change list:
  ```
  [ui]
  diff-formatter = "bc3"
  merge-editor = "bc3"
  [gg.ui]
  diff-formatter="" # overrides ui.diff-formatter, removing the difftool button but not the mergetool button
  ```
- Tags now have an icon.
- Revisions which are the working copy of another workspace are displayed with a yellow dot. The workspace's name is displayed alongside bookmarks and tags.

### Changed
- `[gg.revsets]` has been renamed to `[gg.presets]`. Sorry about the churn, but the earlier name was a mistake - it clashes with jj's `[revsets]` config table.

### Fixed
- Pushes were silently rejected when branches had moved or been locked down; now an error is displayed.
- Reverting from the GUI-mode context menu was broken.

## [0.38.2](releases/tag/v0.38.2)

### Fixed
- The revset selector and textbox colours were broken on MacOS due to some CSS that Safari doesn't yet support.
- The reset-author toggle in the right pane didn't become clickable when toggling on "ignore immutability" until you performed some other operation.

## [0.38.1](releases/tag/v0.38.1)

### Fixed
- `XDG_CONFIG_HOME` wasn't being used to look up global gitignores.

## [0.38.0](releases/tag/v0.38.0)
This release is based on Jujutsu 0.38.

### Added
- There's a toggle in the bottom left of the screen marked with a 🛡; turning it on acts like `jj --ignore immutable`, affording you infinite power.
- The command-line argument `--ignore-immutable` will turn on the new toggle at startup. 
- The additional revsets displayed in the left-pane selector can by customised by adding config values under `[gg.revsets]`.

### Changed
- Temporary behavioural toggles are now represented with a sticky button instead of a checkbox. 

### Fixed
- In web mode, right-clicking on revisions enabled context menu commands based on the *selected* revision rather than the one you'd clicked.

## [0.37.2](releases/tag/v0.37.2)

### Added
- Multiple selection: 
  * Select a range with shift-click or shift-arrowkeys.
  * See a combined change/conflict view in the right pane. 
  * Squash/restore the combined diff, or files within it - not hunks, for now.
  * Drag-drop to rebase an entire range at once.
  * Right-click or use the Revision menu in GUI mode to exeute commands on the range.
- Divergent/hidden revision display, using the change-id offsets introduced in JJ 0.37.
- GG can also *target* divergent revisions or revsets, so you can now use it to abandon or rescue divergent revisions.

### Changed
- "Backout" is now "Revert", following the change in JJ 0.35.

## [0.37.1](releases/tag/v0.37.1)

### Added
- Repository -> Recent Items menu.

### Fixed
- Dropdowns in input dialogs weren't sending their value to the backend correctly.
- Revision menu commands were applying to *every* open window.
- Tweaks to the "Changes" bar visuals.

## [0.37.0](releases/tag/v0.37.0)
This release is based on Jujutsu 0.37.

### Added
- Repository -> Init... and Repository -> Clone... menuitems, for creating repositories.
- Progress bar for slow git operations (fetch, push, clone).
- Relative timestamps update on each snapshot (which happen after modifications or when the window/tab is focused).
- GG now respects the `snapshot.auto-update-stale` setting. Additionally, when first opening a repo, it will always update the working copy if it's stale.

### Fixed
- In GUI mode, the Repository -> Open... menuitem always opened a new window even if you didn't have a workspace loaded in the current window.

## [0.36.4](releases/tag/v0.36.4)

### Added
- **Web Mode**: GG can now be run using `gg web`, which will start a web server and a browser instead of a desktop application. It has the same featureset apart from the lack of a top menubar and features inherent to the platform - only gui mode has a taskbar icon to right-click, only web mode supports http-proxy remote access, etc. 

  New command line options:
  * `-p/--port`: run on a specified port number (default: random).
  * `--launch`/`--no-launch`: do/don't attempt to open a browser (default: do).

  These can also be configured using some new `[gg.web]` settings.
  New config settings:
  `gg.default-mode`: `"gui"` or `"web"`. This controls what `gg` does with no subcommand specified.
  `gg.web.default-port`: equivalent to `--port`.
  `gg.web.launch-browser`: equivalent to `--launch`/`--no-launch`.
  `gg.web.client-timeout`: how long the server should wait for a client ping before shutting down.

  Web mode uses a standard request-response model, shutting down when all tabs are closed or haven't pinged the backend in a while. It has HTML dialogs and context menus instead of the native ones provided by Tauri.

- Restore and squash for individual hunks. Right-click on the header of a diff section in the right pane to manipulate it.

- GIT_ASKPASS support - if you don't have a git credential manager set up, GG will configure the git subprocess doing a fetch or push to request credentials using its built-in dialogs. This shouldn't affect most people, as credential managers are generally included with distributions of git.

- There's now a cursor indication when something is clickable.

- In GUI mode, multiple window support. The "Open..." command in the Repository menu will now open another workspace. Selections and window positions are preserved independently.

- When built as an app, MacOS recent-items support. The dock icon menu will show recent workspaces and can be used to open or switch between them.

### Fixed
- `receiving on a closed channel` error at shutdown.

- Button icon colours not always responding correctly to modal overlays.

- When built as an app, the MacOS dock icon is no longer overridden.

- When built as a CLI, improved child spawning - background and `--foreground` modes work more consistently.

## [0.36.3](releases/tag/v0.36.3)

### Fixed
- CLI build: added dock icon on MacOS.
- CLI build: the advertised `--foreground` now actually exists and works.
- GG now respects the `snapshot.auto-track` setting.

## [0.36.2](releases/tag/v0.36.2)

### Added
- GG is now available from crates.io: `cargo install --locked gg-cli`. This will give you a `gg` CLI binary on your PATH which launches the GUI in the background or, with `--foreground`, in the foreground.

## [0.36.1](releases/tag/v0.36.1)

### Fixed
- Change IDs in the log pane would sometimes display the wrong suffix. This was happening when a line's id changed but its prefix remained the same.

## [0.36.0](releases/tag/v0.36.0)
This release is based on Jujutsu 0.36.

### Added
- The text of error dialogs is now selectable for copying.

### Changed
- Moving sub-file changes ("hunks") has been reworked with a new algorithm that will hopefully have more intuitive results and reduced conflicts. Conceptually, it now works as if you'd split the original commit into two, rebased everything, then squashed the split-out part. 

### Fixes
- Performance improvements due updated dependencies and some internal use of async.

## [0.35.2](releases/tag/v0.35.2)

### Fixed
- Git remote handling: gg now displays only fetchable remotes, and fetching actually works again.
- Pushing a single bookmark with right-click was also broken for some repos, depending on config, and could fail with an error about trying to push to the "git" pseudo-remote.
- Spurious @git bookmarks were showing up in colocated repos. This has probably been an issue for a while, but colocation became more common recently due to a change in jj defaults. Now they're hidden.
- Graph line rendering was breaking in various ways due to our attempt to fix memory leaks with structural comparison. Switched to a different implementation (index comparison, deeper reactivity) which should be more efficient as well as unbreaking scrolling, decorations, etc.
- Drag-drop of bookmarks was also affected, and is also fixed.
- Spurious "receiving on a closed channel" errors at startup - they were harmless, but now they're gone.

## [0.35.1](releases/tag/v0.35.1)

### Added
- New config option `gg.ui.track-recent-workspaces`, which can be set to false to disable saving recent workspaces to the config file.

### Fixed
- Another memory leak (failure to deregister RAF callbacks).
- Some broken graph rendering (which was relying on the previous leak!).

## [0.35.0](releases/tag/v0.35.0)
This release is based on Jujutsu 0.35.

### Fixed
- Memory leak in the log pane (thanks to @brk).

## [0.29.1](releases/tag/v0.29.1)

### Added
- "New inserted parent" menu item (thanks to @brk).
- Move sub-file hunks from the right pane (thanks to @nightscape).
- Show recent workspaces if opening a workspace failed (thanks to @Natural-selection1).
- Change and commit ID can be selected for copying.

### Fixed
- Fix overscroll on MacOS (thanks to @mernen).
- Compress path and action info when window is too narrow (thanks to @Natural-selection1).
- Use from_utf8_lossy to prevent invalid utf-8 sequence errors (thanks to @jmole).
- Enabled LTO for release builds, smaller and faster binary (thanks to @berkus).

## [0.29.0](releases/tag/v0.29.0) 
This version is based on Jujutsu 0.29.

### Changed
- Update to jj 0.29 (thanks to @nightcore and and @ilyagr).
- Update to rust 2024 (thanks to @natural-selection1).

### Fixed
- Excessively tall horizontal scrollbars in WebKit (thanks to @ninjamuffin99).
- Untracked some Tauri artifacts that were changing every version (thanks to @ilyagr).
- `tauri dev` is now compatible with Hyper-V (thanks to @natural-selection1).
- Ctrl-enter keyboard shortcut on some platforms (thanks to @natural-selection1).
- Describe box resizing on some platforms (thanks to @natural-selection1).
- Flickering when dragging commits onto each other to rebase (thanks to @natural-selection1).

## [0.27.0](releases/tag/v0.27.0) 
This version is based on Jujutsu 0.27.

### Added
- Cmd/Ctrl-enter shortcut to save revision descriptions.

### Fixed
- Suppress MacOS auto-capitalisation of branch/remote names. 

## [0.23.0](releases/tag/v0.23.0) 
This version is based on Jujutsu 0.23 and the recently-released Tauri 2.0.

### Changed
- Branches have been renamed to bookmarks. The setting `gg.ui.mark-unpushed-branches` has changed to `mark-unpushed-bookmarks`, but the old one will still work as well.

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
