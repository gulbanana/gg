import type { Operand } from "../messages/Operand";
import ChangeMutator from "./ChangeMutator";
import RefMutator from "./RefMutator";
import RevisionMutator from "./RevisionMutator";
import WorkspaceMutator from "./WorkspaceMutator";

/**
 * Run a menu command against the mutator which owns the operand's type.
 */
export function handleCommand(operand: Operand, command: string, ignoreImmutable: boolean) {
    switch (operand.type) {
        case "Revision":
            new RevisionMutator([operand.header], ignoreImmutable).handle(command);
            break;

        case "Revisions":
            new RevisionMutator(operand.headers, ignoreImmutable).handle(command);
            break;

        case "Change":
            new ChangeMutator(operand.headers, operand.path, operand.hunk, ignoreImmutable).handle(command);
            break;

        case "Ref":
            new RefMutator(operand.ref, ignoreImmutable).handle(command);
            break;

        case "Workspace":
            new WorkspaceMutator(operand.name).handle(command);
            break;

        default:
            console.log(`unimplemented command '${command}'`, operand);
    }
}
