# Developer Guide

ChronoGit is a single Rust binary for exploring worktree changes and commit
history in a terminal. This guide explains the implementation layout and design
boundaries for people changing the code. For contribution workflow, commit
guidelines, and required checks, see [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Current Implementation Status

The current implementation covers the `0.1.0` scope described in
[`README.md`](README.md):

- unstaged tracked and untracked worktree changes
- paged commit history with root and first-parent merge comparisons
- full commit messages, changed-file lists, and lazy commit trees
- bounded, asynchronous Git reads through a typed command allowlist
- Vim-oriented navigation and responsive terminal layouts
- terminal restoration on normal exit, errors, Ctrl-C, and panics

## Module Map

| Module | Role | Notes |
| --- | --- | --- |
| [`src/domain`](src/domain) | Domain model | Owns validated repository paths, object IDs, changes, commits, diffs, and tree entries without Git or terminal I/O. |
| [`src/git`](src/git) | Git adapter | Owns the read-only command allowlist, bounded process execution, machine-output parsing, and domain-level Git operations. |
| [`src/app`](src/app) | Application state | Owns actions, events, effects, asynchronous load state, selection, caching, and stale-response rejection. |
| [`src/tui`](src/tui) | Terminal presentation | Owns key mapping, terminal lifecycle, layout, rendering, and the interactive event loop. |
| [`src/cli.rs`](src/cli.rs) | CLI boundary | Owns command-line parsing, repository discovery input, and startup validation. |
| [`src/error.rs`](src/error.rs) | Top-level errors | Owns contextual application errors and source chaining. |

The detailed architecture, comparison contracts, resource limits, and security
invariants are documented in
[`docs/src/content/docs/developer/architecture.md`](docs/src/content/docs/developer/architecture.md).

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
