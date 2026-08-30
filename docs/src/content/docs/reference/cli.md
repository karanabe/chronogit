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
| `--view changes\|history` | `changes` | View to open first |
| `-h`, `--help` | — | Print help and exit |
| `-V`, `--version` | — | Print the version and exit |

`PATH` is resolved before the TUI starts. It must exist, be a directory, and belong to a non-bare Git worktree. ChronoGit displays the repository root discovered by Git, even when `PATH` names a nested directory or linked worktree.

## Examples

```sh
# Current repository, Changes first
chronogit

# Explicit repository, History first
chronogit /srv/project --view history

# Help and version do not require an interactive TTY
chronogit --help
chronogit --version
```

## Exit behavior

Successful help, version output, `q`, and `Ctrl-C` return success. Startup and terminal failures print a `chronogit:` diagnostic and any available cause chain to standard error, then return failure.

Repository-provided control characters in diagnostics are escaped before printing. Recoverable Git failures after startup appear inside the affected pane or footer instead of terminating the application.

ChronoGit requires interactive standard input and output after repository discovery. It does not read commands from stdin or emit a stable machine-readable representation.
