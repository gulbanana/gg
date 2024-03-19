import type { StoreRef } from "../messages/StoreRef";
import type { TrackBranch } from "../messages/TrackBranch";
import type { UntrackBranch } from "../messages/UntrackBranch";
import { mutate } from "../ipc";
import type { DeleteRef } from "../messages/DeleteRef";

export default class BranchMutator {
    ref: StoreRef;

    constructor(name: StoreRef) {
        this.ref = name;
    }

    handle(event: string | undefined) {
        if (!event) {
            return;
        }

        switch (event) {
            case "track":
                this.onTrack();
                break;

            case "untrack":
                this.onUntrack();
                break;

            default:
                console.log(`unimplemented mutation '${event}'`, this);
        }
    }

    onTrack = () => {
        mutate<TrackBranch>("track_branch", {
            ref: this.ref
        });
    };

    onUntrack = () => {
        mutate<UntrackBranch>("untrack_branch", {
            ref: this.ref
        });
    };

    onDelete = () => {
        mutate<DeleteRef>("delete_ref", {
            ref: this.ref
        });
    };
}
