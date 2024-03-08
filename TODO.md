probably mvp
------------
wait until https://github.com/tauri-apps/tauri/issues/9127 is fixed or work around it with non-native menus
implement or disable branch & change contextmenus
dnd rebase 
- drop on node to reparent
- drop on edge to insert
- drag extra parents in to merge
dnd move
- drag files onto commits
dnd branch
- drag chips onto commits
snapshot/op-head-merge on focus
investigate interaction with other repo mutators
disable all commands while a mutation is in progress
fix reload reloading original cwd

possibly not mvp
----------------
handle change ids that refer to more than one rev
sort out edge cases of selection, for example "nothing selected yet" or a new query that doesn't include the selection
bug: failed command during long load never dismisses mutation-wait overlay
bug: open menu command sometimes opens multiple dialogues
decide whether to remove edit menu
app icon
more settings
- force dark theme on/off
- log revsets
optimise revdetail loads - we already have the header
missing graph nodes?
log keyboard support
log multiselect
more rev/change/branch actions (incl. multiselect)
redo/undo stack
operation menu - restores or views
diffs and/or difftool
resolve (other than rebasing)
remotes/fetch/push
better solution to slow immutability check
tags
design updates 
- edge colours
- conflict markers
- buttons?
- mutability indications
