---
title: Explore commit history
description: Inspect commit patches, complete messages, changed files, and trees.
tags:
  - history
  - commits
  - tree
sidebar:
  order: 3
---

History view combines a paged commit list with each commit's changed files, patch, complete message, and repository tree.

Open it with `chronogit --view history` or press `2`.

## Select a commit and file

History displays commits, changed files/tree, and the diff preview as three full-width rows from top to bottom.

1. Select a commit in the top row with `j` and `k`.
2. Press `Enter` to confirm the commit and focus Changed files. You can also move there with `l` or `Ctrl-j`.
3. Press `Enter` to open the complete patch in a large floating diff.
4. Move the highlighted current line with `j` / `k`, `Ctrl-d` / `Ctrl-u`, and `g` / `G`; the viewport follows it. These keys also take effect when entered while the diff is loading. Use `/` or `?` to search forward or backward, then `n` / `N` to move between matches.
5. Press `Enter` again to close the diff, or use `Esc`.

History loads 200 commits at a time. Moving to the end of the loaded page requests the next page when one exists. Changed-file lists and diffs load only for the selected commit.

## Understand the comparison

ChronoGit displays the active baseline in the changed-files title, diff title, and footer.

| Commit kind | Comparison |
| --- | --- |
| Root commit | Empty tree → selected commit |
| Normal commit | Parent → selected commit |
| Merge commit | First parent → selected merge commit |

Merge commits are not shown as a combined or per-parent diff in `0.1.0`. When another parent matters, use Git alongside ChronoGit.

## Read the complete commit message

Press `m` to open the selected commit's complete message in a floating overlay. Navigation keys scroll the message. Press `m` again or `Esc` to close it.

For a persistent body-oriented layout, press `b`. Its three rows are the same interactive commit list, the selected commit's body (including trailers), and changed files. Use `h` / `l` or `Ctrl-k` / `Ctrl-j` to move focus. Selecting another commit in the top row refreshes both the body and file rows. Focus the bottom row and press `Enter` to open a file diff; use `j` / `k` inside the diff. Press `b` again to return to standard History.

## Follow parent lanes in Graph

Press `3` or start with `chronogit --view graph` to see the same paged commits with parent lanes. `j` / `k` changes the commit, and `m` opens its complete message. Press `Enter` for a focused two-row view of that commit's changed files and selected-file diff. Press `Enter` again for the full floating diff, or `Esc` to return to Graph.

## Browse the commit tree

Press `t` to replace the changed-file list with the selected commit's complete tree.

- Select a directory and press `Enter` to expand or collapse it.
- Select a file and press `Enter` to open its patch for the active commit comparison.
- An unchanged file remains selectable but reports that it has no change in the selected commit.
- Symlinks and submodules are identified by their Git tree modes.
- ChronoGit does not enter a submodule repository.

Tree directories load lazily. Returning to an already expanded directory reuses its entries for the selected commit.

Press `t` again to return to changed files. Changing commits resets the tree to the new commit's root.
