import type { Operand } from "../messages/Operand";

export default class BinaryMutator {
    #from: Operand;
    #to: Operand;

    constructor(from: Operand, to: Operand) {
        this.#from = from;
        this.#to = to;
    }

    canDrop(): boolean {
        return this.#from != this.#to;
    }

    doDrop() {
        console.log("dropping " + this.#from.type + " onto " + this.#to.type);
    }
}
