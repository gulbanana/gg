import type { StoreRef } from "../messages/StoreRef";
import type { TrackBranch } from "../messages/TrackBranch";
import type { UntrackBranch } from "../messages/UntrackBranch";
import type { RenameBranch } from "../messages/RenameBranch";
import type { GitPush } from "../messages/GitPush";
import type { GitFetch } from "../messages/GitFetch";
import type { DeleteRef } from "../messages/DeleteRef";
import { getInput, mutate, query } from "../ipc";

export default class RefMutator {
    #ref: StoreRef;

    constructor(name: StoreRef) {
        this.#ref = name;
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
            ref: this.#ref
        });
    };

    onUntrack = () => {
        mutate<UntrackBranch>("untrack_branch", {
            ref: this.#ref
        });
    };

    onRename = async () => {
        let response = await getInput("Rename Bookmark", "", ["Bookmark Name"]);
        if (response) {
            let new_name = response["Bookmark Name"];
            mutate<RenameBranch>("rename_branch", {
                ref: this.#ref,
                new_name
            })
        }
    };

    onDelete = () => {
        mutate<DeleteRef>("delete_ref", {
            ref: this.#ref
        });
    };

    onPushAll = () => {
        switch (this.#ref.type) {
            case "Tag":
                console.log("error: Can't push tag");
                break;

            case "RemoteBookmark":
                mutate<GitPush>("git_push", {
                    type: "RemoteBookmark",
                    remote_name: this.#ref.remote_name,
                    branch_ref: this.#ref
                });
                break;

            case "LocalBookmark":
                mutate<GitPush>("git_push", {
                    type: "AllRemotes",
                    branch_ref: this.#ref
                });
                break;
        }
    };

    onPushSingle = async () => {
        switch (this.#ref.type) {
            case "Tag":
                console.log("error: Can't push tag to a specific remote");
                break;

            case "RemoteBookmark":
                console.log("error: Can't push tracking bookmark to a specific remote");
                break;

            case "LocalBookmark":
                let allRemotes = await query<string[]>("query_remotes", { tracking_branch: null });
                if (allRemotes.type == "error") {
                    console.log("error loading remotes: " + allRemotes.message);
                    return;
                }

                let response = await getInput("Select Remote", "", [{ label: "Remote Name", choices: allRemotes.value }]);
                if (response) {
                    let remote_name = response["Remote Name"];
                    mutate<GitPush>("git_push", {
                        type: "RemoteBookmark",
                        remote_name,
                        branch_ref: this.#ref
                    })
                }
                break;
        }
    };

    onFetchAll = () => {
        switch (this.#ref.type) {
            case "Tag":
                console.log("error: Can't fetch tag");
                break;

            case "RemoteBookmark":
                mutate<GitFetch>("git_fetch", {
                    type: "AllRemotes",
                    branch_ref: this.#ref
                });
                break;

            case "LocalBookmark":
                mutate<GitFetch>("git_fetch", {
                    type: "AllRemotes",
                    branch_ref: this.#ref
                });
                break;
        }
    };

    onFetchSingle = async () => {
        switch (this.#ref.type) {
            case "Tag":
                console.log("error: Can't fetch tag from a specific remote");
                break;

            case "RemoteBookmark":
                console.log("error: Can't fetch tracking bookmark from a specific remote");
                break;

            case "LocalBookmark":
                let trackedRemotes = await query<string[]>("query_remotes", { tracking_branch: this.#ref.branch_name });
                if (trackedRemotes.type == "error") {
                    console.log("error loading remotes: " + trackedRemotes.message);
                    return;
                }

                let response = await getInput("Select Remote", "", [{ label: "Remote Name", choices: trackedRemotes.value }]);
                if (response) {
                    let remote_name = response["Remote Name"];
                    mutate<GitFetch>("git_fetch", {
                        type: "RemoteBookmark",
                        remote_name,
                        branch_ref: this.#ref
                    })
                }
                break;
        }
    };
}
