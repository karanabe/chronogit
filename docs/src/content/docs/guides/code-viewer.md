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

1. Move with `j` / `k`, the arrow keys, `gg` / `G`, or Home/End.
2. Press `Enter` on a directory to expand or collapse its direct children.
3. Moving onto a file loads its current working-tree content in the lower pane.
4. Use `l` or `Ctrl-j` to focus the code pane. Once code content is focused, `h` / `l` move its character cursor; use `Ctrl-k` to return to the tree.

Press `r` to rebuild the tree from Git's current tracked and non-ignored file list. Deleted tracked paths remain visible and show an unavailable-file summary when opened. ChronoGit identifies binary files and symbolic links without decoding a binary or following a link.

## Read a complete file

Press `Enter` on a file in the tree, or while the lower pane is focused, to open a nearly full-screen code window. The code window deliberately shares the floating-diff keymap:

- `j` / `k`, `gg` / `G`, and `Ctrl-d` / `Ctrl-u` move the current-line marker.
- `h` / `l` or Left / Right moves the semantic cursor by one UTF-8 character; tabs and wide or combining characters retain the correct source position.
- `zh` / `zl` scroll horizontally.
- `/` and `?` start smart-case forward and backward searches; `n` / `N` repeat them with wraparound.
- `Enter`, `q`, or `Esc` closes the window and returns to the two-pane Code view.

Recognized source types use embedded syntax definitions. Reads are bounded to 8 MiB, and the UI marks truncated text rather than growing without limit.

## Search while browsing code

`Space f` searches file paths and `Space g` searches fixed text from the Code viewer just as they do from Git screens. Opening a result returns directly to Code, expands the ancestors needed to reveal the selected file, and loads its current content. A content result positions the current-line marker at the matched line.

## Navigate by symbol with LSP

Language-server navigation is disabled by default. Enable one or more profiles only for a repository you trust:

```sh
chronogit --view code --lsp rust-analyzer
chronogit --view code --lsp jdtls
chronogit --view code --lsp pyright
chronogit --view code --lsp rust-analyzer --lsp jdtls --lsp pyright
```

ChronoGit does not install these executables. Follow the upstream installation instructions for [rust-analyzer](https://rust-analyzer.github.io/book/installation.html), [Eclipse JDT LS](https://github.com/eclipse-jdtls/eclipse.jdt.ls), [Pyright](https://github.com/microsoft/pyright), [basedpyright](https://docs.basedpyright.com/latest/installation/), or [Python LSP Server](https://github.com/python-lsp/python-lsp-server). Built-in profiles use `rust-analyzer`, the `jdtls` wrapper with isolated configuration/data directories, `pyright-langserver --stdio`, `basedpyright-langserver --stdio`, and `pylsp`; current JDT LS releases require Java 21 or newer to run. If more than one enabled profile claims an extension—such as two Python servers—navigation shows an ambiguity notice instead of choosing one.

Place the cursor on a symbol with `h` / `l` or Left / Right and use:

- `K`: open or close hover information
- `gd`: definition
- `gi`: implementation
- `gy`: type definition
- `gD`: declaration
- `Ctrl-o`: move to an older semantic location
- `Ctrl-i`: move to a newer semantic location

Hover opens in a floating window. Use `j` / `k` to scroll it, then press `K`, `q`, or `Esc` to close it. The initialized server's capabilities decide whether hover and each navigation operation are available. A server can temporarily return no information while it is indexing; close the float and invoke the operation again after indexing completes. A single navigation target opens directly. Multiple targets open a `j` / `k` selection list; `Enter` opens one and `q` / `Esc` closes the list. A new semantic jump after `Ctrl-o` discards the newer branch, matching Vim's jump-list behavior. In terminals that cannot distinguish `Ctrl-i` from Tab, Tab invokes the same newer-location action. No target, an unsupported capability, startup failure, timeout, or crash appears as a recoverable notice.

Only complete UTF-8 current-working-tree files participate. ChronoGit accepts only repository-contained `file:` results through its rooted no-follow reader. Standard-library/dependency paths outside the repository and virtual URIs such as `jdt:` are displayed as unsupported and are never interpreted as repository paths.

Profiles use the nearest configured root marker, falling back to the repository root. Sessions are keyed by profile and workspace root, start lazily, and retain at most four processes. A fifth workspace evicts the least recently used session. Rust, Java, Python, and user-defined languages all use the same standard request path.

## Place and load a language server

The language-server executable and ChronoGit's profile file are separate:

1. Install the server outside ChronoGit. Put the executable named by the profile on `PATH`, or override the profile with an absolute executable path in the trusted user-level `lsp.toml`.
2. Use a built-in profile ID, or place custom/overridden profile data at `$XDG_CONFIG_HOME/chronogit/lsp.toml` (normally `~/.config/chronogit/lsp.toml`). ChronoGit never reads a repository-local server command.
3. Start ChronoGit with one or more `--lsp PROFILE` options. Startup validates built-ins plus user overrides and retains only those explicitly selected, but starts no server yet.
4. Focus a matching source file and invoke hover or navigation. ChronoGit selects exactly one enabled profile by extension, finds the nearest root marker, starts the command with that workspace as its current directory, performs LSP `initialize`, synchronizes the displayed file with `didOpen`/`didChange`, checks the advertised capability, and sends the request.
5. Later requests reuse the session for the same `(profile, workspace root)`. ChronoGit shuts resident sessions down on exit; JDT LS's writable workspace data is isolated in a temporary directory outside the repository.

For example, a `rust-analyzer` binary on `PATH` needs no `lsp.toml`; `--lsp rust-analyzer` selects the built-in command. If a binary is elsewhere, copy the corresponding table from the packaged `config/lsp.toml` into the XDG file and replace the first `command` item with its absolute path. Java and Python follow exactly the same workflow with `jdtls`, `pyright-langserver`, `basedpyright-langserver`, or `pylsp`.
