import type { RevHeader } from "../messages/RevHeader";
import type { AbandonRevisions } from "../messages/AbandonRevisions";
import type { CheckoutRevision } from "../messages/CheckoutRevision";
import type { CopyChanges } from "../messages/CopyChanges";
import type { CreateRevision } from "../messages/CreateRevision";
import type { DescribeRevision } from "../messages/DescribeRevision";
import type { DuplicateRevisions } from "../messages/DuplicateRevisions";
import type { MoveChanges } from "../messages/MoveChanges";
import { mutate } from "../ipc";

export default class RevisionMutator {
    #revision: RevHeader;

    constructor(rev: RevHeader) {
        this.#revision = rev;
    }

    // context-free mutations which can be triggered by a menu event
    handle(event: string | undefined) {
        if (!event) {
            return;
        }

        switch (event) {
            case "new":
                this.onNew();
                break;
            case "edit":
                if (!this.#revision.is_immutable) {
                    this.onEdit();
                }
                break;
            case "duplicate":
                this.onDuplicate();
                break;
            case "abandon":
                if (!this.#revision.is_immutable) {
                    this.onAbandon();
                }
                break;
            case "squash":
                if (!this.#revision.is_immutable && this.#revision.parent_ids.length == 1) {
                    this.onSquash();
                }
                break;
            case "restore":
                if (!this.#revision.is_immutable && this.#revision.parent_ids.length == 1) {
                    this.onRestore();
                }
                break;
            default:
                console.log(`unimplemented mutation '${event}'`, this);
        }
    }

    onNew = () => {
        mutate<CreateRevision>("create_revision", {
            parent_ids: [this.#revision.id],
        });
    };

    onEdit = () => {
        mutate<CheckoutRevision>("checkout_revision", {
            id: this.#revision.id,
        });
    };

    onDuplicate = () => {
        mutate<DuplicateRevisions>("duplicate_revisions", {
            ids: [this.#revision.id],
        });
    };

    onAbandon = () => {
        mutate<AbandonRevisions>("abandon_revisions", {
            ids: [this.#revision.id.commit],
        });
    };

    onDescribe = (new_description: string, reset_author: boolean) => {
        mutate<DescribeRevision>("describe_revision", {
            id: this.#revision.id,
            new_description,
            reset_author,
        });
    };

    onSquash = () => {
        mutate<MoveChanges>("move_changes", {
            from_id: this.#revision.id,
            to_id: this.#revision.parent_ids[0],
            paths: []
        });
    };

    onRestore = () => {
        mutate<CopyChanges>("copy_changes", {
            from_id: this.#revision.parent_ids[0],
            to_id: this.#revision.id,
            paths: []
        });
    };
}
