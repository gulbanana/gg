import { mutate } from "../ipc";
import { sameChange } from "../ids";
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
import RefMutator from "./RefMutator";
import type { StoreRef } from "../messages/StoreRef";
import type { CopyHunk } from "../messages/CopyHunk";
import type { CopyChanges } from "../messages/CopyChanges";
export type RichHint = (string | ChangeId | CommitId | Extract<StoreRef, { type: "LocalBookmark" } | { type: "RemoteBookmark" }>)[];
export type Eligibility = { type: "yes", hint: RichHint } | { type: "maybe", hint: string } | { type: "no" };

export default class BinaryMutator {
    #from: Operand;
    #to: Operand | null;
    #ignoreImmutable: boolean;

    constructor(from: Operand, to: Operand | null, ignoreImmutable: boolean) {
        this.#from = from;
        this.#to = to;
        this.#ignoreImmutable = ignoreImmutable;
    }

    canDrag(): Eligibility {
        // can't change finalised commits without a gesture
        if (this.#from.type == "Revision" && this.#from.header.is_immutable) {
            if (this.#ignoreImmutable) {
                return { type: "yes", hint: ["Rebasing immutable revision ", this.#from.header.id.change] };
            } else {
                return { type: "maybe", hint: "(immutable - toggle 🛡 to override)" };
            }
        }
        if ((this.#from.type == "Revisions" || this.#from.type == "Change") && this.#from.headers.some((h) => h.is_immutable)) {
            if (this.#ignoreImmutable) {
                return this.#from.type == "Change"
                    ? { type: "yes", hint: [`Squashing from immutable revision `, this.#from.headers[0].id.change] }
                    : {
                        type: "yes", hint: this.#from.headers.length == 1
                            ? ["Rebasing immutable revision ", this.#from.headers[0].id.change]
                            : ["Rebasing immutable revisions ", this.#from.headers[this.#from.headers.length - 1].id.change, "::", this.#from.headers[0].id.change]
                    };
            }
            else {
                return { type: "maybe", hint: "(immutable - toggle 🛡 to override)" };
            }
        }

        // removing a parent changes the child
        if (this.#from.type == "Parent" && this.#from.child.is_immutable) {
            if (this.#ignoreImmutable) {
                return { type: "yes", hint: ["Removing parent from immutable revision ", this.#from.child.id.change] };
            } else {
                return { type: "maybe", hint: "(immutable - toggle 🛡 to override)" };
            }
        } else if (this.#from.type == "Parent" && this.#from.child.parent_ids.length == 1) {
            return { type: "maybe", hint: "(child has only one parent)" };
        }

        // can change these listed things
        if (this.#from.type == "Revision") {
            return { type: "yes", hint: ["Rebasing revision ", this.#from.header.id.change] };
        } else if (this.#from.type == "Revisions") {
            return {
                type: "yes", hint: this.#from.headers.length == 1 ? ["Rebasing revision ", this.#from.headers[0].id.change] :
                    ["Rebasing revisions ", this.#from.headers[this.#from.headers.length - 1].id.change, "::", this.#from.headers[0].id.change]
            };
        } else if (this.#from.type == "Parent") {
            return { type: "yes", hint: ["Removing parent from revision ", this.#from.child.id.change] };
        } else if (this.#from.type == "Change") {
            if (this.#from.hunk) {
                return {
                    type: "yes", hint: this.#from.headers.length == 1 ?
                        [`Squashing hunk ${this.#from.hunk.location.from_file.start}:${this.#from.hunk.location.from_file.start + this.#from.hunk.location.from_file.len}@${this.#from.path.relative_path} from revision `, this.#from.headers[0].id.change] :
                        [`Squashing hunk ${this.#from.hunk.location.from_file.start}:${this.#from.hunk.location.from_file.start + this.#from.hunk.location.from_file.len}@${this.#from.path.relative_path} from revisions `, this.#from.headers[this.#from.headers.length - 1].id.change, "::", this.#from.headers[0].id.change]
                };
            } else {
                return {
                    type: "yes", hint: this.#from.headers.length == 1 ?
                        [`Squashing file ${this.#from.path.relative_path} from revision `, this.#from.headers[0].id.change] :
                        [`Squashing file ${this.#from.path.relative_path} from revisions `, this.#from.headers[this.#from.headers.length - 1].id.change, "::", this.#from.headers[0].id.change]
                };
            }
        } else if (this.#from.type == "Ref" && this.#from.ref.type != "Tag") {
            return { type: "yes", hint: ["Moving bookmark ", this.#from.ref] };
        }

        return { type: "no" };
    }

    canDrop(): Eligibility {
        // generic prohibitions - don't drop undroppables, don't drop on yourself
        if (this.#to == null) {
            return { type: "no" };
        } else if (this.canDrag().type != "yes" && !((this.#from.type == "Revision" || this.#from.type == "Revisions") && this.#to.type == "Merge")) {
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
                } else if (this.#to.child.is_immutable && !this.#ignoreImmutable) {
                    return { type: "maybe", hint: "(immutable - toggle 🛡 to override)" };
                } else {
                    return { type: "yes", hint: ["Inserting revision ", this.#from.header.id.change, " before ", this.#to.child.id.change] };
                }
            } else if (this.#to.type == "Merge") {
                if (sameChange(this.#to.header.id.change, this.#from.header.id.change)) {
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
                if (this.#from.headers.some(h => sameChange(h.id.change, toHeader.id.change))) {
                    return { type: "no" }; // target within selected range
                } else {
                    return {
                        type: "yes", hint: this.#from.headers.length == 1
                            ? ["Rebasing revision ", this.#from.headers[0].id.change, " onto ", this.#to.header.id.change]
                            : ["Rebasing revisions ", this.#from.headers[this.#from.headers.length - 1].id.change, "::", this.#from.headers[0].id.change, " onto ", this.#to.header.id.change]
                    };
                }
            } else if (this.#to.type == "Parent") {
                // check that neither before nor after are within the selected range
                let beforeHeader = this.#to.child;
                let afterHeader = this.#to.header;
                if (this.#from.headers.some(h => sameChange(h.id.change, beforeHeader.id.change) || sameChange(h.id.change, afterHeader.id.change))) {
                    return { type: "no" }; // target within selected range
                } else {
                    return {
                        type: "yes", hint: this.#from.headers.length == 1
                            ? ["Inserting revision ", this.#from.headers[0].id.change, " before ", beforeHeader.id.change]
                            : ["Inserting revisions ", this.#from.headers[this.#from.headers.length - 1].id.change, "::", this.#from.headers[0].id.change, " before ", beforeHeader.id.change]
                    };
                }
            } else if (this.#to.type == "Repository") {
                return {
                    type: "yes", hint: this.#from.headers.length == 1
                        ? ["Abandoning commit ", this.#from.headers[0].id.commit]
                        : ["Abandoning commits ", this.#from.headers[this.#from.headers.length - 1].id.commit, "::", this.#from.headers[0].id.commit]
                };
            } else if (this.#to.type == "Merge") {
                let toHeader = this.#to.header;
                if (this.#from.headers.some(h => sameChange(h.id.change, toHeader.id.change))) {
                    return { type: "no" }; // target within selected range
                } else {
                    return {
                        type: "yes", hint: this.#from.headers.length == 1
                            ? ["Adding parent ", this.#from.headers[0].id.change, " to revision ", toHeader.id.change]
                            : ["Adding parents ", this.#from.headers[this.#from.headers.length - 1].id.change, "::", this.#from.headers[0].id.change, " to revision ", toHeader.id.change]
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
                let toHeader = this.#to.header;
                if (this.#from.headers.some((header) => sameChange(header.id.change, toHeader.id.change))) {
                    return { type: "no" };
                } else if (toHeader.is_immutable && !this.#ignoreImmutable) {
                    return { type: "maybe", hint: "(immutable - toggle 🛡 to override)" };
                } else {
                    return { type: "yes", hint: [`Squashing changes from ${this.#from.path.relative_path} into `, toHeader.id.change] };
                }
            } else if (this.#to.type == "Repository") {
                let fromOldest = this.#from.headers[this.#from.headers.length - 1];
                if (fromOldest.parent_ids.length == 1) {
                    return { type: "yes", hint: [`Restoring changes at ${this.#from.path.relative_path} from parent `, fromOldest.parent_ids[0]] };
                } else {
                    return { type: "maybe", hint: "(revision has multiple parents)" };
                }
            }
        }

        if (this.#from.type == "Ref" && this.#from.ref.type != "Tag") {
            // local -> rev: set
            if (this.#to.type == "Revision" && this.#from.ref.type == "LocalBookmark") {
                if (sameChange(this.#to.header.id.change, this.#from.header.id.change)) {
                    return { type: "no" };
                } else {
                    return { type: "yes", hint: ["Moving bookmark ", this.#from.ref, " to ", this.#to.header.id.change] };
                }
            }

            // remote -> local: track
            else if (this.#to.type == "Ref" && this.#to.ref.type == "LocalBookmark" &&
                this.#from.ref.type == "RemoteBookmark" && this.#from.ref.bookmark_name == this.#to.ref.bookmark_name) {
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
        if (this.#to == null) {
            console.warn("attempted drop without a target");
            return;
        }
        let options = { ignoreImmutable: this.#ignoreImmutable };

        if (this.#from.type == "Revision") {
            if (this.#to.type == "Revision") {
                // rebase rev onto single target
                mutate<MoveRevisions>("move_revisions", {
                    set: { from: this.#from.header.id, to: this.#from.header.id },
                    parent_ids: [this.#to.header.id]
                }, options);
                return;
            } else if (this.#to.type == "Parent") {
                // insert between targets
                mutate<InsertRevisions>("insert_revisions", {
                    set: { from: this.#from.header.id, to: this.#from.header.id },
                    after_id: this.#to.header.id,
                    before_id: this.#to.child.id
                }, options);
                return;
            } else if (this.#to.type == "Merge") {
                // rebase subtree onto additional targets
                let newParents = [...this.#to.header.parent_ids, this.#from.header.id.commit];
                mutate<AdoptRevision>("adopt_revision", { id: this.#to.header.id, parent_ids: newParents }, options);
                return;
            } else if (this.#to.type == "Repository") {
                // abandon source
                new RevisionMutator([this.#from.header], this.#ignoreImmutable).onAbandon();
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
                }, options);
                return;
            } else if (this.#to.type == "Parent") {
                // insert range between targets
                mutate<InsertRevisions>("insert_revisions", {
                    set: { from: oldest.id, to: newest.id },
                    after_id: this.#to.header.id,
                    before_id: this.#to.child.id
                }, options);
                return;
            } else if (this.#to.type == "Repository") {
                new RevisionMutator(this.#from.headers, this.#ignoreImmutable).onAbandon();
                return;
            } else if (this.#to.type == "Merge") {
                // add all selected revisions as additional parents
                let newParents = [
                    ...this.#to.header.parent_ids,
                    ...this.#from.headers.map(h => h.id.commit)
                ];
                mutate<AdoptRevision>("adopt_revision", { id: this.#to.header.id, parent_ids: newParents }, options);
                return;
            }
            return;
        }

        if (this.#from.type == "Parent") {
            if (this.#to.type == "Repository") {
                // rebase subtree onto fewer targets
                let removeCommit = this.#from.header.id.commit;
                let newParents = this.#from.child.parent_ids.filter(id => id.hex != removeCommit.hex);
                mutate<AdoptRevision>("adopt_revision", { id: this.#from.child.id, parent_ids: newParents }, options);
                return;
            }
        }

        if (this.#from.type == "Change") {
            let fromSingleton = this.#from.headers.length == 1 ? this.#from.headers[0] : null;
            let fromSet = {
                from: this.#from.headers[this.#from.headers.length - 1].id,
                to: this.#from.headers[0].id,
            };
            if (this.#to.type == "Revision") {
                // squash path or subpath to target
                if (this.#from.hunk) {
                    if (!fromSingleton) {
                        return;
                    }
                    mutate<MoveHunk>("move_hunk", {
                        from_id: fromSingleton.id,
                        to_id: this.#to.header.id.commit,
                        path: this.#from.path,
                        hunk: this.#from.hunk
                    }, options);
                } else {
                    mutate<MoveChanges>("move_changes", { from: fromSet, to_id: this.#to.header.id.commit, paths: [this.#from.path] }, options);
                }
                return;
            } else if (this.#to.type == "Repository") {
                // restore path or subpath from source parent to source
                if (this.#from.hunk) {
                    if (!fromSingleton) {
                        return;
                    }
                    mutate<CopyHunk>("copy_hunk", {
                        from_id: fromSingleton.parent_ids[0],
                        to_id: fromSingleton.id,
                        path: this.#from.path,
                        hunk: this.#from.hunk
                    }, options);
                } else {
                    let fromOldest = this.#from.headers[this.#from.headers.length - 1];
                    mutate<CopyChanges>("copy_changes", {
                        from_id: fromOldest.parent_ids[0],
                        to_set: fromSet,
                        paths: [this.#from.path]
                    }, options);
                }
                return;
            }
        }

        if (this.#from.type == "Ref") {
            if (this.#to.type == "Revision") {
                // point ref to revision
                mutate<MoveRef>("move_ref", { to_id: this.#to.header.id, ref: this.#from.ref }, options);
                return;
            } else if (this.#to.type == "Ref" && this.#from.ref.type == "RemoteBookmark") {
                // track remote bookmark with existing local
                new RefMutator(this.#from.ref, this.#ignoreImmutable).onTrack();
            } else if (this.#to.type == "Repository") {
                // various kinds of total or partial deletion
                new RefMutator(this.#from.ref, this.#ignoreImmutable).onDelete();
            }
        }

        console.log("error: unknown validated mutation");
    }
}
