MVP requirements
----------------
drag & drop interactions:
- make parents their own context?
- revision to node rebase
- revision to edge insert
- revision to parents add-parent
- parents-out remove-parent (not if last!)
- branch to revision set
- branch-out delete? or not mvp
- change to revision squash
- stretch: change to revision restore
- stretch: revision to revision/edge duplicate

Ideas and plans
---------------
These changes may or may not be implemented in the future.
* bug: proper fix for https://github.com/tauri-apps/tauri/issues/9127 (currently worked-around via fork; fix may be in master, or it might not work)
* bug: failed command during long load never dismisses mutation-wait overlay
* bug: open menu command sometimes opens multiple dialogues
* edge case: change ids that refer to more than one rev. currently both are selected and the right pane displays an error
    - might require a more rigorous treatment of ids. always abandon-by-commit?
* edge case: selection issues like "nothing selected yet" or a new query that doesn't include the selection. this might be fine as-is
* edge case: what happens when we snapshot after the CLI does? when there's nothing *to* snapshot, we don't refresh the ui...
* perf: optimise revdetail loads - we already have the header
* perf: better solution to slow immutability check - jj-lib will have a revset contains cache soon
* feat: log multiselect
* feat: log filters (find commits that change path etc)
* feat: file select/multiselect? large moves could be tedious otherwise. maybe file menu?
* feat: redo/undo stack
* feat: operation menu - restores or views?
* feat: diffs and/or difftool
* feat: resolve workflow 
* feat: remotes/fetch/push
* feat: tags display & management
* feat: more context actions 
    - push branch
    - delete local branch
* feat: more settings
    - log revsets
* design: decide whether to remove edit menu and maybe add others
* design: draw missing (edge-to-nowhere) graph nodes?
* design: consider common signature control
* epic: categorical expansion - trays, modals, pinned commits etc
* chore: windows codesigning will break in august 2024; needs a new approach