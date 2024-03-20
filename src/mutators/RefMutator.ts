import type { StoreRef } from "../messages/StoreRef";
import type { TrackBranch } from "../messages/TrackBranch";
import type { UntrackBranch } from "../messages/UntrackBranch";
import type { RenameBranch } from "../messages/RenameBranch";
import type { PushRemote } from "../messages/PushRemote";
import type { FetchRemote } from "../messages/FetchRemote";
import type { DeleteRef } from "../messages/DeleteRef";
import { getInput, mutate } from "../ipc";

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

            case "push-all":
                this.onPushAll();
                break;

            case "push-single":
                this.onPushSingle();
                break;

            case "fetch-all":
                this.onFetchAll();
                break;

            case "fetch-single":
                this.onFetchSingle();
                break;

            case "rename":
                this.onRename();
                break;

            case "delete":
                this.onDelete();
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

    onRename = async () => {
        let response = await getInput("Rename Branch", "", ["Branch Name"]);
        if (response) {
            let new_name = response["Branch Name"];
            mutate<RenameBranch>("rename_branch", {
                ref: this.ref,
                new_name
            })
        }
    };

    onDelete = () => {
        mutate<DeleteRef>("delete_ref", {
            ref: this.ref
        });
    };

    onPushAll = () => {
        switch (this.ref.type) {
            case "Tag":
                console.log("error: Can't push tag");
                break;

            case "RemoteBranch":
                mutate<PushRemote>("push_remote", {
                    remote_name: this.ref.remote_name,
                    ref: this.ref
                });
                break;

            case "LocalBranch":
                for (let remote_name of this.ref.tracking_remotes) {
                    if (!mutate<PushRemote>("push_remote", {
                        remote_name,
                        ref: this.ref
                    })) {
                        return;
                    }
                }
                break;
        }
    };

    onPushSingle = async () => {
        switch (this.ref.type) {
            case "Tag":
            case "RemoteBranch":
                console.log("error: Can't push tag/tracking branch to a specific remote");
                break;

            case "LocalBranch":
                // XXX this should be a dropdown, picking any of the remotes available in $repoConfig
                let response = await getInput("Select Remote", "", ["Remote Name"]);
                if (response) {
                    let remote_name = response["Remote Name"];
                    mutate<PushRemote>("push_remote", {
                        remote_name,
                        ref: this.ref
                    })
                }
                break;
        }
    };

    onFetchAll = () => {
        switch (this.ref.type) {
            case "Tag":
                console.log("error: Can't fetch tag");
                break;

            case "RemoteBranch":
                mutate<FetchRemote>("fetch_remote", {
                    remote_name: this.ref.remote_name,
                    ref: this.ref
                });
                break;

            case "LocalBranch":
                for (let remote_name of this.ref.tracking_remotes) {
                    if (!mutate<FetchRemote>("fetch_remote", {
                        remote_name,
                        ref: this.ref
                    })) {
                        return;
                    }
                }
                break;
        }
    };

    onFetchSingle = async () => {
        switch (this.ref.type) {
            case "Tag":
            case "RemoteBranch":
                console.log("error: Can't fetch tag/tracking branch to a specific remote");
                break;

            case "LocalBranch":
                // XXX this should be a dropdown, picking any of the remotes that have a branch of the right name
                let response = await getInput("Select Remote", "", ["Remote Name"]);
                if (response) {
                    let remote_name = response["Remote Name"];
                    mutate<FetchRemote>("fetch_remote", {
                        remote_name,
                        ref: this.ref
                    })
                }
                break;
        }
    };
}
