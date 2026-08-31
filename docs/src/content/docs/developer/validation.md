---
title: Validation strategy and 0.1.0 record
description: Run ChronoGit's automated gates and understand the release-candidate evidence.
tags:
  - testing
  - validation
  - release
sidebar:
  order: 2
---

Use the narrowest relevant test while developing, then run the complete gate for Rust changes. Rendering, key handling, terminal cleanup, and platform claims also require a [real-terminal smoke test](/developer/terminal-smoke/).

## Automated quality gate

From the repository root:

```sh title="Terminal"
cargo fmt --all --check
cargo clippy --all-targets --all-features --tests --benches -- -D warnings
cargo test --all-features
cargo build --release --locked
cargo package --locked
```

For documentation changes, build the site separately:

```sh title="Terminal"
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs build
```

Focused Rust commands include `cargo test --lib`, `cargo test --test git_service`, and `cargo test --test cli`. Do not treat a focused pass as evidence for the full release surface.

## Coverage model

- Domain and parser unit tests cover value invariants, byte parsing, line numbers, and malformed output.
- Reducer and effect tests cover every History panel, commit-to-Changed-files activation, full-message overlays, body-layout transitions/selection/scrolling, pagination, tree expansion, floating diff open/scroll/search/close transitions, navigation entered while a diff loads, message loading, stale-response rejection, and rapid diff coalescing.
- Render and key tests cover loading, empty, failure, truncation, minimum-width three-row History layouts, message overlays, highlighted diff navigation, full-diff scrolling, comparison labels, `Ctrl-j`/`Ctrl-k`, `m`/`b`, search/help keys, semantic colors, and key sequences.
- Temporary-repository integration tests cover root and merge commits, detached and unborn repositories, linked worktrees, staged-only and mixed changes, conflicts, renames, deletes, type changes, symlinks, submodules, binary and oversized diffs, and non-UTF-8 or leading-dash paths.
- CLI tests cover help/version, non-repository diagnostics, missing Git, permission errors, and non-TTY rejection.
- Read-only integration checks compare `HEAD`, porcelain status, and worktree bytes before and after Git service operations.
- Terminal lifecycle tests inspect restoration sequences and an intentionally panicking child process.

## Current change verification

For the current working tree, the complete Rust gate above passed with 77 tests across unit, CLI, and Git-service targets, followed by the locked release build. `cargo package --allow-dirty --locked` produced and rebuilt a 40-file crate (271.4 KiB, 58.1 KiB compressed), and the crates.io publish dry-run reached the upload step before aborting as expected. The documentation build generated 29 pages successfully. Repeat the commands without `--allow-dirty` from the exact committed release revision.

The release binary was also exercised against the separate coreutils repository in a real 80×24 PTY. In standard History, `Enter` on Commits moved focus to Changed files and a second `Enter` opened the selected diff. `j` and `k` visibly moved its highlighted current line. On another uncached file, `Ctrl-d` was entered while `Loading diff…` was visible; the completed diff immediately highlighted the line ten positions down, and `Ctrl-u` returned it to the first line. `q` exited successfully with the terminal restoration sequences present. The complete-message and body-layout checks from the preceding revision remain recorded below.

## Historical `0.1.0` release-candidate record

:::note[Recorded evidence]
The remainder of this page preserves checks performed for the `0.1.0` release candidate. It is not a substitute for rerunning the gates on a later revision.
:::

On Linux with the declared MSRV, `rustc 1.88.0`, the format, Clippy, all-feature test, and release-build gates passed using the locked dependencies.

After the license files were added, `cargo package --allow-dirty --locked` packaged 49 files (284.2 KiB, 72.2 KiB compressed) and rebuilt the generated package with Rust 1.97.1. Cargo's missing-repository warning remained an intentional release blocker. The Linux archive procedure was exercised in a temporary staging directory; its executable, README, changelog, and license files passed `sha256sum -c`.

`cargo install --path . --locked` succeeded. An installed binary was exercised from a subdirectory of a separate large repository in a real PTY. Changes, History, changed files, commit message, commit tree, help, focused-pane layout, `q`, and Ctrl-C were checked. Both exit paths returned success and emitted cursor, mouse-mode, raw-mode, and alternate-screen restoration sequences.

## Recorded performance and limits

On the separate coreutils repository, the first 200 history entries appeared within the first one-second observation interval. A PTY run including navigation and exit took 3.40 seconds total and used approximately 6 MiB maximum resident memory.

The retained implementation limits were 75 ms diff debounce, two concurrent Git reads, 200-commit pages, a 16-entry/16 MiB diff cache, 8 MiB stdout, 64 KiB stderr, and a 30-second Git command duration.

## Recorded dependency and source audit

`cargo-audit 0.22.2` scanned 118 locked packages against 1,226 RustSec advisories and reported no vulnerabilities. `cargo-deny 0.16.3` reported `bans ok` and `sources ok`; its advisory mode was superseded by `cargo-audit` because that deny version could not parse CVSS 4.0 records. Duplicate `hashbrown` and `syn` families came from Ratatui's dependency graph rather than duplicate direct dependencies.

Static searches found no secrets, `unsafe`, `TODO`/`FIXME`, debug macros, direct `anyhow`/`thiserror`, or application-path `unwrap`/`expect`. The only production subprocess constructor was the shell-free typed Git allowlist in `src/git/runner.rs`.

## Platform and release status

The GitHub Actions `CI` workflow is manual-only and does not run for pushes or pull requests. When a remote check is needed, a maintainer opens the repository's **Actions** tab, selects **CI**, and chooses **Run workflow**. That run executes tests and release builds on Ubuntu and macOS, with formatting and Clippy on Ubuntu. Linux real-terminal behavior was verified locally. The macOS row in the manual checklist still requires a real terminal in the CI or release environment.

The manifest permits publication only to crates.io and limits the registry payload to application and test sources plus user-facing release files. Publishing still requires an authorized maintainer to complete the current checks and explicit publish step in the [release procedure](/developer/release/).
