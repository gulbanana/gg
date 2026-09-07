import type { ForgetWorkspace } from "../messages/ForgetWorkspace";
import type { OpenWorkspace } from "../messages/OpenWorkspace";
import type { RenameWorkspace } from "../messages/RenameWorkspace";
import { getInput, mutate } from "../ipc";

export default class WorkspaceMutator {
    #name: string;

    constructor(name: string) {
        this.#name = name;
    }

    handle(event: string | undefined) {
        if (!event) {
            return;
        }

        switch (event) {
            case "open":
                this.onOpen();
                break;

            case "forget":
                this.onForget();
                break;

            case "rename":
                this.onRename();
                break;

            default:
                console.log(`unimplemented mutation '${event}'`, this);
        }
    }

    onOpen = () => {
        mutate<OpenWorkspace>("open_workspace", {
            name: this.#name,
        });
    };

    onForget = () => {
        mutate<ForgetWorkspace>("forget_workspace", {
            name: this.#name,
        });
    };

    onRename = async () => {
        let response = await getInput("Rename Workspace", "", ["Workspace Name"]);
        if (response) {
            let new_name = response["Workspace Name"];
            mutate<RenameWorkspace>("rename_workspace", {
                name: this.#name,
                new_name,
            });
        }
    };
}
