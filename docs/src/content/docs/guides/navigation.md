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
| `q` / `Ctrl-C` | Quit |
| `1` / `2` | Open Changes / History |
| `h` / `l` | Focus the previous / next pane |
| `Ctrl-k` / `Ctrl-j` | Focus the previous / next pane |
| `j` / `k` or `↓` / `↑` | Move selection or scroll down / up |
| `g` / `G` or `Home` / `End` | Move to the first / last item |
| `Ctrl-d` / `Ctrl-u` | Move or scroll half a page down / up |
| `zh` / `zl` | Scroll a diff left / right |
| `r` | Refresh the current view |
| `m` | Toggle the complete commit-message overlay |
| `b` | Toggle History's diff / body layout |
| `t` | Toggle changed files / commit tree |
| `Enter` / `Space` | Confirm a History commit, open/close a selected file's floating diff, or expand/collapse a tree directory |
| `/` / `?` | Search a floating diff forward / backward |
| `n` / `N` | Go to the next / previous search match |
| `Esc` | Close the active overlay |
| `F1` | Toggle the in-app help overlay |

The `zh` and `zl` sequences must be completed within 750 ms. An unrelated key after `z` is handled normally.

## Pane behavior

Changes contains a file pane and a diff pane. Standard History stacks three full-width rows: commits, changed files/tree, and diff. Press `b` for the alternative History layout, which stacks the same interactive commit list, commit body, and changed files.

- At 110 columns or wider, Changes shows its two panes together.
- From 80 through 109 columns, Changes gives the available width to its focused pane. History retains its three-row layout.
- Below 80 columns or 24 rows, the regular interface is replaced by a minimum-size message. `q` and `Ctrl-C` remain available.
- At very wide sizes, the footer also includes the resolved repository root.

The highlighted border identifies the focused pane. Selection and scrolling commands apply to that pane, while `h` / `l` or `Ctrl-k` / `Ctrl-j` change focus. In History's Commits pane, `Enter` confirms the selected commit and moves focus directly to Changed files.

## Overlays

Help, complete commit messages, and selected-file diffs open above the main panes. Press `m` again to close a message. Navigation keys scroll message and diff overlays; `Enter` or `Space` closes a diff with the same key that opened it, and `Esc` closes either overlay. In a floating diff, `j` / `k` move the highlighted current line down / up and the viewport follows it. `Ctrl-d` / `Ctrl-u` move it half a page, including when entered while the diff is still loading. `/` starts a forward search, and `?` a backward search. Type a query and press `Enter`; `Esc` cancels the prompt, and `n` / `N` move through highlighted matches with wraparound. Lowercase queries ignore case, while any uppercase character makes the query case-sensitive.

## Exit and terminal restoration

Startup failures occur before raw mode is enabled and leave the terminal untouched. Once the TUI is active, normal exit, `q`, `Ctrl-C`, runtime errors, and panics are designed to restore raw mode, the cursor, mouse capture, and the alternate screen. If your shell still looks incorrect after a forced kill or terminal failure, see [Troubleshooting](/troubleshooting/common-problems/#the-terminal-was-not-restored).
