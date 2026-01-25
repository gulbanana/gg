import type { RevHeader } from "../messages/RevHeader";
import type { RevSet } from "../messages/RevSet";
import type { ChangeHunk } from "../messages/ChangeHunk";
import type { CopyChanges } from "../messages/CopyChanges";
import type { CopyHunk } from "../messages/CopyHunk";
import type { MoveChanges } from "../messages/MoveChanges";
import type { MoveHunk } from "../messages/MoveHunk";
import type { TreePath } from "../messages/TreePath";
import { mutate } from "../ipc";

export type MutationOptions = { ignoreImmutable?: boolean };

export default class ChangeMutator {
    #revisions: RevHeader[];
    #path: TreePath;
    #hunk: ChangeHunk | null;
    #ignoreImmutable: boolean;

    constructor(revs: RevHeader[], path: TreePath, hunk: ChangeHunk | null = null, ignoreImmutable: boolean) {
        this.#revisions = revs;
        this.#path = path;
        this.#hunk = hunk;
        this.#ignoreImmutable = ignoreImmutable;
    }

    get #singleton() { return this.#revisions.length == 1 ? this.#revisions[0] : null }
    get #set(): RevSet {
        return {
            from: this.#revisions[this.#revisions.length - 1].id,
            to: this.#revisions[0].id,
        };
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
            if (!this.#singleton) {
                return;
            }
            mutate<MoveHunk>("move_hunk", {
                from_id: this.#singleton.id,
                to_id: this.#singleton.parent_ids[0],
                path: this.#path,
                hunk: this.#hunk
            }, { ignoreImmutable: this.#ignoreImmutable });
        } else {
            let oldest = this.#revisions[this.#revisions.length - 1];
            mutate<MoveChanges>("move_changes", {
                from: this.#set,
                to_id: oldest.parent_ids[0],
                paths: [this.#path]
            }, { ignoreImmutable: this.#ignoreImmutable });
        }
    };

    onRestore = () => {
        if (this.#hunk) {
            if (!this.#singleton) {
                return;
            }
            mutate<CopyHunk>("copy_hunk", {
                from_id: this.#singleton.parent_ids[0],
                to_id: this.#singleton.id,
                path: this.#path,
                hunk: this.#hunk
            }, { ignoreImmutable: this.#ignoreImmutable });
        } else {
            let oldest = this.#revisions[this.#revisions.length - 1];
            mutate<CopyChanges>("copy_changes", {
                from_id: oldest.parent_ids[0],
                to_set: this.#set,
                paths: [this.#path]
            }, { ignoreImmutable: this.#ignoreImmutable });
        }
    };
}
