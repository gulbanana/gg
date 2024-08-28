Known Issues
------------
* "Open..." menu command sometimes opens multiple dialogues.
* Mutations can fail due to ambiguity when there are other writers; this should update the UI. Maybe a special From impl for resolve_change.
* Windows codesigning will break in August 2024; the CI needs a new approach.
* On Webkit (macos and linux), horizontal scrollbars in diffs are too tall. 
* Visual issues on Xubuntu 22.04:
  - menu leaves a white background when there's no repo loaded - no xdamage maybe?
  - there's a weird bullet (looks like an uncoloured rev icon) in the sig area
  - fonts are kind of awful

Planned Features
----------------
> The best laid schemes o' mice an' men / Gang aft a-gley.

* Hunk selection/operations. Maybe a change/hunk menu.
* Alternate drag modes for copy/duplicate, perhaps  rebase-all-descendants.
* Optimise revdetail loads - we already have the header available.
* Multiselection, viewing and operating on revsets or changesets.
* Undo/redo stack, possibly with a menu of recent ops.
* Some way to access the resolve (mergetool) workflow. Difftools too, although this is less useful.
* More stuff in the log - timestamps, commit ids... this might have to be configurable. 
* Progress bar, particularly for git and snapshot operations.
* Structured op descriptions - extracted ids etc, maybe via tags. This would benefit from being in JJ core.
* "Onboarding" features - init/clone/colocate.
* Relative timestamps should update on refocus.

UI Expansion
------------
With some dynamic way to show extra panes, replace content, open new windows &c, more useful features would be possible:

* View the repo at past ops.
* View a revision at past evolutions (possibly this could be folded into the log).
* Config UI, both for core stuff and gg's own settings.
* Revision pinning for split/comparison workflows.
