# Changelog

All notable changes to ChronoGit are documented here.

## Unreleased

### Added

- `zt`, `zz`, and `zb` positioning in file and commit lists, including counted
  forms, with retained viewport positions when moving in either direction.
- Shared `scrolloff` cursor context for lists, diffs, and source views, defaulting
  to two rows. Configure it in the user-level `chronogit/config.toml`, select a
  file with `--config`, or override it with `--scrolloff` (`0` disables).

### Fixed

- Keep diff line numbers fixed during horizontal scrolling and keep the cursor
  visible when returning from `$` to `0` in regular and floating diff panes.

## 0.7.0

This release pairs ChronoGit 0.7.0 with the unchanged `vim-navigation` 0.2.0.

### Added

- History starts with the complete commit message in its bottom pane. `Space m`
  toggles message/diff, and Enter on a commit selects the diff preview. Pane
  focus commands preserve the selected preview except when returning to Commits,
  which restores the message.

- Switch existing local branches from any view with `Space b`, with conflict
  protection and automatic history, diff, and code refresh. The History body
  layout toggle uses `Space B`.

- Added `CommandStream` and a public `CommandOutput` constructor so external
  `GitRunner` implementations can return complete or truncated output.

### Fixed

- Preserve source positions for header-like added/removed patch lines, reject
  invalid hunk coordinates, and omit incomplete lines from truncated patches.
- Restore the selected diff when returning to a view and prevent background
  list completions from replacing another view's diff.
- Read a fresh working-tree snapshot when reopening full source from a diff.
- Honor global quit from semantic overlays.
- Keep very large counted list movements bounded without overflow or direction
  reversal.
- Bound LSP header reads and queued writes, include queue waits in request
  deadlines, and stop transport tasks when their connection is dropped.
- Preserve trailing CR/LF bytes when discovering a repository root.

### Changed

- Represent mark-argument waits as typed binding commands instead of sentinel
  characters in completed actions.

## 0.6.0

This release pairs ChronoGit 0.6.0 with `vim-navigation` 0.2.0.

### Added

- Added a diff-to-source review flow for worktree and historical commit files:
  open the complete new-state file directly, toggle an annotated view with
  highlighted additions and inline removals, or
  ask an enabled language server for document symbols and jump from the
  changed-symbol context into the complete file.
- Converted the repository to a Cargo workspace and added the generic
  `vim-navigation` crate for Vim-compatible cursor/viewport motion, command
  state, and explicit Normal/Insert text input. ChronoGit is its first
  local-path user while keeping document content read-only.
- Added a fixed Vim 9.1.1244 compatibility inventory and executable 85-case
  cursor/viewport oracle, plus contextual search, mark, jump, diff, Unicode,
  large-count, snapshot, and input-limit coverage.

### Changed

- Replaced interchangeable primitive flags and counters across the public and
  internal boundaries with validated values: non-zero `CommitPage` pagination,
  one-based `LineNumber`, typed Git tree modes and LSP symbol kinds, distinct
  request/document generations, consistent exact/display-only/truncated file
  documents, typed command/stream outcomes, and explicit mark-jump, search,
  key-input, LSP-availability, source-context, expansion,
  history-continuation, and load modes.
  `ObjectId`, `RepoPath`, and `RepositoryRoot` also expose standard conversion
  traits, and manually counted `vim-navigation` motions now take
  `CountSource::{Explicit, Implicit}`.
- Centralized terminal layout measurements shared by rendering and viewport
  motion, and named Git/LSP buffer, retry, protocol, and display limits.
- File-history and Code content can open the same complete-file and document-
  symbol views; symbol requests remain unavailable until at least one trusted
  `--lsp` profile is explicitly enabled.
- ChronoGit uses `vim-navigation` through a versioned path dependency. Registry
  packaging requires publishing the dependency first; workspace builds and
  source installs use the local crate.
- ChronoGit now uses `Space` as the default application leader for view,
  repository-search, message, layout, and tree actions. The app default leaves
  standalone Space motion unbound; the generic crate retains Vim `RightWrap`.
- `EditableBuffer` now defaults to `jj` for leaving Insert mode while retaining
  Esc. The sequence can be disabled or replaced, and a pending prefix is
  inserted immediately so context changes cannot lose input.
- Standalone `Ctrl-h` / `Ctrl-k` and `Ctrl-j` / `Ctrl-l` now focus the previous
  and next pane by default. Existing `Ctrl-w` sequences remain available, and
  `focus_previous` / `focus_next` still replace all aliases for their action.

### Fixed

- Addition, removal, and hunk backgrounds now continue through the available
  diff content width in regular and floating panes without extending logical
  text or crossing pane borders.

## 0.5.0

### Added

- Code, file, commit-message, and diff documents now support count-aware Vim normal-mode movement, including word/WORD, line, character-find, structural, viewport, horizontal-scroll, mark, jump-list, and search motions with a visible character cursor.

### Changed

- Empty document-search prompts can be cancelled with Backspace. Deleting the last character still allows replacement input; cancellation preserves the previous search and viewing position.
- Diff and Code search decoration is confined to matched strings. Default `Esc` hides it without losing the query, direction or viewing position; `n` / `N` and confirmed searches restore it. Explicit `close` bindings retain immediate close/back behavior.
- The built-in leader is now `\`, leaving `Space` available for Vim's rightward motion. View, repository-search, message, layout, and tree commands use `\1` through `\4`, `\f` / `\g`, and `\m` / `\b` / `\t`. Pane focus uses `Ctrl-w h/k/j/l`; `Ctrl-w k` returns repository-search results to query editing.
- Unmodified `1` through `9` are reserved for counts and cannot start custom key bindings; use a leader sequence or modifier instead.
- Text overlays use `Enter` as Vim's `+` motion; `q` closes them immediately, while default `Esc` first dismisses active Diff/Code search highlights. List panes accept counts and the applicable Vim line, word, window, and page motions.

### Fixed

- Document-search prompts and confirmed search status appear once when a text float is open, without a duplicate in the main footer.

## 0.4.0

### Added

- Opt-in Language Server Protocol navigation in the current-working-tree Code viewer for definition, implementation, type definition, declaration, multiple candidates, and bounded bidirectional jump history.
- Generic trusted user-level server profiles plus built-ins for rust-analyzer, Eclipse JDT LS, Pyright, basedpyright, and Python LSP Server; repeatable `--lsp` supports polyglot repositories without language-specific client implementations.
- Character-accurate `h`/`l` Code cursors and capability-checked LSP hover in a scrollable floating window.

### Changed

- Vim-oriented defaults now use `gg`/`G` for first/last, `K` for hover, `gd`/`gi`/`gy`/`gD` for semantic navigation, and `Ctrl-o`/`Ctrl-i` for older/newer jump locations.

### Security

- LSP stays disabled by default, launches direct argument arrays without implicit shell expansion, bounds protocol messages and resident sessions, rejects ambiguous profiles and repository-external/virtual targets, and cleans up child processes on exit.
- Documentation now distinguishes ChronoGit's repository read-only contract from explicitly enabled external servers that may run project tooling or write caches/build artifacts.

## 0.3.0

### Added

- Working-tree Code viewer with a directory-first expandable tree of tracked and non-ignored untracked files, syntax-highlighted preview, full-screen content, and diff-style navigation and in-document search.
- `4`, `--view code`, and the configurable `show_code` action for entering the Code workflow while preserving the existing Changes landing view.
- Repository file/content search now returns directly to Code, reveals the selected tree path, and positions content searches at the matched line.

## 0.2.0

### Added

- Parent-lane Git graph view with commit messages, changed-file/diff details, and a full-diff overlay.
- Global `Space f` file-path search and `Space g` fixed-text working-tree search with per-file history and current-content browsing.
- Optional XDG or `--keymap` configuration with validated action names, key sequences, alternatives, and ambiguity checks.
- Syntax highlighting for recognized source files and diff hunks using the embedded `syntect` and `two-face` grammar set.

### Changed

- `Space` is now the repository-search leader; `Enter` remains the activation and floating-view close key.
- Repository file lists, content matches, file histories, and current file reads share the existing read-only, bounded asynchronous pipeline.
- Repository searches now refresh after every query edit while stale asynchronous results remain ignored; `Ctrl-j` focuses Results and `Ctrl-k` returns to Search for another live query.
- `q` and `Esc` now close or go back, while `Q` and Ctrl-C quit; Graph commit details now appear as a floating window over the graph.
- Diff additions and removals now use muted, syntax-preserving backgrounds; `j` / `k` navigation uses a gutter marker instead of recoloring the selected code row.

### Fixed

- Search prompts now accept `q` and uppercase `Q` as query text; `Esc` cancels input and Ctrl-C remains the global quit key.
- Current-file previews reject symbolic links in every path component and use descriptor-relative file opens to prevent reads outside the discovered worktree.

## 0.1.0

### Added

- Read-only Vim-oriented TUI for unstaged tracked, untracked, deleted, renamed, type-changed, and conflicted worktree files.
- Unified text diffs with old/new line numbers, binary summaries, bounded output, and truncation notices.
- Paged commit history with first-parent merge semantics and root commit support.
- Changed-file navigation, a full commit-message overlay, and an alternative three-row History layout for commits, body, and files.
- Lazy commit-tree navigation for directories, files, symlinks, and submodules.
- Responsive Changes layout and a full-width, three-row History layout with in-app key help.
- Floating full-file diff navigation with forward/backward, smart-case, wraparound search.
- Previous/next-pane navigation with `Ctrl-k` / `Ctrl-j`, plus same-key closing for floating diffs.
- Viewport-following list selection and selection preservation across refreshes.
- Typed application state, stale asynchronous response rejection, bounded concurrency, and diff caching.
- Linux/macOS terminal lifecycle protection for normal exit, errors, `q`, Ctrl-C, and panics.
- 8 MiB output and 30-second Git-process safety limits.
- Codex-first companion skill, with Claude Code and Grok Build setup, trigger boundaries, separate-terminal handoff, and relaunch guidance.
- crates.io package metadata and a source-only package allowlist.

### Fixed

- Diff navigation now gives immediate visible feedback for `j` / `k` and preserves `Ctrl-d` / `Ctrl-u` input entered while a diff is loading.
- Pressing `Enter` on a History commit now confirms it and moves focus to Changed files.

### Known limitations

- Windows and bare repositories are not supported.
- Staged-only changes are intentionally hidden.
- Merge commits are compared only with their first parent.
- The TUI requires an interactive terminal of at least 80x24.
