Design Principles
-----------------
The primary metaphor is *direct manipulation*. GG aims to present a view of the repository's 
conceptual contents - revisions, changes to files, synced refs and maybe more - which can be
modified, using right-click and drag-drop, to 'edit' the repo as a whole. 

Jujutsu CLI commands sometimes have a lot of options (`rebase`) or are partially redundant 
for convenience (`move`, `squash`). This is good for scripting, but some use cases demand 
interactivity - reordering multiple commits, for example. Hopefully, `gg` can complement `jj`
by providing decomposed means to achieve some of the same tasks, with immediate visual feedback.

The UI uses a couple of key conventions for discoverability:
- An *actionable object* is represented by an icon followed by a line of text. These are
  drag sources, drop targets and context menu hosts. 
- Chrome and labels are greyscale; anything interactable uses specific colours to indicate
  categories of widget or object.

Architectural Choices
---------------------
In order to create a quality desktop app, a pure webapp is out of scope. However, significant
portions of the code could be reused in a client server app, and we won't introduce *needless*
coupling. `mod worker` and `ipc.ts` are key abstraction boundaries which keep Tauri-specific
code in its own layers.

Each window has a worker thread which owns `Session` data. A session can be in multiple states,
including:
- `WorkerSession` - Opening/reopening a workspace
- `WorkspaceSession` - Workspace open, able to execute mutations
- `QuerySession` - Paged query in progress, able to fetch efficiently

IPC is divided into four categories, which is probably one too many:
- Client->Server **commands** trigger backend actions for native UI integration.
- Client->Server **queries** request information from the session without affecting state.
- Client->Server **mutations** modify session state in a structured fashion.
- Server->Client and Client->Client **events** are broadcast to push information to the UI.

Drag & drop capabilities are implemented by `objects/Object.svelte`, a draggable item, and
`objects/Zone.svelte`, a droppable region. Policy is centralised in `mutators/BinaryMutator.ts`.