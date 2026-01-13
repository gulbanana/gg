import { mutate } from "../ipc";
import type { Operand } from "../messages/Operand";
import type { MoveChanges } from "../messages/MoveChanges";
import type { MoveHunk } from "../messages/MoveHunk";
import type { MoveRef } from "../messages/MoveRef";
import type { InsertRevisions } from "../messages/InsertRevisions";
import type { MoveRevisions } from "../messages/MoveRevisions";
import type { AdoptRevision } from "../messages/AdoptRevision";
import type { ChangeId } from "../messages/ChangeId";
import type { CommitId } from "../messages/CommitId";
import RevisionMutator from "./RevisionMutator";
import ChangeMutator from "./ChangeMutator";
import RefMutator from "./RefMutator";
import type { StoreRef } from "../messages/StoreRef";
import type { CopyHunk } from "../messages/CopyHunk";
import type { CopyChanges } from "../messages/CopyChanges";

export type RichHint = (string | ChangeId | CommitId | Extract<StoreRef, { type: "LocalBookmark" } | { type: "RemoteBookmark" }>)[];
export type Eligibility = { type: "yes", hint: RichHint } | { type: "maybe", hint: string } | { type: "no" };

export default class BinaryMutator {
    #from: Operand;
    #to: Operand;

    constructor(from: Operand, to: Operand) {
        this.#from = from;
        this.#to = to;
    }

    static canDrag(from: Operand): Eligibility {
        // can't change finalised commits
        if ((from.type == "Revision" || from.type == "Change") && from.header.is_immutable) {
            return { type: "maybe", hint: "(revision is immutable)" };
        }
        if (from.type == "Revisions" && from.headers.some((h) => h.is_immutable)) {
            return { type: "maybe", hint: from.headers.length == 1 ? "(revision is immutable)" : "(revisions are immutable)" };
        }

        // removing a parent changes the child
        if (from.type == "Parent" && from.child.is_immutable) {
            return { type: "maybe", hint: "(child is immutable)" };
        } else if (from.type == "Parent" && from.child.parent_ids.length == 1) {
            return { type: "maybe", hint: "(child has only one parent)" };
        }

        // can change these listed things (XXX add modes?)
        if (from.type == "Revision") {
            return { type: "yes", hint: ["Rebasing revision ", from.header.id.change] };
        } else if (from.type == "Revisions") {
            return { type: "yes", hint: from.headers.length == 1 ? ["Rebasing revision ", from.headers[0].id.change] : [`Rebasing ${from.headers.length} revisions`] };
        } else if (from.type == "Parent") {
            return { type: "yes", hint: ["Removing parent from revision ", from.child.id.change] };
        } else if (from.type == "Change") {
            if (from.hunk) {
                return { type: "yes", hint: [`Squashing hunk ${from.hunk.location.from_file.start}:${from.hunk.location.from_file.start + from.hunk.location.from_file.len}@${from.path.relative_path} from revision `, from.header.id.change] };
            } else {
                return { type: "yes", hint: [`Squashing file ${from.path.relative_path} from revision `, from.header.id.change] };
            }
        } else if (from.type == "Ref" && from.ref.type != "Tag") {
            return { type: "yes", hint: ["Moving bookmark ", from.ref] };
        }

        return { type: "no" };
    }

    canDrop(): Eligibility {
        // generic prohibitions - don't drop undroppables, don't drop on yourself
        if (BinaryMutator.canDrag(this.#from).type != "yes" && !((this.#from.type == "Revision" || this.#from.type == "Revisions") && this.#to.type == "Merge")) {
            return { type: "no" };
        } else if (this.#from == this.#to) {
            return { type: "no" };
        }

        if (this.#from.type == "Revision") {
            if (this.#to.type == "Revision") {
                return { type: "yes", hint: ["Rebasing revision ", this.#from.header.id.change, " onto ", this.#to.header.id.change] };
            } else if (this.#to.type == "Parent") {
                if (this.#to.child == this.#from.header) {
                    return { type: "no" };
                } else if (this.#to.child.is_immutable) {
                    return { type: "maybe", hint: "(can't insert before an immutable revision)" };
                } else {
                    return { type: "yes", hint: ["Inserting revision ", this.#from.header.id.change, " before ", this.#to.child.id.change] };
                }
            } else if (this.#to.type == "Merge") {
                if (this.#to.header.id.change.hex == this.#from.header.id.change.hex) {
                    return { type: "no" };
                } else {
                    return { type: "yes", hint: ["Adding parent to revision ", this.#to.header.id.change] };
                }
            } else if (this.#to.type == "Repository") {
                return { type: "yes", hint: ["Abandoning commit ", this.#from.header.id.commit] };
            }
        }

        if (this.#from.type == "Revisions") {
            if (this.#to.type == "Revision") {
                let toHeader = this.#to.header;
                if (this.#from.headers.some(h => h.id.change.hex == toHeader.id.change.hex)) {
                    return { type: "no" }; // target within selected range
                } else {
                    return {
                        type: "yes", hint: this.#from.headers.length == 1
                            ? ["Rebasing revision ", this.#from.headers[0].id.change, " onto ", this.#to.header.id.change]
                            : [`Rebasing ${this.#from.headers.length} revisions onto `, this.#to.header.id.change]
                    };
                }
            } else if (this.#to.type == "Parent") {
                // check that neither before nor after are within the selected range
                let beforeHeader = this.#to.child;
                let afterHeader = this.#to.header;
                if (this.#from.headers.some(h => h.id.change.hex == beforeHeader.id.change.hex || h.id.change.hex == afterHeader.id.change.hex)) {
                    return { type: "no" }; // target within selected range
                } else {
                    return {
                        type: "yes", hint: this.#from.headers.length == 1
                            ? ["Inserting revision ", this.#from.headers[0].id.change, " before ", beforeHeader.id.change]
                            : [`Inserting ${this.#from.headers.length} revisions before `, beforeHeader.id.change]
                    };
                }
            } else if (this.#to.type == "Repository") {
                return {
                    type: "yes", hint: this.#from.headers.length == 1
                        ? ["Abandoning commit ", this.#from.headers[0].id.commit]
                        : [`Abandoning ${this.#from.headers.length} commits`]
                };
            } else if (this.#to.type == "Merge") {
                let toHeader = this.#to.header;
                if (this.#from.headers.some(h => h.id.change.hex == toHeader.id.change.hex)) {
                    return { type: "no" }; // target within selected range
                } else {
                    return {
                        type: "yes", hint: this.#from.headers.length == 1
                            ? ["Adding parent ", this.#from.headers[0].id.change, " to revision ", toHeader.id.change]
                            : [`Adding ${this.#from.headers.length} parents to revision `, toHeader.id.change]
                    };
                }
            }
        }

        if (this.#from.type == "Parent") {
            if (this.#to.type == "Repository") {
                return { type: "yes", hint: ["Removing parent from revision ", this.#from.child.id.change] };
            }
        }

        if (this.#from.type == "Change") {
            if (this.#to.type == "Revision") {
                if (this.#to.header.id.change.hex == this.#from.header.id.change.hex) {
                    return { type: "no" };
                } else if (this.#to.header.is_immutable) {
                    return { type: "maybe", hint: "(revision is immutable)" };
                } else {
                    return { type: "yes", hint: [`Squashing changes from ${this.#from.path.relative_path} into `, this.#to.header.id.change] };
                }
            } else if (this.#to.type == "Repository") {
                if (this.#from.header.parent_ids.length == 1) {
                    return { type: "yes", hint: [`Restoring changes at ${this.#from.path.relative_path} from parent `, this.#from.header.parent_ids[0]] };
                } else {
                    return { type: "maybe", hint: "Can't restore (revision has multiple parents)" };
                }
            }
        }

        if (this.#from.type == "Ref" && this.#from.ref.type != "Tag") {
            // local -> rev: set
            if (this.#to.type == "Revision" && this.#from.ref.type == "LocalBookmark") {
                if (this.#to.header.id.change.hex == this.#from.header.id.change.hex) {
                    return { type: "no" };
                } else {
                    return { type: "yes", hint: ["Moving bookmark ", this.#from.ref, " to ", this.#to.header.id.change] };
                }
            }

            // remote -> local: track
            else if (this.#to.type == "Ref" && this.#to.ref.type == "LocalBookmark" &&
                this.#from.ref.type == "RemoteBookmark" && this.#from.ref.branch_name == this.#to.ref.branch_name) {
                if (this.#from.ref.is_tracked) {
                    return { type: "maybe", hint: "(already tracked)" };
                } else {
                    return { type: "yes", hint: ["Tracking remote bookmark ", this.#from.ref] };
                }
            }

            // anything -> anywhere: delete
            else if (this.#to.type == "Repository") {
                if (this.#from.ref.type == "LocalBookmark") {
                    return { type: "yes", hint: ["Deleting bookmark ", this.#from.ref] };
                } else {
                    return {
                        type: "yes", hint: ["Forgetting remote bookmark ", this.#from.ref]
                    };
                }
            }
        }

        return { type: "no" };
    }

    doDrop() {
        if (this.#from.type == "Revision") {
            if (this.#to.type == "Revision") {
                // rebase rev onto single target
                mutate<MoveRevisions>("move_revisions", {
                    set: { from: this.#from.header.id, to: this.#from.header.id },
                    parent_ids: [this.#to.header.id]
                });
                return;
            } else if (this.#to.type == "Parent") {
                // insert between targets
                mutate<InsertRevisions>("insert_revisions", {
                    set: { from: this.#from.header.id, to: this.#from.header.id },
                    after_id: this.#to.header.id,
                    before_id: this.#to.child.id
                });
                return;
            } else if (this.#to.type == "Merge") {
                // rebase subtree onto additional targets
                let newParents = [...this.#to.header.parent_ids, this.#from.header.id.commit];
                mutate<AdoptRevision>("adopt_revision", { id: this.#to.header.id, parent_ids: newParents });
                return;
            } else if (this.#to.type == "Repository") {
                // abandon source
                new RevisionMutator([this.#from.header]).onAbandon();
                return;
            }
        }

        if (this.#from.type == "Revisions") {
            // headers are ordered newest-first, so [0] is newest and [length-1] is oldest
            const newest = this.#from.headers[0];
            const oldest = this.#from.headers[this.#from.headers.length - 1];

            if (this.#to.type == "Revision") {
                // rebase revset onto single target
                mutate<MoveRevisions>("move_revisions", {
                    set: { from: oldest.id, to: newest.id },
                    parent_ids: [this.#to.header.id]
                });
                return;
            } else if (this.#to.type == "Parent") {
                // insert range between targets
                mutate<InsertRevisions>("insert_revisions", {
                    set: { from: oldest.id, to: newest.id },
                    after_id: this.#to.header.id,
                    before_id: this.#to.child.id
                });
                return;
            } else if (this.#to.type == "Repository") {
                new RevisionMutator(this.#from.headers).onAbandon();
                return;
            } else if (this.#to.type == "Merge") {
                // add all selected revisions as additional parents
                let newParents = [
                    ...this.#to.header.parent_ids,
                    ...this.#from.headers.map(h => h.id.commit)
                ];
                mutate<AdoptRevision>("adopt_revision", { id: this.#to.header.id, parent_ids: newParents });
                return;
            }
            return;
        }

        if (this.#from.type == "Parent") {
            if (this.#to.type == "Repository") {
                // rebase subtree onto fewer targets 
                let removeCommit = this.#from.header.id.commit;
                let newParents = this.#from.child.parent_ids.filter(id => id.hex != removeCommit.hex);
                mutate<AdoptRevision>("adopt_revision", { id: this.#from.child.id, parent_ids: newParents });
                return;
            }
        }

        if (this.#from.type == "Change") {
            if (this.#to.type == "Revision") {
                // squash path or subpath to target
                if (this.#from.hunk) {
                    mutate<MoveHunk>("move_hunk", {
                        from_id: this.#from.header.id,
                        to_id: this.#to.header.id.commit,
                        path: this.#from.path,
                        hunk: this.#from.hunk
                    });
                } else {
                    mutate<MoveChanges>("move_changes", { from: { from: this.#from.header.id, to: this.#from.header.id }, to_id: this.#to.header.id.commit, paths: [this.#from.path] });
                }
                return;
            } else if (this.#to.type == "Repository") {
                // restore path or subpath from source parent to source
                if (this.#from.hunk) {
                    mutate<CopyHunk>("copy_hunk", {
                        from_id: this.#from.header.parent_ids[0],
                        to_id: this.#from.header.id,
                        path: this.#from.path,
                        hunk: this.#from.hunk
                    });
                } else {
                    mutate<CopyChanges>("copy_changes", {
                        from_id: this.#from.header.parent_ids[0],
                        to_set: { from: this.#from.header.id, to: this.#from.header.id },
                        paths: [this.#from.path]
                    });
                }
                return;
            }
        }

        if (this.#from.type == "Ref") {
            if (this.#to.type == "Revision") {
                // point ref to revision
                mutate<MoveRef>("move_ref", { to_id: this.#to.header.id, ref: this.#from.ref });
                return;
            } else if (this.#to.type == "Ref" && this.#from.ref.type == "RemoteBookmark") {
                // track remote bookmark with existing local
                new RefMutator(this.#from.ref).onTrack();
            } else if (this.#to.type == "Repository") {
                // various kinds of total or partial deletion
                new RefMutator(this.#from.ref).onDelete();
            }
        }

        console.log("error: unknown validated mutation");
    }
}
