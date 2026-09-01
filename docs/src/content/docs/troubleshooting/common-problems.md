---
title: Common problems
description: Diagnose startup errors, missing changes, Git failures, truncation, and terminal cleanup.
tags:
  - troubleshooting
  - errors
sidebar:
  order: 1
---

## `an interactive TTY is required`

Run `chronogit` directly in a terminal. Do not pipe it, capture its output, redirect standard input/output, or start it as a detached background task. Both stdin and stdout must be interactive.

`chronogit --help` and `chronogit --version` are the exceptions and work without a TTY.

## Repository discovery fails

Check the argument before retrying:

- The path must exist and be a directory.
- The path must be a repository root or a directory below one.
- Git must be installed and available on `PATH`.
- Bare repositories are not supported.

Startup diagnostics include a cause chain when the operating system or Git provides one. Fix the earliest concrete cause, such as a missing executable or permission failure.

## The Changes list is empty or misses a file

Changes intentionally shows the index-to-working-tree side only.

- A staged-only file is hidden.
- A file with staged and unstaged edits shows only its unstaged portion.
- A clean repository reports `No unstaged changes. Staged-only files are hidden.`

Use `git diff --cached` or another Git interface to inspect staged content.

## A pane shows a Git error

The repository may have changed after a selection, a file may have disappeared, permissions may prevent reading it, or Git metadata may be corrupt. Correct the external condition, then press `r` to retry the current view.

Runtime errors are recoverable where possible. ChronoGit keeps other panes usable and does not run a fallback mutation.

## A diff is truncated or a command times out

A text diff stops at 8 MiB and a Git command stops after 30 seconds. These are intentional memory and shutdown bounds. Inspect a smaller file or narrower revision with Git when the complete result does not fit.

Machine-readable results such as history or tree entries are rejected when incomplete. They are not shown as though they were complete.

## A binary file has no text patch

Binary changes appear as a summary. ChronoGit does not render binary contents or invoke external conversion programs.

## The terminal is too small

Resize to at least 80 columns by 24 rows. Below that size, ChronoGit replaces the interface with a stable size warning. `Q` and `Ctrl-C` still quit safely.

Between 80 and 109 columns, Changes uses one full-width pane at a time; use `h` and `l` to move between its file list and diff. History keeps all three full-width rows visible.

## A tree file says it has no change

The tree shows every entry in the selected commit, not only changed files. Selecting an unchanged file therefore reports that it has no change in the active root/parent comparison. Press `t` to return to the changed-file list.

## The terminal was not restored

ChronoGit restores terminal state on ordinary exit, `Q`, `Ctrl-C`, errors, and panics. A force kill (`SIGKILL`), terminal emulator failure, or system interruption cannot run cleanup.

Try the terminal's reset command:

```sh title="Terminal"
reset
```

If input still does not echo on a Unix shell, run `stty sane`. Reproduce the problem without a force kill and report the OS, terminal application, exit method, and whether `Q` or `Ctrl-C` was used.
