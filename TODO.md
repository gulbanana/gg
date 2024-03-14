Desirable things
----------------

These changes may or may not be implemented in the future.
* bug: proper fix for https://github.com/tauri-apps/tauri/issues/9127 (currently worked-around via fork; fix may be in master, or it might not work)
* bug: failed command during long load never dismisses mutation-wait overlay
* bug: open menu command sometimes opens multiple dialogues
* bug: does not work when core.fsmonitor is true (watchman support not compiled in?)
* edge case: change ids that refer to more than one rev. currently both are selected and the right pane displays an error. 
* edge case: what happens when we snapshot after the CLI does? when there's nothing *to* snapshot, we don't refresh the ui...
* perf: optimise revdetail loads - we already have the header
* perf: better solution to slow immutability check - jj-lib will have a revset contains cache soon
* feat: alternate drag modes for copy/duplicate, maybe for rebase-all-descendants
* feat: log multiselect
* feat: log filters (find commits that change path etc)
* feat: file select/multiselect? large moves could be tedious otherwise. maybe file menu?
* feat: redo/undo stack
* feat: operation menu - restores or views?
* feat: sub-file hunk changes
* feat: diffs and/or difftool
* feat: resolve workflow 
* feat: remotes/fetch/push
* feat: tags display & management
* feat: view commit ids in log (configurable?)
* feat: structured op descs - want to be able to present them more nicely, extracting ids etc. tags?
* feat: more mutations
    - delete local branch
    - drag branches onto each other to create a merge? might be a little too opinionated
* feat: more settings
    - log revsets
* design: decide whether to remove edit menu and maybe add others
* design: draw missing (edge-to-nowhere) graph nodes?
* design: consider common signature control
* epic: categorical expansion - trays, modals, pinned commits etc
* chore: windows codesigning will break in august 2024; needs a new approach