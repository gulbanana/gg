## Design Principles

The primary metaphor is _direct manipulation_. GG aims to present a view of the repository's
conceptual contents - revisions, changes to files, synced refs and maybe more - which can be
modified, using right-click and drag-drop, to 'edit' the repo as a whole.

Jujutsu CLI commands sometimes have a lot of options (`rebase`) or are partially redundant
for convenience (`move`, `squash`). This is good for scripting, but some use cases demand
interactivity - reordering multiple commits, for example. Hopefully, `gg` can complement `jj`
by providing decomposed means to achieve some of the same tasks, with immediate visual feedback.

The UI uses a couple of key conventions for discoverability:

- An _actionable object_ is represented by an icon followed by a line of text. These are
  drag sources, drop targets and context menu hosts.
- Chrome and labels are greyscale; anything interactable uses specific colours to indicate
  categories of widget or object states.

## Architectural Choices

In order to create a quality desktop app, a pure webapp is out of scope. However, significant
portions of the code could be reused in a client server app, and we won't introduce _needless_
coupling. `mod worker` and `ipc.ts` are key abstraction boundaries which keep Tauri-specific
code in its own glue layers.

Each window has a worker thread which owns `Session` data. A session can be in multiple states,
including:

- `WorkerSession` - Opening/reopening a workspace
- `WorkspaceSession` - Workspace open, able to execute mutations
- `QuerySession` - Paged query in progress, able to fetch efficiently

IPC is divided into four categories, which is probably one too many:

- Client->Server **triggers** cause the backend to perform native UI actions.
- Client->Server **queries** request information from the session without affecting state.
- Client->Server **mutations** modify session state in a structured fashion.
- Server->Client and Client->Client **events** are broadcast to push information to the UI.

Drag & drop capabilities are implemented by `objects/Object.svelte`, a draggable item, and
`objects/Zone.svelte`, a droppable region. Policy is centralised in `mutators/BinaryMutator.ts`.

## Branch Objects

The representation of branches, in JJ and GG, is a bit complicated; there are multiple state axes.
A repository can have zero or more **remotes**.
A **local branch** can track zero or more of the remotes. (Technically, remote _branches_.)
A **remote branch** can be any of _tracked_ (a flag on the ref), _synced_ (if it points to the same
commit as a local branch of the same name), and _absent_ (if there's a local branch with _no_ ref,
in which case it will be deleted by the CLI on push.

GG attempts to simplify the display of branches by combining refs in the UI. Taking advantage of
Jujutsu's model, which guarantees that a branch name identifies the same branch across remotes, a
local branch and the tracked remote branches with which it is currently synced are be combined into
a single UI object. Remote branches are displayed separately if they're unsynced, untracked or absent.

Consequently, the commands available for a branch as displayed in the UI have polymorphic effect:

1. "Track": Applies to any remote branch that is not already tracked.
2. "Untrack":
   - For a _tracking local/combined branch_, untracks all remotes.
   - For an _unsynced remote branch_, untracks one remote.
3. "Push": Applies to local branches tracking any remotes.
4. "Push to remote...": Applies to local branches when any remotes exist.
5. "Fetch": Downloads for a specific branch only.
   - For a _tracking local/combined branch_, fetches from all remotes.
   - For a _remote branch_, fetches from its remote.
6. "Fetch from remote...": Applies to local branches when any trackable remotes exist.
7. "Rename...": Renames a local branch, without affecting remote branches.

- For a _nontracking local branch_, just renames.
- For a _tracking/combined branch_, untracks first.

8. "Delete": Applies to a user-visible object, not combined objects.
   - For a _local/combined branch_, deletes the local ref.
   - For a _remote branch_, forgets the remote ref (which also clears pending deletes.)

Multiple-dispatch commands:

1. "Move": Drop local branch onto revision. Sets the ref to a commit, potentially de- or re-syncing it.
2. "Track": Drop remote branch onto local of the same name.
3. "Delete": Drag almost any branch out, with polymorphic effect (see above).

Displaying the branch state is a bit fuzzy. The idea is to convey the most useful bits of information at
a glance, and leave the rest to tooltips or context menus. Most branches display in the
"modify" state; "add" and "remove" are used only for _unsynced_ branches, with unsynced locals being "add"
and unsynced or absent remotes "remove".

This is vaguely analogous to the more straightforward use of modify/add/remove for file changes, adapted to
the fact that many branch states are "normal"; the mental shorthand is that add/green means that pushing will
cause a remote to set this ref, and remove/red means the remote will no longer contain this ref (at this pointer).

Additionally, a dashed border (like the dashed lines used for elided commits) has a special meaning, also
fuzzy: this ref is "disconnected", either local-only or remote-only. Disconnected local branches are ones
which have no remotes (in a repo that does have remotes); disconnected remote branches are ones which will
be deleted on push (with an absent local ref).
