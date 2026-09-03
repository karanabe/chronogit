# Developer Guide

ChronoGit is a single Rust binary for exploring Git changes, history, and
working-tree source in a terminal. This guide explains the implementation layout and design
boundaries for people changing the code. For contribution workflow, commit
guidelines, and required checks, see [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Current Implementation Status

The current implementation covers the `0.3.0` scope described in
[`README.md`](README.md):

- unstaged tracked and untracked worktree changes
- paged commit history with root and first-parent merge comparisons
- parent-lane Git graph with changed-file/diff details
- repository file/content search with per-file history and current content
- expandable working-tree Code viewer with bounded full-file content
- full commit messages, changed-file lists, and lazy commit trees
- optional validated XDG or explicit keymap configuration
- bounded, asynchronous Git reads through a typed command allowlist
- Vim-oriented navigation and responsive terminal layouts
- terminal restoration on normal exit, errors, Ctrl-C, and panics

## Module Map

| Module | Role | Notes |
| --- | --- | --- |
| [`src/domain.rs`](src/domain.rs) + [`src/domain/`](src/domain) | Domain model | Owns validated repository paths, object IDs, changes, commits, diffs, search hits, file documents, and tree entries without Git or terminal I/O. |
| [`src/git.rs`](src/git.rs) + [`src/git/`](src/git) | Repository adapter | Owns the read-only Git command allowlist, bounded process/current-file reads, machine-output parsing, and domain-level repository operations. |
| [`src/app.rs`](src/app.rs) + [`src/app/`](src/app) | Application state | Owns actions, events, effects, asynchronous load state, Git and Code workflow selection, projected code-tree state, caching, and stale-response rejection. |
| [`src/tui.rs`](src/tui.rs) + [`src/tui/`](src/tui) | Terminal presentation | Owns configurable key mapping, graph lanes, bounded syntax highlighting, terminal lifecycle, layout, rendering, and the interactive event loop. |
| [`src/cli.rs`](src/cli.rs) | CLI boundary | Owns command-line parsing, repository discovery input, and startup validation. |
| [`src/error.rs`](src/error.rs) | Top-level errors | Owns contextual application errors and source chaining. |

The detailed architecture, comparison contracts, resource limits, and security
invariants are documented in
[`docs/src/content/docs/developer/architecture.md`](docs/src/content/docs/developer/architecture.md).

Layered modules use Rust's `module.rs` plus `module/child.rs` layout. Module
roots hold responsibility documentation, declarations, and deliberate
re-exports; child files own individual concepts. Keep this non-`mod.rs` layout
when adding or splitting modules.

## Runtime Shape

The terminal event loop translates input into actions. Updating application
state may produce a typed Git effect, which is executed asynchronously and
returned as an event before the next render:

```text
crossterm event -> key map -> AppState update -> GitEffect + RequestId
                                            |             |
                                            |             `-> bounded executor
                                            |                    `-> GitService
                                            |                         `-> GitRunner
                                            `-> ratatui render              |
                                                      ^                     |
                                                      `------ Event <-------`
```

`src/domain` remains independent of process and terminal I/O. `src/git` is the
only layer allowed to invoke Git, and `src/tui` is the only layer that manages
terminal state. Keep those boundaries explicit so reducers and parsers remain
testable without a real terminal.

## Design Boundaries

- Preserve the read-only contract. Add Git operations through `GitCommand` and
  never bypass its closed allowlist.
- Pass repository paths and pathspecs as separate process arguments; never
  interpolate them into shell text.
- Keep Git paths as bytes on Unix until presentation requires lossy rendering.
- Open current files relative to the discovered worktree descriptor and reject
  symbolic links in every path component.
- Represent exclusive UI states and load outcomes with enums instead of
  combinations of flags.
- Attach request IDs to asynchronous work and ignore completions that no longer
  match the selected resource.
- Keep process output, task concurrency, caches, history pages, debounce time,
  and command duration bounded.
- Restore terminal state on every exit path before reporting an application
  failure.

Read the relevant implementation and tests before changing domain invariants,
Git commands or parsers, asynchronous state transitions, terminal lifecycle,
or platform support.
