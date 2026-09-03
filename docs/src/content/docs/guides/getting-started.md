---
title: Get started
description: Install ChronoGit with Cargo and open your first repository.
tags:
  - installation
  - quickstart
  - terminal
sidebar:
  order: 1
---

ChronoGit is a read-only terminal interface for inspecting Git changes, history, and working-tree source code. This guide installs it with Cargo and opens a repository without changing it.

:::note[Distribution status]
The manifest is ready for crates.io publication. If `0.3.0` is not available from the registry yet, install it from a trusted checkout.
:::

## Requirements

- Linux or macOS
- Rust `1.88` or newer, including Cargo
- Git available on `PATH`
- An interactive terminal at least 80 columns by 24 rows
- A non-bare Git repository

Windows, bare repositories, pipes, captured commands, and background sessions are not supported in `0.3.0`.

## Install from crates.io

Install the published crate and use its locked dependency versions:

```sh title="Terminal"
cargo install chronogit --locked
```

## Install from a checkout

From the ChronoGit repository root, run:

```sh title="Terminal"
cargo install --path . --locked
```

Whichever installation method you use, confirm that the binary is available:

```sh title="Terminal"
chronogit --version
# chronogit 0.3.0
```

## Open a repository

Run ChronoGit inside a repository:

```sh title="Terminal"
cd /path/to/repository
chronogit
```

You can also pass the repository root or any directory below it:

```sh title="Terminal"
chronogit /path/to/repository/subdirectory
```

ChronoGit asks Git for the worktree root, so the interface always covers the complete repository rather than only the selected subdirectory. If `PATH` is omitted, it defaults to the current directory.

## Choose the initial view

The default **Changes** view shows unstaged work. Start directly in **History**, **Graph**, or the working-tree **Code** viewer when that better matches your task:

```sh title="Terminal"
chronogit /path/to/repository --view history
chronogit /path/to/repository --view graph
chronogit /path/to/repository --view code
```

Press `1` for Changes, `2` for History, `3` for Graph, or `4` for Code at any time. The first three form the Git workflow; Code provides the separate source-browsing workflow. `Space f` searches files and `Space g` searches working-tree text from any main view. Press `F1` for the in-app key guide, `q` / `Esc` to close or go back, and `Q` / `Ctrl-C` to exit.

## Next steps

- [Inspect unstaged changes](/guides/changes/)
- [Explore commit history, messages, and trees](/guides/history/)
- [Browse the working-tree source code](/guides/code-viewer/)
- [Search files, content, and per-file history](/guides/search/)
- [Learn the keyboard and responsive layout](/guides/navigation/)
- [Review the read-only contract and resource limits](/reference/safety-and-limits/)

If startup fails, use the [troubleshooting guide](/troubleshooting/common-problems/).
