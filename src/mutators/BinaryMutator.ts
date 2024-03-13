import type { Operand } from "../messages/Operand";

export type Eligibility = { type: "yes", hint: string } | { type: "maybe", hint: string } | { type: "no" };
export type Hint = Extract<Eligibility, { hint: string }>;

export default class BinaryMutator {
    #from: Operand;
    #to: Operand;

    constructor(from: Operand, to: Operand) {
        this.#from = from;
        this.#to = to;
    }

    static canDrag(from: Operand): Eligibility {
        if ((from.type == "Revision" || from.type == "Change" || from.type == "Parent") && from.header.is_immutable) {
            return { type: "maybe", hint: "(commit is immutable)" };
        }

        if (from.type == "Branch" && from.name.type == "RemoteBranch") {
            return { type: "maybe", hint: "(branch is remote)" };
        }

        return { type: "yes", hint: "" };
    }

    canDrop(): Eligibility {
        if (BinaryMutator.canDrag(this.#from).type != "yes") {
            return { type: "no" };
        }

        if (this.#from == this.#to) {
            return { type: "no" };
        }

        return { type: "yes", "hint": "" };
    }

    doDrop() {
        console.log("dropping " + this.#from.type + " onto " + this.#to.type);
    }
}
