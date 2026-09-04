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
5. Resize from at least 140×40 to approximately 90×24. Confirm multiple panes become one focused pane and `h`/`l` changes focus.
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
5. Select changed text and binary files, press `Enter`, and confirm a large floating patch or binary summary opens. Close it once with `q`, once with `Esc`, and once with `Enter`.
6. Open a recognized source file and confirm code tokens are syntax-highlighted, additions/removals retain muted green/red backgrounds, and the current-line gutter marker does not recolor the code. While opening an uncached long text diff, immediately press `Ctrl-d` and confirm the marker moves half a page as soon as the diff appears. Confirm `j` / `k` visibly move it one line and `Ctrl-u` moves it up without a delay.
7. Reach the first and last diff lines with `gg` / `G`. Search forward with `/`, backward with `?`, and confirm `n` / `N` wrap between highlighted matches.
8. Press `m`, scroll the complete commit-message overlay with `j` / `k`, and close it separately with `m`, `q`, and `Esc`.
9. Press `b` and confirm the rows are the same commit list, commit body, and changed files. Use `Ctrl-j` / `Ctrl-k` to move focus, change the top-row commit and confirm the other rows update, scroll the body, and open a bottom-row file diff. Press `b` again to return to standard History.
10. Press `t`, expand and collapse two directory levels, and open a blob diff.
11. Exit with `Ctrl-C`, then repeat the `stty` comparison and shell checks.

## Graph and repository search

1. Press `3`; confirm parent lanes and commit subjects are visible. Press `m` and close the complete message.
2. Press `Enter`; confirm a bordered two-row window floats over the still-visible Graph, with changed files above the selected diff. Press `Enter` for the full diff, use `q` to close it, then use `q` again to return to Graph. Repeat with `Esc`.
3. From Changes, History, and Graph, run `Space f` and type a known path one character at a time. Confirm results update before `Enter`, use `Enter` or `Ctrl-j` to focus Results, then use `Ctrl-k` to return to Search. Edit the query and confirm live results update again before opening it; confirm file history is above current content.
4. Change the history selection and confirm the lower pane becomes that commit's diff. Open and close the full diff, then press `q` or `Esc` back to the originating view.
5. Run `Space g`, type known text, confirm live results follow each edit and deletion, open a result, and confirm the matching current-content line is highlighted. Reopen the prompt, enter a query containing both `q` and uppercase `Q`, and confirm both are inserted and update results. Confirm `Esc` closes the prompt and `Ctrl-C` quits.
6. Start once with the default XDG keymap and once with `--keymap` pointing to a valid custom binding. Confirm an invalid explicit file fails before the alternate screen opens.

## Code workflow

1. Press `4`, then confirm a tracked root file and a collapsed nested directory appear above the code pane. Repeat by starting with `--view code`.
2. Move onto a file and confirm its current syntax-highlighted content loads below. Move rapidly between files and confirm the final content matches the final selection.
3. Press `Enter` on a directory, expand at least two levels, then press it again and confirm all descendants collapse.
4. Move between tree and code with both `h` / `l` and `Ctrl-k` / `Ctrl-j`. In the code pane use `j` / `k`, `gg` / `G`, `Ctrl-u` / `Ctrl-d`, and `zh` / `zl`.
5. Press `Enter` from both a tree file and the lower pane. Confirm the full Code window opens, `/` / `?` and `n` / `N` search with wraparound, and `Enter`, `q`, and `Esc` each return to Code.
6. With a language server enabled, move the character cursor onto a symbol. Confirm `K` opens and closes hover, `gd` / `gi` / `gy` / `gD` request the four semantic targets, and `Ctrl-o` / `Ctrl-i` move backward and forward through successful jumps. After going backward, make a new jump and confirm the former forward location is no longer reachable.
7. Run `Space f` and `Space g` from Code. Open a nested result and confirm Code returns directly, expands the path in the tree, and places the marker on the content-match line.
8. Open a binary, symbolic link, deleted tracked path, and file larger than 8 MiB. Confirm each displays a safe summary or truncation marker and no symbolic-link target is read.

## Sign-off

Do not mark a platform complete from automated tests alone.

| Platform | OS version | Terminal | Color/Unicode/resize | `Q` cleanup | Ctrl-C cleanup | Tester/date |
| --- | --- | --- | --- | --- | --- | --- |
| Linux | | | | | | |
| macOS | | | | | | |
