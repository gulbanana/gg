import type { ChangeId } from "./messages/ChangeId";

// compares both hex and offset to handle divergent changes
export function sameChange(a: Pick<ChangeId, "hex" | "offset">, b: Pick<ChangeId, "hex" | "offset">): boolean {
    return a.hex == b.hex && a.offset == b.offset;
}
