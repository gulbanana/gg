probably mvp
------------
contextless commit actions
- context menu on a revsummary
more actions:
- duplicate
- abandon
- squash
- restore
undo (maybe in repo menu)
dnd rebase 
- drop on node to reparent
- drop on edge to insert
- drag extra parents in to merge
dnd move
- drag files onto commits
branching
- drag chips onto commits - probably no -B required!
- menu?
universal macos builds in CI
snapshot/op-head-merge on focus
investigate interaction with other repo mutators
draw file conflict markers 
disable all commands while a mutation is in progress
decide whether to remove edit menu

possibly not mvp
----------------
bug: failed command during long load never dismisses mutation-wait overlay
bug: selection of first row fails
more settings
- force dark theme on/off
- log revsets
optimise revdetail loads - we already have the header
missing graph nodes?
fix doubled+ open dialogue
log keyboard support
log multiselect
fill out more actions (incl. multiselect)
redo/undo stack
operation menu - restores or views
file menu, file actions
diffs and/or difftool
resolve (other than rebasing)
remotes/fetch/push
design updates 
- edge colours
- conflict markers
- buttons?
- mutability indications
better solution to slow immutability check
tags