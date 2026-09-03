---
title: Navigation and layout
description: Use ChronoGit's Vim-oriented keys, panes, overlays, and responsive layout.
tags:
  - keyboard
  - navigation
  - accessibility
sidebar:
  order: 4
---

ChronoGit is operated entirely from the keyboard. Press `F1` inside the application for a compact reminder.

## Key reference

| Key | Action |
| --- | --- |
| `q` / `Esc` | Close the current float or go back |
| `Q` / `Ctrl-C` | Quit |
| `1` / `2` / `3` | Open the Git workflow's Changes / History / Graph views |
| `4` | Open the working-tree Code viewer |
| `Space f` / `Space g` | Search repository file names / working-tree content |
| `h` / `l` | Focus the previous / next pane |
| `Ctrl-k` / `Ctrl-j` | Focus the previous / next pane |
| `j` / `k` or `↓` / `↑` | Move selection or scroll down / up |
| `g` / `G` or `Home` / `End` | Move to the first / last item |
| `Ctrl-d` / `Ctrl-u` | Move or scroll half a page down / up |
| `zh` / `zl` | Scroll a diff or code document left / right |
| `r` | Refresh the current view |
| `m` | Toggle the complete commit-message overlay |
| `b` | Toggle History's diff / body layout |
| `t` | Toggle changed files / commit tree |
| `Enter` | Confirm or open the selected item, or close a floating full view |
| `/` / `?` | Search a floating diff or Code file forward / backward |
| `n` / `N` | Go to the next / previous search match |
| `F1` | Toggle the in-app help overlay |

The `zh`, `zl`, `Space f`, and `Space g` sequences must be completed within 750 ms. An unrelated key after a sequence prefix is handled normally. These are the built-in defaults; see [Keymap configuration](/reference/keymap/) to replace them.

## Pane behavior

Changes contains a file pane and a diff pane. Standard History stacks three full-width rows: commits, changed files/tree, and diff. Press `b` for the alternative History layout, which stacks the same interactive commit list, commit body, and changed files. Graph is a full-height parent-lane list; its two-row commit details float over that list. Code always stacks an expandable working-tree file tree above the selected file content. File search results outside Code use two rows for history above content or diff.

- At 110 columns or wider, Changes shows its two panes together.
- From 80 through 109 columns, Changes gives the available width to its focused pane. History retains its three-row layout.
- Below 80 columns or 24 rows, the regular interface is replaced by a minimum-size message. `Q` and `Ctrl-C` remain available.
- At very wide sizes, the footer also includes the resolved repository root.

The highlighted border identifies the focused pane. Selection and scrolling commands apply to that pane, while `h` / `l` or `Ctrl-k` / `Ctrl-j` change focus. In History's Commits pane, `Enter` confirms the selected commit and moves focus directly to Changed files.

## Overlays

Help, Graph details, repository search, complete commit messages, current file content, Code files, and selected-file diffs open above the main panes. Press `m` again to close a message. Navigation keys scroll message, content, and diff overlays; `Enter` closes a full content or diff view with the same key that opened it. In repository search, `Enter` or `Ctrl-j` moves from Search to Results, while `Ctrl-k` returns to Search with the current query ready to edit. Outside text entry, `q` and `Esc` close the current float or return from a detail/file view. While a repository, diff, or Code-search prompt is active, `q` and `Q` are query text, `Esc` cancels input, and `Ctrl-C` quits; cancelling a repository-search prompt also closes that overlay. In a floating diff or Code file, `j` / `k` move the current-line gutter marker down / up and the viewport follows it without recoloring the code. `Ctrl-d` / `Ctrl-u` move it half a page, including when entered while the document is still loading. `/` starts a forward search, and `?` a backward search. Type a query and press `Enter`; `n` / `N` move through highlighted matches with wraparound. Lowercase queries ignore case, while any uppercase character makes the query case-sensitive.

## Exit and terminal restoration

Startup failures occur before raw mode is enabled and leave the terminal untouched. Once the TUI is active, normal exit, `Q`, `Ctrl-C`, runtime errors, and panics are designed to restore raw mode, the cursor, mouse capture, and the alternate screen. If your shell still looks incorrect after a forced kill or terminal failure, see [Troubleshooting](/troubleshooting/common-problems/#the-terminal-was-not-restored).
