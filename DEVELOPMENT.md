# Developer Guide

ChronoGit is a Cargo workspace whose existing Rust library/binary explores Git
changes, history, and working-tree source in a terminal. The workspace also
owns the reusable `vim-navigation` library crate. This guide explains the implementation layout and design
boundaries for people changing the code. For contribution workflow, commit
guidelines, and required checks, see [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Current Implementation Status

The current implementation covers the `0.7.0` scope described in
[`README.md`](README.md):

- unstaged tracked and untracked worktree changes
- paged commit history with root and first-parent merge comparisons
- parent-lane Git graph with changed-file/diff details
- repository file/content search with per-file history and current content
- expandable working-tree Code viewer with bounded full-file content
- diff-to-source navigation for working-tree and commit files, with inline
  addition/removal context and a complete new-state view
- full commit messages, changed-file lists, and lazy commit trees
- optional validated XDG or explicit keymap configuration
- opt-in profile-driven LSP navigation, hover, and document-symbol context
- bounded, asynchronous Git reads through a typed command allowlist
- count-aware Vim movement, document search, Code marks, and shared Vim/LSP jump history
- a framework-independent Vim motion and explicit Normal/Insert input contract
- responsive terminal layouts
- terminal restoration on normal exit, errors, Ctrl-C, and panics

## Module Map

| Module | Role | Notes |
| --- | --- | --- |
| [`crates/vim-navigation`](crates/vim-navigation) | Reusable text navigation | Public `command`, `motion`, and `editor` modules own incomplete Normal command state, pure cursor/viewport motion, and an explicitly mutable Normal/Insert buffer with configurable Insert escape input (`jj` by default). Private motion children separate semantic buffer scans, motion/viewport policy, Unicode display calculations, and tests. The crate has no Git, LSP, ratatui, or crossterm dependencies. |
| [`src/domain.rs`](src/domain.rs) + [`src/domain/`](src/domain) | Domain model | Owns validated repository paths, object IDs, changes, commits, diffs, search hits, file revisions/documents, document symbols, and tree entries without Git or terminal I/O. |
| [`src/git.rs`](src/git.rs) + [`src/git/`](src/git) | Repository adapter | Owns the Git read and explicit branch-switch command allowlist, bounded process/current/revision-file reads, machine-output parsing, and domain-level repository operations. |
| [`src/lsp.rs`](src/lsp.rs) + [`src/lsp/`](src/lsp) | Language-server adapter | Owns trusted profiles, bounded JSON-RPC transport, document synchronization, capability/position negotiation, and profile/workspace session lifecycle. |
| [`src/app.rs`](src/app.rs) + [`src/app/`](src/app) | Application state | Owns actions, events, effects, asynchronous load state, Git and Code workflow selection, diff-to-source projection, symbol/full-file overlays, caching, and stale-response rejection. |
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

The terminal event loop translates input into actions. `KeyMapper` uses
`vim_navigation::MotionState`; `app::vim` converts ChronoGit coordinates to
`vim_navigation::Cursor` and `Viewport`. Updating application
state may produce a typed Git effect, which is executed asynchronously and
returned as an event before the next render:

```text
crossterm event -> key map -> AppState update -> AppEffect + RequestId
                                            |             |
                                            |             `-> bounded executor
                                            |                    |-> GitService -> GitRunner
                                            |                    `-> LspManager -> stdio server
                                            `-> ratatui render              |
                                                      ^                     |
                                                      `------ Event <-------`
```

`src/domain` remains independent of process and terminal I/O. `src/git` is the
only layer allowed to invoke Git, `src/lsp` owns optional language-server
processes, and `src/tui` is the only layer that manages terminal state. Keep
those boundaries explicit so reducers and parsers remain testable without a
real terminal or language server.

## Design Boundaries

- Browsing remains read-only; explicit local-branch switching is the only mutation.
  Add Git operations through `GitCommand` and never bypass its closed allowlist.
  Switches acquire both Git worker permits. After completion, invalidate all
  repository state while preserving request IDs, then reload the active view.
- Keep `EditableBuffer` opt-in and outside ChronoGit's application state.
  ChronoGit search prompts retain their existing single-line
  confirm/cancel behavior, accept Space and `jj` literally, and are not generic
  Insert buffers.
- Keep Space's two roles at the adapter boundary: the generic crate implements
  Vim `RightWrap`, while ChronoGit's default normal context reserves Space as
  its application leader and leaves standalone Space motion unbound.
- Pass repository paths and pathspecs as separate process arguments; never
  interpolate them into shell text.
- Keep Git paths as bytes on Unix until presentation requires lossy rendering.
- Open current files relative to the discovered worktree descriptor and reject
  symbolic links in every path component.
- Read historical files only through a validated object ID and repository path;
  never check out a revision to display it.
- Represent exclusive UI states and load outcomes with enums instead of
  combinations of flags.
- Attach request IDs to asynchronous work and ignore completions that no longer
  match the selected resource. Reselect shared diff state when changing view
  families; background list results must not replace another view’s diff.
- Model incomplete key commands separately from completed actions.
- Keep global quit handling ahead of modal dispatch.
- Couple captured process bytes with their complete/truncated state through
  `CommandStream`; custom runners use the public `CommandOutput` constructor.
- Keep process output, task concurrency, caches, history pages, debounce time,
  and command duration bounded.
- Restore terminal state on every exit path before reporting an application
  failure.

Read the relevant implementation and tests before changing domain invariants,
Git commands or parsers, asynchronous state transitions, terminal lifecycle,
or platform support.
