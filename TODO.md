probably mvp
------------
dnd rebase 
- drop on node to reparent
- drop on edge to insert
- drag extra parents in to merge
dnd move
- drag files onto commits
snapshot/op-head-merge on focus
investigate interaction with other repo mutators
draw file conflict markers 
disable all commands while a mutation is in progress
fix reload reloading original cwd

possibly not mvp
----------------
sort out edge cases of selection, for example "nothing selected yet" or a new query that doesn't include the selection
branch management
- dnd (drag chips onto commits, probably no -B required)
- menu
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
fill out more actions (incl. multiselect)
redo/undo stack
operation menu - restores or views
file menu, file actions
diffs and/or difftool
resolve (other than rebasing)
remotes/fetch/push
better solution to slow immutability check
tags
context menu event store is dubious. it only works because the top-level handler clears events after reading them
design updates 
- edge colours
- conflict markers
- buttons?
- mutability indications
