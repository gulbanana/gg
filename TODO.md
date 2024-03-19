Desirable things
----------------

These changes may or may not be implemented in the future.
* bug: proper fix for https://github.com/tauri-apps/tauri/issues/9127 (currently worked-around via fork; fix may be in beta12, or it might not work)
* bug: open menu command sometimes opens multiple dialogues
* edge case: mutations can fail due to ambiguity due to other writers; this should update the UI. maybe use a special From on resolve_change
* perf: optimise revdetail loads - we already have the header
* perf: better solution to slow immutability check - jj-lib will have a revset contains cache soon
* feat: alternate drag modes for copy/duplicate, maybe for rebase-all-descendants
* feat: log multiselect
* feat: file select/multiselect? large moves could be tedious otherwise. maybe file menu?
* feat: redo/undo stack
* feat: operation menu - restores or views?
* feat: sub-file hunk changes
* feat: diffs and/or difftool
* feat: resolve workflow 
* feat: view commit ids in log (configurable?)
* feat: view repo at different ops (slider? entire pane?) 
* feat: progress display (probably in statusbar); useful for git & snapshot
* feat: finish branch management
* feat: structured op descs - want to be able to present them more nicely, extracting ids etc. tags? 
    - there's a request for this to be part of jj
* feat: more mutations
    - make track available to recreate deleting remotes
    - delete branch
    - push branch 
    - fetch branch?
    - create/delete tags? even moving them is implemented, but may be a bad idea
* feat: more settings
    - log revsets
* design: decide whether to remove edit menu and maybe add others
* design: consider common signature control
* epic: categorical expansion - trays, modals, pinned commits etc
* chore: windows codesigning will break in august 2024; needs a new approach