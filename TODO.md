MVP requirements
----------------
implement or disable branch context commands
dnd rebase 
- drop on node to reparent
- drop on edge to insert
- drag extra parents in to merge
dnd move
- drag files onto commits
dnd branch
- drag chips onto commits
gecko-dev is too slow again, perhaps due to auto-snapshots
work around or wait for the tauri bug wherein event listeners are cleaned up on page load - this prevents the macos prod build receiving events in frontend

Ideas and plans
---------------
These changes may or may not be implemented in the future.
* bug: proper fix for https://github.com/tauri-apps/tauri/issues/9127 (currently worked-around via fork)
* bug: failed command during long load never dismisses mutation-wait overlay
* bug: open menu command sometimes opens multiple dialogues
* edge case: change ids that refer to more than one rev
* edge case: selection issues like "nothing selected yet" or a new query that doesn't include the selection. this might be fine as-is
* edge case: what happens when we snapshot after the CLI does? when there's nothing *to* snapshot, we don't refresh the ui...
* perf: optimise revdetail loads - we already have the header
* perf: better solution to slow immutability check
* feat: log keyboard support
* feat: log multiselect
* feat: more context actions (incl. multiselect)
* feat: file select/multiselect? large moves could be tedious otherwise. maybe file menu?
* feat: redo/undo stack
* feat: operation menu - restores or views?
* feat: diffs and/or difftool
* feat: resolve workflow 
* feat: remotes/fetch/push
* feat: tags display & management
* feat: more settings
    - force dark theme on/off
    - log revsets
    - large history/large checkout heuristics
* design: decide whether to remove edit menu
* design: app icon
* design: draw missing (edge-to-nowhere) graph nodes?
* epic: categorical expansion - trays, modals, pinned commits etc
* chore: windows codesigning will break in august 2024; needs a new approach