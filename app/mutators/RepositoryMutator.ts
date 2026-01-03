import { getInput, mutate, trigger } from "../ipc";
import type { CloneRepository } from "../messages/CloneRepository";
import type { InitRepository } from "../messages/InitRepository";
import type { RepoEvent } from "../messages/RepoEvent";

export default class RepositoryMutator {
    async handle(event: RepoEvent) {
        switch (event.type) {
            case "CloneURL":
                await this.#handleUrl();
                break;

            case "CloneConfirm":
                await this.#handleClone(event.url, event.path);
                break;

            case "InitConfirm":
                await this.#handleInit(event.path, event.has_git);
                break;
        }
    }

    async #handleUrl() {
        let fields = [
            { label: "Repository", choices: [] }
        ];

        let result = await getInput("Clone Repository", "", fields);
        if (!result) return;

        let url = result["Repository"].trim();

        trigger("forward_clone_url", { url });
    }

    async #handleClone(originalUrl: string, originalPath: string) {
        let fields = [
            { label: "Repository", choices: [originalUrl] },
            { label: "Destination", choices: [originalPath] },
            { label: "Colocated", choices: ["false", "true"] },
        ];

        let result = await getInput("Clone Repository", "", fields);
        if (!result) return;

        let url = result["Repository"].trim();
        let path = result["Destination"].trim();
        let colocated = result["Colocated"] === "true";

        mutate<CloneRepository>("clone_repository", {
            url,
            path,
            colocated,
        }, { operation: "Cloning..." });
    }

    async #handleInit(originalPath: string, hasGit: boolean) {
        let fields = [
            { label: "Destination", choices: [originalPath] },
            { label: "Colocated", choices: hasGit ? ["true", "false"] : ["false", "true"] },
        ];

        let result = await getInput("Initialize Repository", "", fields);
        if (!result) return;

        let path = result["Destination"];
        let colocated = result["Colocated"] === "true";

        mutate<InitRepository>("init_repository", {
            path,
            colocated,
        });
    }
}
