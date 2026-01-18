import type { ChangeId } from "./messages/ChangeId";

// compares both hex and offset to handle divergent changes
export function sameChange(a: ChangeId, b: ChangeId): boolean {
    return a.hex == b.hex && a.offset == b.offset;
}
