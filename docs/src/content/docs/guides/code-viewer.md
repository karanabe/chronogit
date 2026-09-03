---
title: Browse source code
description: Explore the working-tree file hierarchy and read complete source files.
tags:
  - code
  - files
  - navigation
sidebar:
  order: 4
---

ChronoGit separates its existing Git review screens from a working-tree Code viewer. The default launch still opens Changes. Press `4`, or start with `chronogit --view code`, when you want to browse the repository rather than a patch or commit.

## Navigate the file tree

The upper pane contains tracked files and non-ignored untracked files. At every level, directories appear first in name order, followed by files in name order. Directories start collapsed so a large repository remains easy to scan.

1. Move with `j` / `k`, the arrow keys, `g` / `G`, or Home/End.
2. Press `Enter` on a directory to expand or collapse its direct children.
3. Moving onto a file loads its current working-tree content in the lower pane.
4. Use `l` or `Ctrl-j` to focus the code pane, and `h` or `Ctrl-k` to return to the tree.

Press `r` to rebuild the tree from Git's current tracked and non-ignored file list. Deleted tracked paths remain visible and show an unavailable-file summary when opened. ChronoGit identifies binary files and symbolic links without decoding a binary or following a link.

## Read a complete file

Press `Enter` on a file in the tree, or while the lower pane is focused, to open a nearly full-screen code window. The code window deliberately shares the floating-diff keymap:

- `j` / `k`, `g` / `G`, and `Ctrl-d` / `Ctrl-u` move the current-line marker.
- `zh` / `zl` scroll horizontally.
- `/` and `?` start smart-case forward and backward searches; `n` / `N` repeat them with wraparound.
- `Enter`, `q`, or `Esc` closes the window and returns to the two-pane Code view.

Recognized source types use embedded syntax definitions. Reads are bounded to 8 MiB, and the UI marks truncated text rather than growing without limit.

## Search while browsing code

`Space f` searches file paths and `Space g` searches fixed text from the Code viewer just as they do from Git screens. Opening a result returns directly to Code, expands the ancestors needed to reveal the selected file, and loads its current content. A content result positions the current-line marker at the matched line.

## Semantic navigation status

Definition, implementation, type-definition, and declaration jumps are feasible, but are not implemented by the current file reader. They require a Language Server Protocol client because Git and syntax highlighting do not provide symbol resolution.

A future implementation should keep LSP transport outside the domain model, detect a language server from project configuration, synchronize `didOpen`/`didChange` documents, translate the current byte/character position using the server's negotiated encoding, issue `textDocument/definition`, `implementation`, `typeDefinition`, or `declaration`, and show one or multiple returned locations through typed application effects. It also needs server lifecycle, timeout, cancellation, stale-response, multi-root, unsupported-language, and external-location handling. Until those boundaries and fallbacks exist, Code remains a read-only text viewer rather than an editor or IDE.
