---
title: Safety, limits, and non-goals
description: Understand ChronoGit's read-only guarantees, resource bounds, and unsupported operations.
tags:
  - safety
  - limits
  - security
sidebar:
  order: 2
---

ChronoGit treats repository contents and configuration as untrusted input. Its Git boundary is closed, shell-free, and bounded.

## Read-only contract

The application can only request typed operations for repository discovery, bare/`HEAD` checks, worktree status, history, messages, changed files, diffs, and tree entries. It contains no generic “run Git arguments” path.

- Git runs directly without a shell.
- Repository paths and pathspecs are separate process arguments placed after `--` where applicable.
- Optional Git locks and terminal prompts are disabled.
- Pagers, color, external diff drivers, textconv, and fsmonitor execution are disabled.
- Object IDs are accepted for reuse as revisions only after hexadecimal validation.

ChronoGit never stages, restores, commits, resets, checks out, creates branches, or updates references.

:::note[Concurrent external changes]
Read-only means ChronoGit does not mutate the repository. Editors, hooks started elsewhere, and other Git processes can still change it while the TUI is open. Press `r` to refresh after such a change.
:::

## Resource bounds

| Resource | Limit or behavior |
| --- | --- |
| Git stdout | 8 MiB per command |
| Git stderr | 64 KiB per command |
| Git command duration | 30 seconds |
| Concurrent Git reads | At most 2 |
| Diff request debounce | 75 ms |
| Diff cache | 16 entries and 16 MiB total |
| History page | 200 commits |
| `zh` / `zl` sequence | 750 ms |
| Minimum terminal | 80×24 |
| Changes multi-pane threshold | 110 columns (History always uses three rows) |

When a text diff reaches the stdout limit, the process is stopped and the available patch is labeled truncated. Partial machine-readable status, log, or tree data is rejected instead of being parsed. A timeout or stderr overflow becomes a recoverable error.

## Supported environment

Version `0.1.0` supports Linux and macOS, non-bare repositories, and interactive terminals. Unix path bytes are preserved internally; Windows is outside the current compatibility boundary.

## Non-goals in `0.1.0`

ChronoGit does not provide:

- staged-change inspection;
- repository mutation of any kind;
- remotes, pull requests, blame, or stash workflows;
- an editor, configuration file, or plugin runtime;
- combined or selectable-parent merge diffs;
- machine-readable export, batch mode, or a non-interactive UI;
- traversal into submodule repositories.
