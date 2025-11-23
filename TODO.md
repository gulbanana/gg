Planned Features
----------------
> The best laid schemes o' mice an' men / Gang aft a-gley.

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
