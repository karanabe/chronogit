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
| `--config PATH` | XDG path when present | Display settings file |
| `--scrolloff LINES` | Config value, otherwise `2` | Context rows above/below the cursor; `0` disables. Overrides the config file |
| `--keymap PATH` | XDG path when present | Explicit keymap configuration file |
| `--lsp PROFILE` | disabled | Enable one trusted external language-server profile; repeatable |
| `--lsp-config PATH` | XDG path when present | Explicit trusted user-level LSP profile file |
| `-h`, `--help` | — | Print help and exit |
| `-V`, `--version` | — | Print the version and exit |

`PATH` is resolved before the TUI starts. It must exist, be a directory, and belong to a non-bare Git worktree. ChronoGit displays the repository root discovered by Git, even when `PATH` names a nested directory or linked worktree.

## Display settings

ChronoGit reads `$XDG_CONFIG_HOME/chronogit/config.toml`, falling back to `~/.config/chronogit/config.toml` when XDG is unset. Use `--config PATH` to select another file.

```toml
scrolloff = 2
```

`scrolloff` is a nonnegative integer specifying context rows above and below the cursor in file and commit lists, diffs, source code, and other text panes. Short panes reduce the margin to leave room for the cursor; file boundaries show the available context. Run `chronogit --scrolloff 3` to override it for one invocation.

An absent default file uses built-in settings. A missing explicit file, malformed TOML, unknown setting, negative value, or non-integer value fails before terminal initialization. Key bindings remain in `keymap.conf`.

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

# Rust semantic navigation and document symbols
chronogit /srv/project --view code --lsp rust-analyzer

# Polyglot Rust, Java, and Python semantic features
chronogit /srv/project --view code \
  --lsp rust-analyzer --lsp jdtls --lsp pyright

# Help and version do not require an interactive TTY
chronogit --help
chronogit --version
```

## Exit behavior

Successful help, version output, `Q`, and `Ctrl-C` return success. Repository, keymap, and terminal startup failures print a `chronogit:` diagnostic and any available cause chain to standard error, then return failure. An explicit `--keymap` file must exist and be valid; an absent default XDG file simply uses built-in bindings.

Repository-provided control characters in diagnostics are escaped before printing. Recoverable Git failures after startup appear inside the affected pane or footer instead of terminating the application.

`--lsp` is explicit project trust and never downloads a server. Built-in IDs are `rust-analyzer`, `jdtls`, `pyright`, `basedpyright`, and `pylsp`. Enabling two profiles for the same extension is allowed at startup, but semantic navigation and document-symbol requests refuse the ambiguous match. An explicit `--lsp-config` must exist and pass schema and command validation; the implicit path is `$XDG_CONFIG_HOME/chronogit/lsp.toml`, falling back to `~/.config/chronogit/lsp.toml`.

ChronoGit requires interactive standard input and output after repository discovery. It does not read commands from stdin or emit a stable machine-readable representation.
