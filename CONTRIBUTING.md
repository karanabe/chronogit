# Contribute To ChronoGit

This document owns the contribution workflow: how to prepare a change, what to
run before committing, and how to keep pull requests reviewable. For
implementation architecture and module boundaries, read
[`DEVELOPMENT.md`](DEVELOPMENT.md).

## Requirements

- Rust `1.88` or newer
- Git available on `PATH`
- An interactive terminal of at least 80x24 for manual TUI checks

Check your local toolchain:

```bash
rustc --version
cargo --version
git --version
```

Run commands from the repository root unless a command says otherwise.

## Before You Start

- Read [`README.md`](README.md) for the current user-facing scope.
- Read [`DEVELOPMENT.md`](DEVELOPMENT.md) and the
  [architecture guide](docs/src/content/docs/developer/architecture.md) before changing domain types, Git command
  policy, state transitions, terminal lifecycle, or platform behavior.
- Keep changes focused on one behavior, bug, or documentation topic.
- Update documentation and tests in the same change when behavior changes.
- Preserve the read-only Git contract and bounded-resource invariants.

## Change Flow

1. Create a focused branch, for example
   `git switch -c docs/contribution-flow`.
2. Make the smallest coherent code or documentation change.
3. Add or update tests and documentation for behavior changes.
4. Run the relevant local checks, then the full pre-commit checks before
   committing Rust changes.
5. Commit the staged change and open a pull request with the checks listed.

## Local Workflow

Start with the narrowest command that exercises the area you changed:

```bash
cargo test --lib
cargo test --test git_service
cargo test --test cli
```

Use the full feature set when a change affects shared behavior or multiple
layers:

```bash
cargo test --all-features
cargo build --release
```

Changes to rendering, key handling, terminal cleanup, or platform behavior also
need the relevant steps from
[manual terminal smoke test](docs/src/content/docs/developer/terminal-smoke.md). Automated tests
do not replace a real terminal check for release sign-off.

## Before Commit

For Rust changes, run the full verification set:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features --tests --benches -- -D warnings
cargo test --all-features
cargo build --release
```

For documentation-only changes, Rust build and test commands are optional
unless source code, generated code, compiled examples, commands, or expected
output changed.

## Testing Expectations

- Add unit tests beside domain rules, parsers, reducers, key mapping, and
  rendering logic where proximity makes failures easiest to understand.
- Add integration tests for real Git repository behavior and public CLI
  behavior.
- Cover root, normal, merge, unborn, detached, and linked-worktree cases when a
  change affects repository history or discovery.
- Cover stale asynchronous responses and rapid selection changes when a change
  affects loading or caching.
- Preserve tests that compare repository state before and after Git service
  operations.
- Exercise terminal restoration and the minimum-size layout when changing
  terminal lifecycle or rendering.

## Documentation Expectations

- Update [`README.md`](README.md) for user-visible commands, keys, behavior,
  errors, or limitations.
- Update [`DEVELOPMENT.md`](DEVELOPMENT.md) and the
  [architecture guide](docs/src/content/docs/developer/architecture.md) when module ownership, design constraints,
  state flow, Git policy, or resource limits change.
- Update the [manual terminal smoke test](docs/src/content/docs/developer/terminal-smoke.md) and
  [release procedure](docs/src/content/docs/developer/release.md) when validation or release procedures
  change.
- Keep examples aligned with the actual CLI and supported platforms.

## Commit Guidelines

- Check `git status --short` before staging so unrelated files stay out of the
  commit.
- Prefer `git add -p` or explicit paths when staging a mixed working tree.
- Review staged changes with `git diff --cached`.
- Coding agents create unsigned commits with `git commit`. They use
  `git commit --no-gpg-sign` when local Git configuration signs commits by
  default. Human maintainers sign or re-sign those commits with GPG or SSH
  signing before publishing.
- Write a short imperative subject line that names the area changed, for
  example `docs: add contributor and developer guides`.
- Keep commits focused. Do not commit `target/` or other local build output.

Example commit flow:

```bash
git status --short
git add -p
git diff --cached
git commit -m "docs: clarify contribution workflow"
```

Sign the latest commit after reviewing it:

```bash
git commit --amend --no-edit -S
```

Re-signing rewrites commit IDs, so update remote branches with
`git push --force-with-lease` when needed.

## Pull Request Checklist

Before requesting review, confirm:

- The change is scoped and avoids unrelated refactors.
- Required checks from the "Before Commit" section pass, or the pull request
  explains why a check was not run.
- Documentation was updated for changed user-facing or developer-facing
  behavior.
- New behavior has tests at the narrowest useful level.
- Changes to Git execution, path handling, terminal cleanup, process limits, or
  platform support call out their safety and compatibility impact.

## Security And Safety

ChronoGit opens repositories and renders data that may be malformed or
untrusted. Preserve shell-free command execution, validated object IDs,
separate path arguments, disabled external Git helpers, bounded subprocess
output, and reliable terminal cleanup. Do not add a mutating Git command or
silently weaken these boundaries.
