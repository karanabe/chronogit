# Changelog

All notable changes to ChronoGit are documented here.

## Unreleased

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
