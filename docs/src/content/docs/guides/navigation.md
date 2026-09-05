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
| `q` | Close the current float or go back immediately |
| `Esc` | Cancel input; in Diff/Code dismiss search highlights first, then close/back |
| `Q` / `Ctrl-C` | Quit |
| `\1` / `\2` / `\3` / `\4` | Open Changes / History / Graph / Code |
| `\f` / `\g` | Search repository file names / working-tree content |
| `Ctrl-w h/k` / `Ctrl-w j/l` | Focus the previous / next pane (`Ctrl-w W/w`, control-letter, Backspace, and arrow aliases are available) |
| `[count]h/j/k/l`, arrows | Move by character or logical line |
| `[count]Space`, `[count]Backspace` / `Ctrl-H` | Move by character and wrap across lines like Vim's default `'whichwrap'` |
| `w/W/e/E`, `b/B/ge/gE` | Move by word or whitespace-delimited WORD (`Shift-←/→` is `b/w`; `Ctrl-←/→` is `B/W`) |
| `0`, `^`, `$`, `g_`, `g0`, `g^`, `g$`, `g<End>`, `gm`, `gM`, <code>\|</code> | Line and screen-column motions |
| `f/F/t/T{char}`, `;` / `,` | Find/till a character and repeat/reverse it |
| `gg`, `G`, `Ctrl-Home/End`, `[count]%`, `go`, `H/M/L` | Buffer line/percentage/byte and viewport-line motions |
| `(`/`)`, `{`/`}`, `[[`/`]]`, `[]`/`][` | Sentence, paragraph, and section motions |
| `%` / `g%`, `[( / [{`, `]) / ]}`, `[m/M / ]m/M`, `[# / ]#`, `[* / ]*`, `[/ / ]/` | Forward/reverse pair, unmatched delimiter, method, preprocessor, and comment motions |
| `Ctrl-d/u`, `Ctrl-f/b`, PageUp/Down, `Ctrl-e/y` | Half/full-page and single-line viewport movement |
| `zt/zz/zb`, `z<CR>/z./z-`, `z+/z^`, `zh/zl/zH/zL/zs/ze` | Cursor-relative and horizontal viewport movement |
| `[c` / `]c` | Previous / next diff change block |
| `m{char}`, `'{char}`, `` `{char}``, `g'{char}`, `` g`{char}`` | Set a Code mark; jump linewise / exactly, with `g` variants preserving the jump list |
| `['` / `` [` ``, `]'` / `` ]` `` | Previous / next lowercase Code mark, linewise / exactly |
| `r` | Refresh the current view |
| `\m` / `\b` / `\t` | Toggle complete message / History body layout / commit tree |
| `Enter` | Confirm/open a selection; in an open text document, move like `+` |
| `/` / `?`, `n` / `N`, `*` / `#`, `g*` / `g#` | Search the active text document |
| `K` | Toggle LSP hover at the Code cursor |
| `gd` / `gi`, `gy` / `gD` | LSP definition / implementation, type definition / declaration |
| `[count]Ctrl-o` / `[count]Ctrl-i` | Older / newer Vim or LSP jump location |
| `F1` | Toggle the in-app help overlay |

Numeric counts apply to Vim motions, character searches, search repetition, and jump-list traversal. Multi-key commands such as `gg`, `zh`, and `\f` must be completed within 750 ms; `f`, `t`, `m`, apostrophe, and backtick wait for their character argument without expiring. An unrelated key after a sequence prefix is handled normally. These are the built-in defaults; see [Keymap configuration](/reference/keymap/) to replace them.

The compatibility scope is Vim normal-mode movement that applies to ChronoGit's read-only text and list views. Editing operators, Insert/Visual mode, and movements that require an editable buffer are intentionally outside that scope.

## Pane behavior

Changes contains a file pane and a diff pane. Standard History stacks three full-width rows: commits, changed files/tree, and diff. Press `\b` for the alternative History layout, which stacks the same interactive commit list, commit body, and changed files. Graph is a full-height parent-lane list; its two-row commit details float over that list. Code always stacks an expandable working-tree file tree above the selected file content. File search results outside Code use two rows for history above content or diff.

- At 110 columns or wider, Changes shows its two panes together.
- From 80 through 109 columns, Changes gives the available width to its focused pane. History retains its three-row layout.
- Below 80 columns or 24 rows, the regular interface is replaced by a minimum-size message. `Q` and `Ctrl-C` remain available.
- At very wide sizes, the footer also includes the resolved repository root.

The highlighted border identifies the focused pane. Selection and scrolling commands apply to that pane. `h` / `l` are true Vim character motions in a text pane and select the adjacent pane in list-only contexts; `Ctrl-w h/k/j/l` always changes pane focus. In History's Commits pane, `Enter` confirms the selected commit and moves focus directly to Changed files.

## Overlays

Help, Graph details, repository search, complete commit messages, current file content, Code files, and selected-file diffs open above the main panes. Press `\m` again to close a message. Text overlays retain the Vim character cursor and movement vocabulary. `Enter` moves to the next line's first nonblank like `+`; `q` closes the overlay immediately. In Diff/Code, `Esc` first dismisses search highlights, then closes on the next press. In repository search, `Enter` or `Ctrl-j` moves from Search to Results, while `Ctrl-w k` returns to Search with the current query ready to edit. Outside text entry, `q` closes the current float or returns from a detail/file view. `Esc` does the same when no search highlights remain. While a search prompt is active, `q` and `Q` are query text, `Esc` cancels input, and `Ctrl-C` quits. `/` starts a forward search and `?` a backward search; `n` / `N` repeat it with counts and wraparound. `*` / `#` search for the whole word at the cursor, while `g*` / `g#` allow a partial-word match. Lowercase queries ignore case, while any uppercase character makes the query case-sensitive.

In document searches (`/` or `?`), Backspace deletes the last character. Deleting the last character leaves an empty prompt so you can type a replacement; press Backspace once more to cancel. Esc cancels at any point. Cancellation removes the input cursor (`█`) and keeps the document, focus, cursor, scroll position and previous confirmed search, including its direction and highlight visibility. `n` / `N` resumes that search. Enter in an empty prompt reuses the previous query. A retained search status such as `/word 1/3` is not input mode. Repository searches (`\f` / `\g`) keep their existing behavior.

## Exit and terminal restoration

Startup failures occur before raw mode is enabled and leave the terminal untouched. Once the TUI is active, normal exit, `Q`, `Ctrl-C`, runtime errors, and panics are designed to restore raw mode, the cursor, mouse capture, and the alternate screen. If your shell still looks incorrect after a forced kill or terminal failure, see [Troubleshooting](/troubleshooting/common-problems/#the-terminal-was-not-restored).

Diff and Code panes and floats decorate only matched strings: the current match has a yellow background, other matches are underlined, and the cursor stays cyan. Default `Esc` removes only search decoration, preserving the query, direction, focus, cursor and scroll position. `n` / `N` or a confirmed search restores it. No matches means no extra Esc. Prompt and find/till/mark cancellation preserve the previous search and its visible or dismissed state. Frontmost help, hover and repository search close before any underlying highlights are dismissed.
