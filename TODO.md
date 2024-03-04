probably mvp
------------
use immutability for action vis
    - partially done - new/edit buttons only
contextless commit actions (good initial candidates: reset author, squash)
    - commit menu
    - context menu on a revsummary
contextless file actions
    - restore
undo (maybe in repo menu)
dnd rebase 
    - drop on node to reparent
    - drop on edge to insert
    - drag extra parents in to merge
dnd move
    - drag files onto commits
dnd branching
    - drag tags onto commits - probably no -B required!
universal macos builds in CI
snapshot/op-head-merge on focus
investigate interaction with other repo mutators
draw file conflict markers 

possibly not mvp
----------------
bug: failed command during long load never dismisses mutation-wait overlay
more settings (force dark theme on/off)
optimise revdetail loads - we already have the header
missing graph nodes
fix doubled+ open dialogue
log keyboard support
log multiselect
redo/undo stack
operation menu - restores or views
file menu, file actions
fill out more commit actions (incl. multiselect)
fill out more dnd actions (incl. multiselect)
diffs and/or difftool
resolve (other than rebasing)
remotes/fetch/push
design updates 
    - edge colours
    - conflict markers
    - buttons?
better solution to slow immutability check