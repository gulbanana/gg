import type { RevHeader } from "../messages/RevHeader";
import type { RevSet } from "../messages/RevSet";
import type { AbandonRevisions } from "../messages/AbandonRevisions";
import type { BackoutRevisions } from "../messages/BackoutRevisions";
import type { CheckoutRevision } from "../messages/CheckoutRevision";
import type { CopyChanges } from "../messages/CopyChanges";
import type { CreateRevision } from "../messages/CreateRevision";
import type { CreateRevisionBetween } from "../messages/CreateRevisionBetween";
import type { DescribeRevision } from "../messages/DescribeRevision";
import type { DuplicateRevisions } from "../messages/DuplicateRevisions";
import type { MoveChanges } from "../messages/MoveChanges";
import type { CreateRef } from "../messages/CreateRef";
import { getInput, mutate } from "../ipc";
import type { StoreRef } from "../messages/StoreRef";

export default class RevisionMutator {
    #revisions: RevHeader[];

    constructor(revisions: RevHeader[]) {
        this.#revisions = revisions;
    }

    get #singleton() { return this.#revisions.length == 1 ? this.#revisions[0] : null }
    get #set(): RevSet {
        return {
            from: this.#revisions[this.#revisions.length - 1].id,
            to: this.#revisions[0].id,
        };
    }

    // context-free mutations which can be triggered by a menu event
    handle(event: string | undefined) {
        if (!event) {
            return;
        }

        switch (event) {
            case "new_child":
                this.onNewChild();
                break;
            case "new_parent":
                this.onNewParent();
                break;
            case "edit":
                this.onEdit();
                break;
            case "backout":
                this.onBackout();
                break;
            case "duplicate":
                this.onDuplicate();
                break;
            case "abandon":
                this.onAbandon();
                break;
            case "squash":
                this.onSquash();
                break;
            case "restore":
                this.onRestore();
                break;
            case "branch":
                this.onBranch();
                break;
            default:
                console.log(`unimplemented mutation '${event}'`, this);
        }
    }

    onNewChild = () => {
        mutate<CreateRevision>("create_revision", {
            set: this.#set
        });
    };

    onNewParent = () => {
        if (!this.#singleton) return;
        mutate<CreateRevisionBetween>("create_revision_between", {
            before_id: this.#singleton.id,
            after_id: this.#singleton.parent_ids[0]
        });
    };

    onEdit = () => {
        if (!this.#singleton) return;
        if (this.#singleton.is_working_copy) {
            return;
        }

        if (this.#singleton.is_immutable) {
            mutate<CreateRevision>("create_revision", {
                set: this.#set,
            });
        } else {
            mutate<CheckoutRevision>("checkout_revision", {
                id: this.#singleton.id,
            });
        }
    };

    onBackout = () => {
        if (!this.#singleton) return;
        mutate<BackoutRevisions>("backout_revisions", {
            ids: [this.#singleton.id],
        });
    };

    onDuplicate = () => {
        if (!this.#singleton) return;
        mutate<DuplicateRevisions>("duplicate_revisions", {
            ids: [this.#singleton.id],
        });
    };

    onAbandon = () => {
        if (this.#revisions.some(r => r.is_immutable)) return;
        mutate<AbandonRevisions>("abandon_revisions", {
            set: this.#set,
        });
    };

    onDescribe = (new_description: string, reset_author: boolean) => {
        if (!this.#singleton) return;
        mutate<DescribeRevision>("describe_revision", {
            id: this.#singleton.id,
            new_description,
            reset_author,
        });
    };

    onSquash = () => {
        mutate<MoveChanges>("move_changes", {
            from: this.#set,
            to_id: this.#revisions[this.#revisions.length - 1].parent_ids[0],
            paths: []
        });
    };

    onRestore = () => {
        if (!this.#singleton || this.#singleton.is_immutable || this.#singleton.parent_ids.length != 1) return;
        mutate<CopyChanges>("copy_changes", {
            from_id: this.#singleton.parent_ids[0],
            to_id: this.#singleton.id,
            paths: []
        });
    };

    onBranch = async () => {
        if (!this.#singleton) return;
        let response = await getInput("Create Bookmark", "", ["Bookmark Name"]);
        if (response) {
            let ref: StoreRef = {
                type: "LocalBookmark",
                branch_name: response["Bookmark Name"],
                has_conflict: false,
                is_synced: false,
                potential_remotes: 0,
                available_remotes: 0,
                tracking_remotes: []
            };
            mutate<CreateRef>("create_ref", { ref, id: this.#singleton.id })
        }
    }
}
