# Changelog

All notable changes to ChronoGit are documented here.

## Unreleased

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
