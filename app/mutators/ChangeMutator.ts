import type { RevHeader } from "../messages/RevHeader";
import type { ChangeHunk } from "../messages/ChangeHunk";
import type { CopyChanges } from "../messages/CopyChanges";
import type { CopyHunk } from "../messages/CopyHunk";
import type { MoveChanges } from "../messages/MoveChanges";
import type { MoveHunk } from "../messages/MoveHunk";
import type { TreePath } from "../messages/TreePath";
import { mutate } from "../ipc";

export default class ChangeMutator {
    #revision: RevHeader;
    #path: TreePath;
    #hunk: ChangeHunk | null;

    constructor(rev: RevHeader, path: TreePath, hunk: ChangeHunk | null = null) {
        this.#revision = rev;
        this.#path = path;
        this.#hunk = hunk;
    }

    handle(event: string | undefined) {
        if (!event) {
            return;
        }

        switch (event) {
            case "squash":
                this.onSquash();
                break;
            case "restore":
                this.onRestore();
                break;
            default:
                console.log(`unimplemented mutation '${event}'`, this);
        }
    }

    onSquash = () => {
        if (this.#hunk) {
            mutate<MoveHunk>("move_hunk", {
                from_id: this.#revision.id,
                to_id: this.#revision.parent_ids[0],
                path: this.#path,
                hunk: this.#hunk
            });
        } else {
            mutate<MoveChanges>("move_changes", {
                from: { from: this.#revision.id, to: this.#revision.id },
                to_id: this.#revision.parent_ids[0],
                paths: [this.#path]
            });
        }
    };

    onRestore = () => {
        if (this.#hunk) {
            mutate<CopyHunk>("copy_hunk", {
                from_id: this.#revision.parent_ids[0],
                to_id: this.#revision.id,
                path: this.#path,
                hunk: this.#hunk
            });
        } else {
            mutate<CopyChanges>("copy_changes", {
                from_id: this.#revision.parent_ids[0],
                to_set: { from: this.#revision.id, to: this.#revision.id },
                paths: [this.#path]
            });
        }
    };
}
