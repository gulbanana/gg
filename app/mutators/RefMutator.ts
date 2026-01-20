import type { StoreRef } from "../messages/StoreRef";
import type { TrackBookmark } from "../messages/TrackBookmark";
import type { UntrackBookmark } from "../messages/UntrackBookmark";
import type { RenameBookmark } from "../messages/RenameBookmark";
import type { GitPush } from "../messages/GitPush";
import type { GitFetch } from "../messages/GitFetch";
import type { DeleteRef } from "../messages/DeleteRef";
import { getInput, mutate, query } from "../ipc";

export type MutationOptions = { ignoreImmutable?: boolean };

export default class RefMutator {
    #ref: StoreRef;
    #ignoreImmutable: boolean;

    constructor(name: StoreRef, ignoreImmutable: boolean) {
        this.#ref = name;
        this.#ignoreImmutable = ignoreImmutable;
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

    onTrack = (options?: MutationOptions) => {
        mutate<TrackBookmark>("track_bookmark", {
            ref: this.#ref
        }, options);
    };

    onUntrack = (options?: MutationOptions) => {
        mutate<UntrackBookmark>("untrack_bookmark", {
            ref: this.#ref
        }, options);
    };

    onRename = async (options?: MutationOptions) => {
        let response = await getInput("Rename Bookmark", "", ["Bookmark Name"]);
        if (response) {
            let new_name = response["Bookmark Name"];
            mutate<RenameBookmark>("rename_bookmark", {
                ref: this.#ref,
                new_name
            }, options)
        }
    };

    onDelete = (options?: MutationOptions) => {
        mutate<DeleteRef>("delete_ref", {
            ref: this.#ref
        }, options);
    };

    onPushAll = (options?: MutationOptions) => {
        switch (this.#ref.type) {
            case "Tag":
                console.log("error: Can't push tag");
                break;

            case "RemoteBookmark":
                mutate<GitPush>("git_push", {
                    refspec: {
                        type: "RemoteBookmark",
                        remote_name: this.#ref.remote_name,
                        bookmark_ref: this.#ref
                    },
                    input: null
                }, { ...options, operation: `Pushing ${this.#ref.bookmark_name} to ${this.#ref.remote_name}...` });
                break;

            case "LocalBookmark":
                mutate<GitPush>("git_push", {
                    refspec: {
                        type: "AllRemotes",
                        bookmark_ref: this.#ref
                    },
                    input: null
                }, { ...options, operation: `Pushing ${this.#ref.bookmark_name}...` });
                break;
        }
    };

    onPushSingle = async (options?: MutationOptions) => {
        switch (this.#ref.type) {
            case "Tag":
                console.log("error: Can't push tag to a specific remote");
                break;

            case "RemoteBookmark":
                console.log("error: Can't push tracking bookmark to a specific remote");
                break;

            case "LocalBookmark":
                let allRemotes = await query<string[]>("query_remotes", { tracking_bookmark: null });
                if (allRemotes.type == "error") {
                    console.log("error loading remotes: " + allRemotes.message);
                    return;
                }

                let response = await getInput("Select Remote", "", [{ label: "Remote Name", choices: allRemotes.value }]);
                if (response) {
                    let remote_name = response["Remote Name"];
                    mutate<GitPush>("git_push", {
                        refspec: {
                            type: "RemoteBookmark",
                            remote_name,
                            bookmark_ref: this.#ref
                        },
                        input: null
                    }, { ...options, operation: `Pushing ${this.#ref.bookmark_name} to ${remote_name}...` })
                }
                break;
        }
    };

    onFetchAll = (options?: MutationOptions) => {
        switch (this.#ref.type) {
            case "Tag":
                console.log("error: Can't fetch tag");
                break;

            case "RemoteBookmark":
                mutate<GitFetch>("git_fetch", {
                    refspec: {
                        type: "AllRemotes",
                        bookmark_ref: this.#ref
                    },
                    input: null
                }, { ...options, operation: `Fetching ${this.#ref.bookmark_name} from ${this.#ref.bookmark_name}...` });
                break;

            case "LocalBookmark":
                mutate<GitFetch>("git_fetch", {
                    refspec: {
                        type: "AllRemotes",
                        bookmark_ref: this.#ref
                    },
                    input: null
                }, { ...options, operation: `Fetching ${this.#ref.bookmark_name}...` });
                break;
        }
    };

    onFetchSingle = async (options?: MutationOptions) => {
        switch (this.#ref.type) {
            case "Tag":
                console.log("error: Can't fetch tag from a specific remote");
                break;

            case "RemoteBookmark":
                console.log("error: Can't fetch tracking bookmark from a specific remote");
                break;

            case "LocalBookmark":
                let trackedRemotes = await query<string[]>("query_remotes", { tracking_bookmark: this.#ref.bookmark_name });
                if (trackedRemotes.type == "error") {
                    console.log("error loading remotes: " + trackedRemotes.message);
                    return;
                }

                let response = await getInput("Select Remote", "", [{ label: "Remote Name", choices: trackedRemotes.value }]);
                if (response) {
                    let remote_name = response["Remote Name"];
                    mutate<GitFetch>("git_fetch", {
                        refspec: {
                            type: "RemoteBookmark",
                            remote_name,
                            bookmark_ref: this.#ref
                        },
                        input: null
                    }, { ...options, operation: `Fetching ${this.#ref.bookmark_name} from ${remote_name}...` })
                }
                break;
        }
    };
}
