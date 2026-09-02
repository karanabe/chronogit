---
title: Inspect unstaged changes
description: Read tracked and untracked worktree changes in the Changes view.
tags:
  - changes
  - diff
  - worktree
sidebar:
  order: 2
---

The Changes view answers one question: what differs between the Git index and the working tree right now?

Open it with `chronogit`, `chronogit --view changes`, or the `1` key.

## What appears

The left pane lists unstaged entries reported by Git, including:

| Worktree state | ChronoGit behavior |
| --- | --- |
| Modified, deleted, renamed, copied, or type-changed | Shows the unstaged part of the tracked file |
| Untracked | Compares `/dev/null` with the working-tree file |
| Conflicted | Shows the worktree-side conflict status and the available diff |
| Staged only | Hidden intentionally |
| Both staged and unstaged | Shows only the index-to-working-tree change |

ChronoGit does not show an index-to-`HEAD` staged diff. Use ordinary Git tooling when you need to review staged content.

## Read a file diff

1. Use `j` and `k` to select a file in **Unstaged changes**.
2. Press `l` to focus the diff pane when needed.
3. Use `j`, `k`, `Ctrl-d`, and `Ctrl-u` to scroll vertically.
4. Use `zh` and `zl` to scroll long lines horizontally.

Text patches include old and new line numbers and visually distinguish headers, hunks, additions, removals, context, and metadata. Recognized source paths are syntax-highlighted with the embedded `syntect` and `two-face` grammar set. Muted green and red backgrounds distinguish additions and removals while preserving token colors. The current line selected with `j` / `k` uses a narrow gutter marker instead of a row background, so it does not cover the code colors. Binary changes appear as a summary instead of raw bytes.

Syntax highlighting falls back to plain diff text for unrecognized files or unusually expensive inputs (more than 512 KiB, 10,000 lines, or 4 KiB on one line). Diff classification colors and navigation remain available in that fallback.

At widths below 110 columns, only the focused pane is visible. Use `h` and `l` to move between the file list and diff. See [Navigation and layout](/guides/navigation/) for every key.

## Refresh after an external edit

Press `r` to reread the current view after an editor or another process changes the repository. Refresh preserves the selected path when it still exists and clears cached diffs so displayed content cannot remain stale.

ChronoGit itself never stages, restores, or edits the selected file. The repository may still change while ChronoGit is open because another process is operating on it.

## Large and unusual files

- A text diff that reaches the 8 MiB stdout limit is stopped and marked as truncated.
- A binary diff is represented by a Git summary.
- Non-UTF-8 Unix paths are retained as bytes internally and rendered lossily only for display.
- If a selected untracked file disappears before its diff loads, the diff pane shows a recoverable Git error. Press `r` after correcting the external condition.
