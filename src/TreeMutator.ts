import type { RevHeader } from "./messages/RevHeader";
import type { CopyChanges } from "./messages/CopyChanges";
import type { MoveChanges } from "./messages/MoveChanges";
import { mutate } from "./ipc";
import type { TreePath } from "./messages/TreePath";

export default class TreeMutator {
    #revision: RevHeader;
    #path: TreePath;

    constructor(rev: RevHeader, path: TreePath) {
        this.#revision = rev;
        this.#path = path;
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
        mutate<MoveChanges>("move_changes", {
            from_change_id: this.#revision.change_id,
            to_id: this.#revision.parent_ids[0],
            paths: [this.#path]
        });
    }

    onRestore = () => {
        mutate<CopyChanges>("copy_changes", {
            from_change_id: this.#revision.parent_ids[0],
            to_id: this.#revision.change_id,
            paths: [this.#path]
        });
    }
}