---
title: Manual terminal smoke test
description: Verify rendering, navigation, and terminal restoration on Linux and macOS.
tags:
  - testing
  - terminal
  - checklist
sidebar:
  order: 3
---

Run this checklist on both Linux and macOS before signing a release. Use a terminal with color support and a UTF-8 locale, not a captured CI command. Record the terminal application and OS version with the result.

## Setup

From a clean ChronoGit checkout:

```sh title="Terminal"
cargo install --path . --locked
before_stty=$(stty -g)
printf 'locale=%s term=%s\n' "${LC_ALL:-${LANG:-unset}}" "${TERM:-unset}"
```

Choose a non-bare test repository containing:

- an unstaged text change with added and removed lines;
- a Unicode filename and Unicode file content;
- at least one root, normal, and merge commit;
- a commit that changes a binary file;
- a directory nested at least two levels deep.

ChronoGit is read-only, but a disposable repository makes it easier to adjust fixtures safely.

## Pane-focus controls and input bytes

Record the ChronoGit revision, OS, terminal, multiplexer (or none), dimensions, and active keymap. Before starting ChronoGit, record the five input bytes for physical `Ctrl-h`, `Ctrl-j`, `Ctrl-k`, `Ctrl-l`, and Backspace in that order:

```sh title="Terminal"
pane_stty=$(stty -g)
trap 'stty "$pane_stty"' 0 1 2 15
stty raw -echo
od -An -t x1 -N 5
stty "$pane_stty"
trap - 0 1 2 15
```

A common distinct result is `08 0a 0b 0c 7f`. If Backspace is also `08`, record that physical Backspace and `Ctrl-h` are indistinguishable in this environment instead of claiming both behaviors independently.

1. With the built-in keymap, press each standalone control separately in Changes, History / Commit details, Graph / Graph details, File history, and Code. Record the starting and ending pane. Confirm `Ctrl-h` and `Ctrl-k` each move backward, `Ctrl-j` and `Ctrl-l` each move forward, every edge stops, and Graph remains on its only pane.
2. Repeat in Changes at 80–109 columns and at 110 columns or wider. Confirm the focused pane replaces the visible pane at narrow widths and only the focused border changes at wide widths. Resize below the supported minimum and confirm focus commands do not escape ChronoGit.
3. Repeat representative movements with `Ctrl-w h/k/j/l` and its documented aliases. Confirm unmodified `h/j/k/l`, arrows, a separately reported Backspace, and remaining control motion aliases still move within the focused content.
4. In repository Search input, Results, document-search input, a find/till wait, a mark wait, Help, and each available single text/hover/target overlay, try all four standalone controls. Confirm input and frontmost-overlay behavior takes priority. From repository Results, either previous control preserves the query and returns to Search; either next control stays in Results.
5. Start with temporary keymaps that replace only `focus_previous`, only `focus_next`, and both. Include one case retaining only some standalone controls and one moving all four to other keys while relisting any `Ctrl-w` aliases to keep. Confirm removed defaults no longer focus panes and duplicate/prefix conflicts fail before the alternate screen opens.
6. Compare `git status --short`, `git diff`, the index, and `HEAD` before and after the session. ChronoGit must not change any of them.

## Changes workflow

1. Run `chronogit /absolute/path/to/test-repository --view changes`.
2. Confirm borders, arrows, and Unicode filenames occupy stable columns.
3. Confirm added, removed, hunk, header, and metadata lines are visually distinct. On short or nearly empty added and removed rows, and on hunk rows, confirm the existing background reaches the right edge of the diff content and stops before the border. Context, header, and metadata rows must not gain a background.
4. Move rapidly through files with `j` and `k`; confirm the final displayed diff matches the final selection.
   - With the diff ready, press `Space v`; confirm the complete working-tree file opens at the corresponding new-side line. Confirm Changes highlights additions and inserts red removed rows at their former positions, then toggle `Space d` and confirm the plain new-state view hides those removed rows without changing the source position. Without `--lsp`, `Space s` must show a disabled notice without opening the chooser.
5. Resize from at least 140×40 to approximately 90×24. Confirm multiple panes become one focused pane, each standalone `Ctrl-h/k/j/l` changes focus in its documented direction, and diff row backgrounds follow the current content edge without entering a border or adjacent pane. Repeat with the retained `Ctrl-w` forms.
6. Resize below 80×24. Confirm the minimum-size message and quit hint appear without a crash, then resize back.
7. Open help with `F1`, close it with `q`, then exit with uppercase `Q`.

After returning to the shell:

```sh title="Terminal"
test "$(stty -g)" = "$before_stty"
printf 'terminal accepts normal input after Q\n'
```

Confirm typed input echoes normally, the cursor is visible, mouse selection works normally, and the previous screen contents are restored.

## History workflow

1. Run `chronogit /absolute/path/to/test-repository --view history`.
2. At both 140×40 and 80×24, confirm commits, changed files/tree, and diff are visible as three full-width rows and long subjects/paths remain readable.
3. Visit root, normal, and merge commits. Confirm the footer and diff title describe `empty tree`, `parent`, or `first parent` as appropriate.
4. With Commits focused, press `Enter` and confirm focus moves directly to Changed files for the selected commit.
5. Select changed text and binary files, press `Enter`, and confirm a large floating patch or binary summary opens. In text, confirm `Enter` moves to the next line's first nonblank. Close it with `q` and, when no search highlights are present, `Esc`.
6. Open a recognized source file in both the regular pane and floating diff. Confirm code tokens are syntax-highlighted, addition/removal/hunk backgrounds reach the content edge, and the current-line gutter marker does not recolor the code. Include a tab, a wide character, and a long line; scroll horizontally and confirm the text still clips and scrolls independently of the background. While opening an uncached long text diff, immediately press `Ctrl-d` and confirm the marker moves half a page as soon as the diff appears. Confirm `j` / `k` visibly move it one line and `Ctrl-u` moves it up without a delay.
7. Exercise counts plus `w/W/e/E`, `b/B/ge/gE`, `0/^/$/g_`, `f/F/t/T` with `;` / `,`, `gg/G/%/go/H/M/L`, sentence/paragraph/section and delimiter motions, page/scroll/`z` motions, and `[c` / `]c`. Search with `/`, `?`, `n/N`, `*` / `#`, and `g*` / `g#`.
   - From a commit diff, press `Space v`; confirm the file is read from the selected commit rather than the working tree and opens at the corresponding line. Toggle `Space d`. With a matching trusted LSP profile, use `Space s`, confirm only symbols containing changed new-side lines appear, select one and verify the complete file jumps to it, then use the explicit full-file row without selecting a symbol.
8. Press `Space m`, move through the complete commit message with character and word motions, and close it separately with `Space m`, `q`, and `Esc`.
9. Press `Space b` and confirm the rows are the same commit list, commit body, and changed files. Use each standalone `Ctrl-h/k/j/l`, then the retained `Ctrl-w` forms, to move focus. Change the top-row commit and confirm the other rows update, scroll the body, and open a bottom-row file diff. Press `Space b` again to return to standard History.
10. Press `Space t`, expand and collapse two directory levels, and open a blob diff.
11. Exit with `Ctrl-C`, then repeat the `stty` comparison and shell checks.

## Graph and repository search

1. Press `Space 3`; confirm parent lanes and commit subjects are visible. Press `Space m` and close the complete message.
2. Press `Enter`; confirm a bordered two-row window floats over the still-visible Graph, with changed files above the selected diff. Press `Enter` for the full diff, use `q` to close it, then use `q` again to return to Graph. Repeat with `Esc`.
3. From Changes, History, and Graph, run `Space f` and type a known path one character at a time. Confirm results update before `Enter`, use `Enter` or the input-reserved `Ctrl-j` to focus Results, then separately use `Ctrl-h`, `Ctrl-k`, and retained `Ctrl-w k` from Results to return to Search. Edit the preserved query and confirm live results update again before opening it; confirm file history is above current content. Verify `Ctrl-j/l` from Results leave it focused.
4. Change the history selection and confirm the lower pane becomes that commit's diff. Open and close the full diff, then press `q` or `Esc` back to the originating view.
5. Run `Space g`, type known text, confirm live results follow each edit and deletion, open a result, and confirm the matching current-content line is highlighted. Reopen the prompt, enter a query containing Space and `jj` as well as both `q` and uppercase `Q`, and confirm all are inserted and update results. Confirm `Esc` closes the prompt and `Ctrl-C` quits.
6. Start once with the default XDG keymap and once with `--keymap` pointing to a valid custom binding. Confirm an invalid explicit file fails before the alternate screen opens.

## Code workflow

1. Press `Space 4`, then confirm a tracked root file and a collapsed nested directory appear above the code pane. Repeat by starting with `--view code`.
2. Move onto a file and confirm its current syntax-highlighted content loads below. Move rapidly between files and confirm the final content matches the final selection.
3. Press `Enter` on a directory, expand at least two levels, then press it again and confirm all descendants collapse.
4. Move between tree and code with each standalone `Ctrl-h/k/j/l`, then repeat with the retained `Ctrl-w` forms. In the code pane exercise the complete count-aware motion set, including wanted-column behavior across short lines.
5. Press `Enter` from both a tree file and the lower pane. Confirm the full Code window opens, `Enter` moves like `+`, searches wrap, and `q` returns to Code immediately; `Esc` first dismisses search highlights when present.
6. With a language server enabled, move the character cursor onto a symbol. Confirm `K` opens and closes hover, `gd` / `gi` / `gy` / `gD` request the four semantic targets, and `Ctrl-o` / `Ctrl-i` move backward and forward through successful jumps. After going backward, make a new jump and confirm the former forward location is no longer reachable.
   - From focused Code content and its float, press `Space s`; confirm the document-symbol list includes the explicit full-file row, selecting a symbol opens the complete file at that position, and `Space v` can open the same file directly. Repeat from current content in file history.
7. Run `Space f` and `Space g` from Code. Open a nested result and confirm Code returns directly, expands the path in the tree, and places the marker on the content-match line. Set lowercase and uppercase marks, jump with apostrophe and backtick, cross files, and traverse the combined history with counted `Ctrl-o` / `Ctrl-i`.
8. Open a binary, symbolic link, deleted tracked path, and file larger than 8 MiB. Confirm each displays a safe summary or truncation marker and no symbolic-link target is read.

## Search highlight dismissal

Record the version/revision, OS, terminal, dimensions, query, pane/float and keymap.
Compare before and after using the same file and operation sequence. If the
reported environment cannot be reproduced, record that limitation.

1. In Diff, Code, and the new-state full-file view, test the applicable pane or float at 140×40 and 80×24. Use `/needle`, `Enter`, `n`, `Esc`, then repeat with `?needle` and `N`.
   Read the surrounding text before and after Esc: only matched strings should
   carry search styling, the current and other matches must be distinguishable,
   and syntax colors, added/removed diff meaning, cursor, focus and scroll must
   survive dismissal. Check gutters and end-of-line padding too.
2. Resume with `n` / `N`, including counts and wraparound, without retyping.
   Dismiss again and confirm a different search. Compare a second Esc after
   dismissal with `q` during highlighting: both follow the existing close/back
   path. A no-match query must not require an extra Esc.
3. Cancel `/` and `?` input with both visible and dismissed previous highlights.
   Cancel find/till and mark character waits. Confirm those Esc presses cancel
   only input. Close frontmost help and repository search (prompt and results).
   Check hover if an existing opt-in server setup is available; otherwise record
   why it was not observed.
4. Include Japanese, tabs, matching whitespace, several hits on one line and
   horizontal scrolling through a hit. Confirm styling stays attached to the
   visible match and still permits reading the surrounding context.
5. Read F1 help and the active search hints. Repeat with `close = x`,
   `close = q, esc`, and `close = x` plus `refresh = esc`; record the actual
   mappings and check immediate close and explicit Esc reassignment.

Record each result and any confusing behavior. Automated cell/key checks and
agent-operated terminal sessions do not establish maintainer visual/use sign-off.

## Empty document-search cancellation

At 80×24 and 140×40, test Code, Diff, current file content and commit messages
in their existing panes and floats, using the actual terminal Backspace key.
Record the revision, terminal, keymap, operation sequence and observations.

1. Try `/`, Backspace, `j` / `k`, then repeat with `?`. The input cursor must
   disappear without changing focus, cursor or either scroll offset; movement
   must resume in the same pane/float without closing it or moving left on cancel.
2. Try `/a`, Backspace, replacement text, Enter; then `/a`, Backspace, Backspace.
   Repeat backward, including Japanese, spaces and literal `/` / `?` characters.
   The last deletion must leave the prompt open, and only the next Backspace
   cancels. Check that the empty-prompt hint describes this boundary.
3. With a previous search visible and dismissed, cancel same/opposite prompts.
   Compare positions and highlights, then resume with `n` / `N`. A previous
   status may remain, but the input cursor must be absent. Also test no prior
   search, Esc cancellation, empty Enter, normal Backspace and custom keys.
4. Confirm repository-search empty queries, live edits and Search/Results focus
   still work. Keep frontmost help, hover and target-list input independent.

Record maintainer use feedback separately from automated and agent-operated
checks, including anything confusing about replacement input or resumed browsing.

## Sign-off

Do not mark a platform complete from automated tests alone.

| Platform | OS version | Terminal | Color/Unicode/resize | `Q` cleanup | Ctrl-C cleanup | Tester/date |
| --- | --- | --- | --- | --- | --- | --- |
| Linux | | | | | | |
| macOS | | | | | | |
