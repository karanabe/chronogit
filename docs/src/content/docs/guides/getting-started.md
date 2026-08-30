---
title: Get started
description: Install ChronoGit from a checkout and open your first repository.
tags:
  - installation
  - quickstart
  - terminal
sidebar:
  order: 1
---

ChronoGit is a read-only terminal interface for inspecting unstaged worktree changes and commit history. This guide installs version `0.1.0` from a source checkout and opens a repository without changing it.

:::note[Distribution status]
Cargo registry publishing is disabled for `0.1.0`. Install from a trusted checkout until the project documents a public distribution channel.
:::

## Requirements

- Linux or macOS
- Rust `1.88` or newer, including Cargo
- Git available on `PATH`
- An interactive terminal at least 80 columns by 24 rows
- A non-bare Git repository

Windows, bare repositories, pipes, captured commands, and background sessions are not supported in `0.1.0`.

## Install from a checkout

From the ChronoGit repository root, run:

```sh title="Terminal"
cargo install --path . --locked
```

Confirm that the installed binary is available:

```sh title="Terminal"
chronogit --version
# chronogit 0.1.0
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

The default **Changes** view shows unstaged work. Start directly in **History** when you want to inspect commits:

```sh title="Terminal"
chronogit /path/to/repository --view history
```

Press `1` for Changes or `2` for History at any time. Press `F1` for the in-app key guide and `q` or `Ctrl-C` to exit.

## Next steps

- [Inspect unstaged changes](/guides/changes/)
- [Explore commit history, messages, and trees](/guides/history/)
- [Learn the keyboard and responsive layout](/guides/navigation/)
- [Review the read-only contract and resource limits](/reference/safety-and-limits/)

If startup fails, use the [troubleshooting guide](/troubleshooting/common-problems/).
