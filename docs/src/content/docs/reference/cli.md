---
title: CLI reference
description: ChronoGit command syntax, arguments, options, and startup behavior.
tags:
  - cli
  - reference
sidebar:
  order: 1
---

## Synopsis

```text
chronogit [OPTIONS] [PATH]
```

## Arguments and options

| Input | Default | Meaning |
| --- | --- | --- |
| `[PATH]` | `.` | Repository root or any directory below it |
| `--view changes\|history\|graph\|code` | `changes` | View to open first |
| `--keymap PATH` | XDG path when present | Explicit keymap configuration file |
| `--lsp PROFILE` | disabled | Enable one trusted external language-server profile; repeatable |
| `--lsp-config PATH` | XDG path when present | Explicit trusted user-level LSP profile file |
| `-h`, `--help` | — | Print help and exit |
| `-V`, `--version` | — | Print the version and exit |

`PATH` is resolved before the TUI starts. It must exist, be a directory, and belong to a non-bare Git worktree. ChronoGit displays the repository root discovered by Git, even when `PATH` names a nested directory or linked worktree.

## Examples

```sh
# Current repository, Changes first
chronogit

# Explicit repository, History first
chronogit /srv/project --view history

# Graph first with a project-specific keymap
chronogit /srv/project --view graph --keymap ./keymap.conf

# Working-tree source browser first
chronogit /srv/project --view code

# Rust semantic navigation
chronogit /srv/project --view code --lsp rust-analyzer

# Polyglot Rust, Java, and Python navigation
chronogit /srv/project --view code \
  --lsp rust-analyzer --lsp jdtls --lsp pyright

# Help and version do not require an interactive TTY
chronogit --help
chronogit --version
```

## Exit behavior

Successful help, version output, `Q`, and `Ctrl-C` return success. Repository, keymap, and terminal startup failures print a `chronogit:` diagnostic and any available cause chain to standard error, then return failure. An explicit `--keymap` file must exist and be valid; an absent default XDG file simply uses built-in bindings.

Repository-provided control characters in diagnostics are escaped before printing. Recoverable Git failures after startup appear inside the affected pane or footer instead of terminating the application.

`--lsp` is explicit project trust and never downloads a server. Built-in IDs are `rust-analyzer`, `jdtls`, `pyright`, `basedpyright`, and `pylsp`. Enabling two profiles for the same extension is allowed at startup but navigation refuses the ambiguous match. An explicit `--lsp-config` must exist and pass schema and command validation; the implicit path is `$XDG_CONFIG_HOME/chronogit/lsp.toml`, falling back to `~/.config/chronogit/lsp.toml`.

ChronoGit requires interactive standard input and output after repository discovery. It does not read commands from stdin or emit a stable machine-readable representation.
