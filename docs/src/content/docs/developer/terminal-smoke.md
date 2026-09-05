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

## Changes workflow

1. Run `chronogit /absolute/path/to/test-repository --view changes`.
2. Confirm borders, arrows, and Unicode filenames occupy stable columns.
3. Confirm added, removed, hunk, header, and metadata lines are visually distinct.
4. Move rapidly through files with `j` and `k`; confirm the final displayed diff matches the final selection.
5. Resize from at least 140×40 to approximately 90×24. Confirm multiple panes become one focused pane and `Ctrl-w h` / `Ctrl-w l` changes focus.
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
6. Open a recognized source file and confirm code tokens are syntax-highlighted, additions/removals retain muted green/red backgrounds, and the current-line gutter marker does not recolor the code. While opening an uncached long text diff, immediately press `Ctrl-d` and confirm the marker moves half a page as soon as the diff appears. Confirm `j` / `k` visibly move it one line and `Ctrl-u` moves it up without a delay.
7. Exercise counts plus `w/W/e/E`, `b/B/ge/gE`, `0/^/$/g_`, `f/F/t/T` with `;` / `,`, `gg/G/%/go/H/M/L`, sentence/paragraph/section and delimiter motions, page/scroll/`z` motions, and `[c` / `]c`. Search with `/`, `?`, `n/N`, `*` / `#`, and `g*` / `g#`.
8. Press `\m`, move through the complete commit message with character and word motions, and close it separately with `\m`, `q`, and `Esc`.
9. Press `\b` and confirm the rows are the same commit list, commit body, and changed files. Use `Ctrl-w h/k/j/l` to move focus, change the top-row commit and confirm the other rows update, scroll the body, and open a bottom-row file diff. Press `\b` again to return to standard History.
10. Press `\t`, expand and collapse two directory levels, and open a blob diff.
11. Exit with `Ctrl-C`, then repeat the `stty` comparison and shell checks.

## Graph and repository search

1. Press `\3`; confirm parent lanes and commit subjects are visible. Press `\m` and close the complete message.
2. Press `Enter`; confirm a bordered two-row window floats over the still-visible Graph, with changed files above the selected diff. Press `Enter` for the full diff, use `q` to close it, then use `q` again to return to Graph. Repeat with `Esc`.
3. From Changes, History, and Graph, run `\f` and type a known path one character at a time. Confirm results update before `Enter`, use `Enter` or `Ctrl-j` to focus Results, then use `Ctrl-w k` to return to Search. Edit the query and confirm live results update again before opening it; confirm file history is above current content.
4. Change the history selection and confirm the lower pane becomes that commit's diff. Open and close the full diff, then press `q` or `Esc` back to the originating view.
5. Run `\g`, type known text, confirm live results follow each edit and deletion, open a result, and confirm the matching current-content line is highlighted. Reopen the prompt, enter a query containing both `q` and uppercase `Q`, and confirm both are inserted and update results. Confirm `Esc` closes the prompt and `Ctrl-C` quits.
6. Start once with the default XDG keymap and once with `--keymap` pointing to a valid custom binding. Confirm an invalid explicit file fails before the alternate screen opens.

## Code workflow

1. Press `\4`, then confirm a tracked root file and a collapsed nested directory appear above the code pane. Repeat by starting with `--view code`.
2. Move onto a file and confirm its current syntax-highlighted content loads below. Move rapidly between files and confirm the final content matches the final selection.
3. Press `Enter` on a directory, expand at least two levels, then press it again and confirm all descendants collapse.
4. Move between tree and code with `Ctrl-w h/k/j/l`. In the code pane exercise the complete count-aware motion set, including wanted-column behavior across short lines.
5. Press `Enter` from both a tree file and the lower pane. Confirm the full Code window opens, `Enter` moves like `+`, searches wrap, and `q` returns to Code immediately; `Esc` first dismisses search highlights when present.
6. With a language server enabled, move the character cursor onto a symbol. Confirm `K` opens and closes hover, `gd` / `gi` / `gy` / `gD` request the four semantic targets, and `Ctrl-o` / `Ctrl-i` move backward and forward through successful jumps. After going backward, make a new jump and confirm the former forward location is no longer reachable.
7. Run `\f` and `\g` from Code. Open a nested result and confirm Code returns directly, expands the path in the tree, and places the marker on the content-match line. Set lowercase and uppercase marks, jump with apostrophe and backtick, cross files, and traverse the combined history with counted `Ctrl-o` / `Ctrl-i`.
8. Open a binary, symbolic link, deleted tracked path, and file larger than 8 MiB. Confirm each displays a safe summary or truncation marker and no symbolic-link target is read.

## Search highlight dismissal

Record the version/revision, OS, terminal, dimensions, query, pane/float and keymap.
Compare before and after using the same file and operation sequence. If the
reported environment cannot be reproduced, record that limitation.

1. In both Diff and Code, test the ordinary pane and full float at 140×40 and
   80×24. Use `/needle`, `Enter`, `n`, `Esc`, then repeat with `?needle` and `N`.
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
