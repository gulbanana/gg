import type { RevHeader } from "./messages/RevHeader";
import type { RefName } from "./messages/RefName";
import type { TrackBranch } from "./messages/TrackBranch";
import { mutate } from "./ipc";
import type { UntrackBranch } from "./messages/UntrackBranch";

export default class BranchMutator {
    #revision: RevHeader;
    #name: RefName;

    constructor(rev: RevHeader, name: RefName) {
        this.#revision = rev;
        this.#name = name;
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
            name: this.#name
        });
    }

    onUntrack = () => {
        mutate<UntrackBranch>("untrack_branch", {
            name: this.#name
        });
    }
}