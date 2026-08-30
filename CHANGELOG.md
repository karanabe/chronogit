# Changelog

All notable changes to ChronoGit are documented here.

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

### Fixed

- Diff navigation now gives immediate visible feedback for `j` / `k` and preserves `Ctrl-d` / `Ctrl-u` input entered while a diff is loading.
- Pressing `Enter` on a History commit now confirms it and moves focus to Changed files.

### Known limitations

- Windows and bare repositories are not supported.
- Staged-only changes are intentionally hidden.
- Merge commits are compared only with their first parent.
- The TUI requires an interactive terminal of at least 80x24.
- Cargo registry publishing remains disabled for the initial release.
