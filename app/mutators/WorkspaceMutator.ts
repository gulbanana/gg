import type { ForgetWorkspace } from "../messages/ForgetWorkspace";
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
            case "rename":
                this.onRename();
                break;

            case "forget":
                this.onForget();
                break;

            default:
                console.log(`unimplemented mutation '${event}'`, this);
        }
    }

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

    onForget = () => {
        mutate<ForgetWorkspace>("forget_workspace", {
            name: this.#name,
        });
    };
}
