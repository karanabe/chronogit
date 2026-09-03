---
title: Search files and content
description: Find a repository file or working-tree text, then inspect its history and current content.
tags:
  - search
  - files
  - history
sidebar:
  order: 4
---

Repository search opens from every main view. Press `Space f` to search tracked and untracked file paths, or `Space g` to search fixed text in non-binary working-tree files.

## Find and open a file

1. Enter a query. Results refresh after each inserted or deleted character. An empty file query lists all files after an edit or `Enter`; an empty content query returns no matches.
2. Press `Enter` or `Ctrl-j` to leave text entry and focus the current Results.
3. Move through results with `j` / `k`, `g` / `G`, or the arrow and Home/End keys.
4. Press `Ctrl-k` to return to Search and edit the current query again, or press `Enter` to open the selected result. A content result initially highlights its matching line. When search started in Code, the result returns directly to Code and reveals the file in its tree; other views open the file-history screen.

File-name search is smart-case: an all-lowercase query ignores case, while uppercase makes it case-sensitive. Content search is a literal, case-sensitive search and does not interpret a regular expression. It includes tracked and untracked files that are not ignored, skips binary content, and does not enter submodule repositories.

## Compare current content with history

The file view places up to 200 commits affecting the path in the upper pane and the syntax-highlighted current working-tree content in the lower pane. The current-line gutter marker does not replace token colors. Symlinks, binary files, deleted paths, and submodules show a summary instead of following or decoding the target.

Move the history selection with `j` / `k`; after the selection changes, the lower pane shows that commit's diff against its first parent, or against the empty tree for a root commit. Use `h` / `l` to move between panes. Press `Enter` to open whichever current content or diff is shown in a large floating view. Press `q` or `Esc` once to close a floating view and again to return to the view where search began. While a search prompt is active, `q` and `Q` are query text; `Esc` cancels the prompt and `Ctrl-C` quits.

All searches and file reads retain ChronoGit's read-only behavior, 8 MiB output/content bound, 30-second Git timeout, and two-read concurrency limit.
