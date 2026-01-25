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
    #ignoreImmutable: boolean;

    constructor(revisions: RevHeader[], ignoreImmutable: boolean) {
        this.#revisions = revisions;
        this.#ignoreImmutable = ignoreImmutable;
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
            case "revert":
                this.onRevert();
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
            case "bookmark":
                this.onBookmark();
                break;
            default:
                console.log(`unimplemented mutation '${event}'`, this);
        }
    }

    onNewChild = () => {
        mutate<CreateRevision>("create_revision", {
            set: this.#set
        }, { ignoreImmutable: this.#ignoreImmutable });
    };

    onNewParent = () => {
        let oldest = this.#revisions[this.#revisions.length - 1];
        mutate<CreateRevisionBetween>("create_revision_between", {
            before_id: oldest.id,
            after_id: oldest.parent_ids[0]
        }, { ignoreImmutable: this.#ignoreImmutable });
    };

    onEdit = () => {
        if (!this.#singleton) return;
        if (this.#singleton.is_working_copy) {
            return;
        }

        if (this.#singleton.is_immutable && !this.#ignoreImmutable) {
            mutate<CreateRevision>("create_revision", {
                set: this.#set,
            }, { ignoreImmutable: this.#ignoreImmutable });
        } else {
            mutate<CheckoutRevision>("checkout_revision", {
                id: this.#singleton.id,
            }, { ignoreImmutable: this.#ignoreImmutable });
        }
    };

    onRevert = () => {
        mutate<BackoutRevisions>("backout_revisions", {
            set: this.#set,
        }, { ignoreImmutable: this.#ignoreImmutable });
    };

    onDuplicate = () => {
        mutate<DuplicateRevisions>("duplicate_revisions", {
            set: this.#set,
        }, { ignoreImmutable: this.#ignoreImmutable });
    };

    onAbandon = () => {
        mutate<AbandonRevisions>("abandon_revisions", {
            set: this.#set,
        }, { ignoreImmutable: this.#ignoreImmutable });
    };

    onDescribe = (new_description: string, reset_author: boolean) => {
        if (!this.#singleton) return;
        mutate<DescribeRevision>("describe_revision", {
            id: this.#singleton.id,
            new_description,
            reset_author,
        }, { ignoreImmutable: this.#ignoreImmutable });
    };

    onSquash = () => {
        mutate<MoveChanges>("move_changes", {
            from: this.#set,
            to_id: this.#revisions[this.#revisions.length - 1].parent_ids[0],
            paths: []
        }, { ignoreImmutable: this.#ignoreImmutable });
    };

    onRestore = () => {
        let oldest = this.#revisions[this.#revisions.length - 1];
        if (oldest.parent_ids.length != 1) return;
        mutate<CopyChanges>("copy_changes", {
            from_id: oldest.parent_ids[0],
            to_set: this.#set,
            paths: []
        }, { ignoreImmutable: this.#ignoreImmutable });
    };

    onBookmark = async () => {
        if (!this.#singleton) return;
        let response = await getInput("Create Bookmark", "", ["Bookmark Name"]);
        if (response) {
            let ref: StoreRef = {
                type: "LocalBookmark",
                bookmark_name: response["Bookmark Name"],
                has_conflict: false,
                is_synced: false,
                potential_remotes: 0,
                available_remotes: 0,
                tracking_remotes: []
            };
            mutate<CreateRef>("create_ref", { ref, id: this.#singleton.id }, { ignoreImmutable: this.#ignoreImmutable })
        }
    }
}
