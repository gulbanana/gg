import { mutate } from "../ipc";
import type { Operand } from "../messages/Operand";
import type { MoveChanges } from "../messages/MoveChanges";
import type { MoveRef } from "../messages/MoveRef";
import type { InsertRevision } from "../messages/InsertRevision";
import type { MoveRevision } from "../messages/MoveRevision";
import type { MoveSource } from "../messages/MoveSource";
import type { ChangeId } from "../messages/ChangeId";
import type { CommitId } from "../messages/CommitId";
import RevisionMutator from "./RevisionMutator";
import ChangeMutator from "./ChangeMutator";
import RefMutator from "./RefMutator";
import type { StoreRef } from "../messages/StoreRef";

export type DropEffect = "move" | "copy" | "link";
export type RichText = (string | ChangeId | CommitId | Extract<StoreRef, { type: "LocalBranch" } | { type: "RemoteBranch" }>)[];
export type Eligibility = { type: "yes", hints: Partial<Record<DropEffect, RichText>> } | { type: "maybe", hint: string } | { type: "no" };

export default class BinaryMutator {
    #from: Operand;
    #to: Operand;

    constructor(from: Operand, to: Operand) {
        this.#from = from;
        this.#to = to;
    }

    static canDrag(from: Operand): Eligibility {
        // can't change finalised revs
        if ((from.type == "Revision" || from.type == "Change") && from.header.is_immutable) {
            return { type: "maybe", hint: "(revision is immutable)" };
        }

        // can't change parents of finalised revs
        if (from.type == "Parent" && from.child.is_immutable) {
            return { type: "maybe", hint: "(child is immutable)" };
        } else if (from.type == "Parent" && from.child.parent_ids.length == 1) {
            return { type: "maybe", hint: "(child has only one parent)" };
        }

        // can potentially change revs, their parents, their changes and their refs
        if (from.type == "Revision") {
            return {
                type: "yes", hints: {
                    move: ["Rebasing revision ", from.header.id.change],
                    copy: ["Duplicating revision ", from.header.id.change]
                }
            };
        } else if (from.type == "Parent") {
            return {
                type: "yes", hints: {
                    move: ["Removing parent from revision ", from.child.id.change]
                }
            };
        } else if (from.type == "Change") {
            return {
                type: "yes", hints: {
                    move: [`Squashing changes at ${from.path.relative_path}`]
                }
            };
        } else if (from.type == "Ref" && from.ref.type != "Tag") {
            return {
                type: "yes", hints: {
                    move: ["Moving branch ", from.ref]
                }
            };
        }

        return { type: "no" };
    }

    canDrop(): Eligibility {
        // generic prohibitions - don't drop undroppables, don't drop on yourself
        if (BinaryMutator.canDrag(this.#from).type != "yes" && !(this.#from.type == "Revision" && this.#to.type == "Merge")) {
            return { type: "no" };
        } else if (this.#from == this.#to) {
            return { type: "no" };
        }

        // revs can drop on other revs, inbetween pairs (Parent) or on parent sets (Merge)
        if (this.#from.type == "Revision") {
            if (this.#to.type == "Revision") {
                return {
                    type: "yes", hints: {
                        move: ["Rebasing revision ", this.#from.header.id.change, " onto ", this.#to.header.id.change],
                        copy: ["Duplicating revision ", this.#from.header.id.change, " onto ", this.#to.header.id.change]
                    }
                };
            } else if (this.#to.type == "Parent") {
                if (this.#to.child == this.#from.header) {
                    return { type: "no" };
                } else if (this.#to.child.is_immutable) {
                    return { type: "maybe", hint: "(can't insert before an immutable revision)" };
                } else {
                    return {
                        type: "yes", hints: {
                            move: ["Inserting revision ", this.#from.header.id.change, " before ", this.#to.child.id.change]
                        }
                    };
                }
            } else if (this.#to.type == "Merge") {
                if (this.#to.header.id.change.hex == this.#from.header.id.change.hex) {
                    return { type: "no" };
                } else {
                    return {
                        type: "yes", hints: {
                            move: ["Adding parent to revision ", this.#to.header.id.change]
                        }
                    };
                }
            } else if (this.#to.type == "Repository") {
                return {
                    type: "yes", hints: {
                        move: ["Abandoning commit ", this.#from.header.id.commit]
                    }
                };
            }
        }

        if (this.#from.type == "Parent") {
            if (this.#to.type == "Repository") {
                return {
                    type: "yes", hints: {
                        move: ["Removing parent from revision ", this.#from.child.id.change]
                    }
                };
            }
        }

        if (this.#from.type == "Change") {
            if (this.#to.type == "Revision") {
                if (this.#to.header.id.change.hex == this.#from.header.id.change.hex) {
                    return { type: "no" };
                } else if (this.#to.header.is_immutable) {
                    return { type: "maybe", hint: "(revision is immutable)" };
                } else {
                    return {
                        type: "yes", hints: {
                            move: [`Squashing changes at ${this.#from.path.relative_path} into `, this.#to.header.id.change]
                        }
                    };
                }
            } else if (this.#to.type == "Repository") {
                if (this.#from.header.parent_ids.length == 1) {
                    return {
                        type: "yes", hints: {
                            move: [`Restoring changes at ${this.#from.path.relative_path} from parent `, this.#from.header.parent_ids[0]]
                        }
                    };
                } else {
                    return { type: "maybe", hint: "Can't restore (revision has multiple parents)" };
                }
            }
        }

        if (this.#from.type == "Ref" && this.#from.ref.type != "Tag") {
            // local -> rev: set
            if (this.#to.type == "Revision" && this.#from.ref.type == "LocalBranch") {
                if (this.#to.header.id.change.hex == this.#from.header.id.change.hex) {
                    return { type: "no" };
                } else {
                    return {
                        type: "yes", hints: {
                            move: ["Moving branch ", this.#from.ref, " to ", this.#to.header.id.change]
                        }
                    };
                }
            }

            // remote -> local: track
            else if (this.#to.type == "Ref" && this.#to.ref.type == "LocalBranch" &&
                this.#from.ref.type == "RemoteBranch" && this.#from.ref.branch_name == this.#to.ref.branch_name) {
                if (this.#from.ref.is_tracked) {
                    return { type: "maybe", hint: "(already tracked)" };
                } else {
                    return {
                        type: "yes", hints: {
                            move: ["Tracking remote branch ", this.#from.ref]
                        }
                    };
                }
            }

            // anything -> anywhere: delete
            else if (this.#to.type == "Repository") {
                if (this.#from.ref.type == "LocalBranch") {
                    return {
                        type: "yes", hints: {
                            move: ["Deleting branch ", this.#from.ref]
                        }
                    };
                } else {
                    return {
                        type: "yes", hints: {
                            move: ["Forgetting remote branch ", this.#from.ref]
                        }
                    };
                }
            }
        }

        return { type: "no" };
    }

    doDrop(effect: DropEffect) {
        if (this.#from.type == "Revision") {
            if (this.#to.type == "Revision") {
                if (effect == "move") {
                    // rebase rev onto single target
                    mutate<MoveRevision>("move_revision", { id: this.#from.header.id, parent_ids: [this.#to.header.id] });
                    return;
                } else if (effect == "copy") {
                    console.log("todo: dupe rev");
                }
            } else if (this.#to.type == "Parent") {
                // rebase between targets 
                mutate<InsertRevision>("insert_revision", { id: this.#from.header.id, after_id: this.#to.header.id, before_id: this.#to.child.id });
                return;
            } else if (this.#to.type == "Merge") {
                // rebase subtree onto additional targets
                let newParents = [...this.#to.header.parent_ids, this.#from.header.id.commit];
                mutate<MoveSource>("move_source", { id: this.#to.header.id, parent_ids: newParents });
                return;
            } else if (this.#to.type == "Repository") {
                // abandon source
                new RevisionMutator(this.#from.header).onAbandon();
                return;
            }
        }

        if (this.#from.type == "Parent") {
            if (this.#to.type == "Repository") {
                // rebase subtree onto fewer targets 
                let removeCommit = this.#from.header.id.commit;
                let newParents = this.#from.child.parent_ids.filter(id => id.hex != removeCommit.hex);
                mutate<MoveSource>("move_source", { id: this.#from.child.id, parent_ids: newParents });
                return;
            }
        }

        if (this.#from.type == "Change") {
            if (this.#to.type == "Revision") {
                // squash path to target
                mutate<MoveChanges>("move_changes", { from_id: this.#from.header.id, to_id: this.#to.header.id.commit, paths: [this.#from.path] });
                return;
            } else if (this.#to.type == "Repository") {
                // restore path from source parent to source
                new ChangeMutator(this.#from.header, this.#from.path).onRestore();
                return;
            }
        }

        if (this.#from.type == "Ref") {
            if (this.#to.type == "Revision") {
                // point ref to revision
                mutate<MoveRef>("move_ref", { to_id: this.#to.header.id, ref: this.#from.ref });
                return;
            } else if (this.#to.type == "Ref" && this.#from.ref.type == "RemoteBranch") {
                // track remote branch with existing local
                new RefMutator(this.#from.ref).onTrack();
            } else if (this.#to.type == "Repository") {
                // various kinds of total or partial deletion
                new RefMutator(this.#from.ref).onDelete();
            }
        }

        console.log("error: unknown validated mutation", [effect, this.#from, this.#to]);
    }
}
