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

The application can only request typed operations for repository discovery, bare/`HEAD` checks, worktree status, history, messages, changed files, diffs, tree entries, repository file lists, and fixed-text grep. It contains no generic “run Git arguments” path.

- Git runs directly without a shell.
- Repository paths and pathspecs are separate process arguments placed after `--` where applicable.
- Optional Git locks and terminal prompts are disabled.
- Pagers, color, external diff drivers, textconv, and fsmonitor execution are disabled.
- Object IDs are accepted for reuse as revisions only after hexadecimal validation.
- Current file reads open each component relative to the discovered worktree directory, do not follow symbolic links, and stop after 8 MiB.
- Keymap files accept only documented action and key names; they cannot run commands.

ChronoGit never stages, restores, commits, resets, checks out, creates branches, or updates references.

## Explicit LSP trust boundary

Language-server support does not weaken ChronoGit's Git guarantees, but the external server is a separate process with its own behavior. LSP is disabled unless `--lsp PROFILE` is supplied. Enable it only for a trusted repository: rust-analyzer may evaluate build scripts/procedural macros, and Java or Python servers may invoke project tooling, inspect environments, download dependencies through that tooling, or write caches and build artifacts.

ChronoGit does not bundle or download servers. It starts a validated trusted user-level argument array directly, never reads a server command from repository configuration, and performs no implicit shell interpolation. JDT workspace data and writable OSGi configuration use one unique temporary tree outside the repository for each process. Only complete UTF-8 current files are synchronized. Returned locations are opened only when a `file:` path remains inside the repository and passes the existing no-follow reader; external and virtual URIs are notices only.

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
| Live repository-search debounce | 100 ms |
| Diff cache | 16 entries and 16 MiB total |
| History page | 200 commits |
| File history | 200 commits |
| Current file content | 8 MiB |
| LSP message body | 8 MiB inbound and outbound |
| LSP response headers | 16 KiB |
| LSP writer queue | 64 messages per session |
| Resident LSP sessions | 4 profile/workspace pairs; least-recently-used eviction |
| Synchronized document | One complete file per session, at most 8 MiB |
| LSP navigation or hover request | 15 seconds |
| Normalized hover text | 262,144 characters |
| LSP initialize request | 30 seconds |
| LSP shutdown grace period | 2 seconds before child termination |
| Retained LSP stderr | 16 KiB tail |
| Semantic jump history | 64 older/newer locations in total |
| Key sequence | 750 ms |
| Minimum terminal | 80×24 |
| Changes multi-pane threshold | 110 columns (History always uses three rows) |

When a text diff reaches the stdout limit, the process is stopped and the available patch is labeled truncated. Partial machine-readable status, log, or tree data is rejected instead of being parsed. A timeout or stderr overflow becomes a recoverable error.

## Supported environment

Version `0.4.0` supports Linux and macOS, non-bare repositories, and interactive terminals. Unix path bytes are preserved internally; Windows is outside the current compatibility boundary.

## Non-goals in `0.4.0`

ChronoGit does not provide:

- staged-change inspection;
- repository mutation of any kind;
- remotes, pull requests, blame, or stash workflows;
- an editor or plugin runtime;
- combined or selectable-parent merge diffs;
- machine-readable export, batch mode, or a non-interactive UI;
- traversal into submodule repositories;
- opening dependency, standard-library, archive, or virtual LSP documents outside the active repository.
