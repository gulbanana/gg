The implementation of the new “absorb” feature can be broken down into two main phases:

1. **Splitting the Source Changes into Hunks Mapped to Their Original Commits**
   The function `split_hunks_to_trees` (in [src-tauri/jj/lib/src/absorb.rs](src-tauri/jj/lib/src/absorb.rs)) is the entry point for absorbing hunks. Its steps are roughly:

   - **Diff Generation:**
     It first computes the diff between the parent tree (`left_tree`) and the source commit’s tree (`right_tree`). This is done using a diff stream with support for copy tracking:
     ```rust:src-tauri/jj/lib/src/absorb.rs
     102|     let left_tree = &source.parent_tree;
     103|     let right_tree = source.commit.tree()?;
     106|     let tree_diff = left_tree.diff_stream_with_copies(&right_tree, matcher, &copy_records);
     ```

   - **Processing Each Diff Entry:**
     The diff stream is iterated asynchronously. For each file change (each “entry”), the code reads the file contents from both the base (left) and the source (right). It uses helper functions (like `to_file_value`) to properly read the file contents and record metadata (e.g. executability). Error cases—for example, new or deleted files—are handled by skipping or explicitly marking the path as “skipped.”

   - **Determining Hunk Boundaries via File Annotations:**
     The function `split_file_hunks` takes two inputs:
     - A set of annotation ranges (obtained by blaming or annotating the file to know which lines originally came from which commit).
     - The file diff itself (computed by comparing the two file versions).

     These are compared to determine which parts (or “hunks”) of the change should be attributed to which commit. In practice, it walks through the diff hunks (filtering for differences) and finds intersections with the annotation ranges. The resulting mapping is from source commit IDs to vectors of line-range pairs that indicate which parts of the file should be “absorbed” into that commit. The tests in the module cover many scenarios (contiguous hunks, deletions, modifications, ambiguous changes, etc.):
     ```rust:src-tauri/jj/lib/src/absorb.rs
     181| fn split_file_hunks<'a>(
     182|     mut annotation_ranges: &[(&'a CommitId, Range<usize>)],
         // ...
     188|         let mut diff_hunk_ranges = diff.hunk_ranges().filter(|hunk| hunk.kind == DiffHunkKind::Different);
         // ...
     252|    }
     ```

   - **Combining Hunks into New File Contents:**
     For each file and for each hunk, the function `combine_texts` is used to merge the unchanged parts with the hunks that should be taken from the right (source commit) text. The new file data is then written back into a temporary tree builder (a `MergedTreeBuilder`). This is how the code “extracts” the hunks that belong to a given destination commit while leaving out the others.

   - **Storing the Results in `SelectedTrees`:**
     The final result of this phase is a `SelectedTrees` structure that contains:
     - A mapping (`target_commits`) from commit IDs to their corresponding `MergedTreeBuilder`s (i.e. the new trees representing modifications for each commit), and
     - A list of file paths that couldn’t be processed (with error messages).

     This lets later stages know exactly which hunks should be absorbed into which commit.

2. **Rewriting Commits to Apply the Hunk Absorption**
   The second phase is performed by the function `absorb_hunks`, which takes the repository (as a mutable reference), the absorb source, and the set of selected trees produced in phase one. Its implementation works like this:

   - **Rewriting the Source Commit:**
     The function first “reparents” or rewrites the source commit by removing the hunks that were moved. If, after removal, the commit becomes discardable (i.e. it no longer contains any changes), it is abandoned.
     ```rust:src-tauri/jj/lib/src/absorb.rs
     301|         if rewriter.old_commit().id() == source.commit.id() {
     302|             let commit_builder = rewriter.reparent();
     303|             if commit_builder.is_discardable()? {
     304|                 commit_builder.abandon();
     305|             } else {
     306|                 rewritten_source = Some(commit_builder.write()?);
     ```

   - **Rebasing Destination Commits:**
     For each of the destination commits (i.e. those that should receive hunks), the function performs a rebase:
     - It writes the new tree obtained from the corresponding tree builder,
     - Retrieves the destination commit’s current tree,
     - Merges the destination tree with the modifications extracted from the source (based on the diff computed earlier),
     - And then finally rewrites the commit so that it now contains the absorbed changes. Additionally, the original source commit ID is added as an extra predecessor, preserving history and connectivity.
     ```rust:src-tauri/jj/lib/src/absorb.rs
     317|         let selected_tree_id = tree_builder.write_tree(&store)?;
     318|         let commit_builder = rewriter.rebase()?;
     319|         let destination_tree = store.get_root_tree(commit_builder.tree_id())?;
     320|         let selected_tree = store.get_root_tree(&selected_tree_id)?;
     321|         let new_tree = destination_tree.merge(&source.parent_tree, &selected_tree)?;
     322|         let mut predecessors = commit_builder.predecessors().to_vec();
     323|         predecessors.push(source.commit.id().clone());
     ```

   - **Returning a Summary of the Operation:**
     After processing all relevant commits, an `AbsorbStats` struct is returned. This struct includes:
     - The (possibly rewritten) source commit (or `None` if it was fully abandoned),
     - The list of destination commits that were rewritten,
     - And the count of descendant commits that were rebased.

---

### Reusing Functionality for a UI Drag & Drop Feature

If you want to build a UI where a user can drag and drop hunks between commits, you can reuse much of the above functionality:

- **Diff and Hunk Splitting:**
  Use the same logic as in `split_hunks_to_trees` and `split_file_hunks` to extract the hunks from a commit’s diff based on file annotations. This will let you display the hunks (with their line ranges and file paths) in the UI.

- **Hunk Combination:**
  Once the user drags a hunk to a different commit, you can leverage `combine_texts` (or its underlying logic) to compute the new file content for both the source and the destination.

- **Tree Rebuilding and Commit Rewriting:**
  The workflow in `absorb_hunks` shows how to create new “merged trees” and update commits by:
  - Reparenting the source commit (to remove the moved hunks), and
  - Rebasing the destination commit (to include the moved hunks).
  You can either call this function directly (or extract its core logic) when the user confirms a drag‐and‐drop action.

- **Error Handling and Ambiguity:**
  Notice that if a hunk cannot be unambiguously assigned (for example, if it overlaps a “masked” area), it is skipped (collected in `skipped_paths`). Your UI could surface these cases to let the user resolve ambiguities manually.

---

### In Summary

- **Diff Generation and Hunk Extraction:**
  The process starts by comparing a commit’s tree to its parent, then walking through the diff stream and using line annotations to partition the diff into hunks (via `split_hunks_to_trees` and `split_file_hunks`).

- **Tree Building and File Content Reconstruction:**
  For each affected file, the relevant hunks are combined with the unchanged portions (using `combine_texts`) and written to a temporary tree structure (through `MergedTreeBuilder`), effectively “preparing” the hunks for absorption.

- **Commit Rewriting:**
  Finally, the source commit (or its descendant commits) is rewritten. The source commit is “reparented” (losing its absorbed hunks), and the destination commits are “rebased” with the new merged tree that integrates the hunks. The function `absorb_hunks` orchestrates this process and returns an `AbsorbStats` summary.

For your UI feature, you can leverage these same steps by exposing the hunk-splitting part to the UI (thus allowing drag-and-drop operations on individual hunks) and then reusing the tree-merging and commit-rewriting logic to persist the changes. This modularity lets you offer an interactive drag-and-drop experience while reusing the robust backend processing already implemented in the absorb feature.
