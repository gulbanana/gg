import type { RevHeader } from "../messages/RevHeader";
import type { RevId } from "../messages/RevId";
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
            case "new_after_parent_0":
                console.log("new_after_parent_0 invoking");
                this.onNewAfterParent0();
                break;
            case "edit":
                if (!this.#revision.is_immutable) {
                    this.onEdit();
                }
                break;
            case "backout":
                this.onBackout();
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
            case "branch":
                this.onBranch();
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

    onNewBefore = (successor: RevId) => {
        mutate<CreateRevisionBetween>("create_revision_between", {
            after_id: this.#revision.id,
            before_id: successor
        });
    };

    onNewAfterParent0 = () => {
        mutate<CreateRevisionBetween>("create_revision_between", {
            before_id: this.#revision.id,
            after_id: this.#revision.parent_ids[0]
        });
    };

    onEdit = () => {
        if (this.#revision.is_working_copy) {
            return;
        }

        if (this.#revision.is_immutable) {
            mutate<CreateRevision>("create_revision", {
                parent_ids: [this.#revision.id],
            });
        } else {
            mutate<CheckoutRevision>("checkout_revision", {
                id: this.#revision.id,
            });
        }
    };

    onBackout = () => {
        mutate<BackoutRevisions>("backout_revisions", {
            ids: [this.#revision.id],
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

    onBranch = async () => {
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
            mutate<CreateRef>("create_ref", { ref, id: this.#revision.id })
        }
    }
}
