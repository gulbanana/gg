# Jujutsu (jj) Version Control Guide

This project uses Jujutsu (jj) for source control instead of Git. Jujutsu is a modern version control system with some key differences from Git.

## Basic Concepts

- **Working copy** is represented by `@` in revsets
- **Parent commits** are referenced by `@-`, `@--`, `@---`, etc.
- **Revsets** are expressions that select commits (similar to Git's revision syntax but more powerful)

## Essential Commands

### Viewing History

```powershell
# View recent commit history (use --no-pager to avoid GUI pager)
jj log --no-pager

# View specific range of commits
jj log -r "@-----::@" --no-pager

# Show commit details (--git is REQUIRED to avoid GUI viewer)
jj show -r "@-" --no-pager --git
```

**Critical**: `jj show` requires `--git` flag in addition to `--no-pager`, otherwise it will open a GUI viewer.

### Viewing Diffs

```powershell
# IMPORTANT: Always use --no-pager and --git flags to avoid GUI diff viewers
# Compare working copy with parent commit
jj diff --no-pager --git --from "@-" --to "@"

# View changes in a specific file
jj diff --no-pager --git --from "@-" --to "@" -- path/to/file.cs

# View all uncommitted changes
jj diff --no-pager --git
```

**Critical Note**: Without `--no-pager`, jj will try to open a GUI diff viewer which doesn't work in terminal contexts. Always include `--no-pager` when viewing diffs programmatically.

### Making Commits

```powershell
# Commit all changes with a message
jj commit -m "description of changes"

# The working copy automatically advances to a new empty commit
# This is different from Git - you don't need to create a new branch
```

### Working with Revsets

Revsets must be quoted in PowerShell to avoid parsing errors:

```powershell
# CORRECT: Use quotes around revsets with special characters
jj show -r "@-"
jj log -r "@-----::@"

# INCORRECT: Will cause PowerShell parsing errors
jj show -r @-
```

## Important Differences from Git

1. **No staging area**: Changes are automatically tracked, no `git add` equivalent needed
2. **Automatic working copy advancement**: After committing, you're immediately on a new empty commit
3. **Revset syntax**: More powerful than Git's revision syntax but requires proper quoting in PowerShell
4. **Default pager behavior**: jj defaults to GUI tools, always use `--no-pager` for terminal output

## Common Patterns

### Incremental Development with Commits

When implementing a feature in phases:

```powershell
# Make changes for phase 1
# ... edit files ...

# Commit phase 1
jj commit -m "phase 1: description"

# Make changes for phase 2
# ... edit files ...

# Commit phase 2
jj commit -m "phase 2: description"

# Continue with additional phases...
```

The working copy automatically advances after each commit, so you can continue working immediately.

### Viewing Recent Work

```powershell
# See last 5 commits
jj log -r "@----::@" --no-pager

# See changes in the last commit
jj diff --no-pager --git --from "@--" --to "@-"

# View details of the last commit
jj show -r "@-" --no-pager --git
```

## Troubleshooting

### Error: "Syntax error" in revset

**Problem**: PowerShell is interpreting special characters
```powershell
jj show -r @-  # ❌ Causes syntax error
```

**Solution**: Quote the revset
```powershell
jj show -r "@-"  # ✅ Works correctly
```

### Error: GUI diff viewer opens instead of terminal output

**Problem**: Missing `--no-pager` flag
```powershell
jj diff --from "@-" --to "@"  # ❌ Opens GUI
```

**Solution**: Add `--no-pager` flag
```powershell
jj diff --no-pager --git --from "@-" --to "@"  # ✅ Terminal output
```

## Resources

- [Jujutsu Documentation](https://jj-vcs.github.io/jj/)
- [Filesets Documentation](https://jj-vcs.github.io/jj/latest/filesets/)
- [Revsets Documentation](https://jj-vcs.github.io/jj/latest/revsets/)
