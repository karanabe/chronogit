<p align="center">
  <img src="https://raw.githubusercontent.com/karanabe/chronogit/master/docs/src/assets/ChronoGitLogo.png" alt="ChronoGit" width="520" />
</p>

<h1 align="center">ChronoGit</h1>
<h3 align="center">A terminal UI for exploring Git history, diffs, and source code.</h3>

ChronoGit brings unstaged changes, commit history and graphs, repository search,
and syntax-highlighted source code into one Vim-oriented terminal interface. Browsing reads your repository. Use `Space b` from Changes, History, Graph, or
Code to switch an existing local branch; this updates HEAD, the index, and the
working tree while preserving local changes or reporting conflicts. Select with
`j`/`k`, press `Enter` to switch, or `q`/`Esc` to cancel.

## Quick start

### Requirements

- Linux or macOS
- Rust 1.88 or newer
- Git available on `PATH`
- An interactive terminal at least 80 columns by 24 rows

Install the published release with Cargo:

```bash
cargo install chronogit --locked
```

Then open a Git repository:

```bash
chronogit /path/to/repository
```

When run inside a repository, the path can be omitted:

```bash
chronogit
```

ChronoGit starts in the **Changes** view. Press `F1` for the complete key guide
for your installed version, use `j` / `k` to move, `Enter` to open an item, `q`
to go back, and `Q` or `Ctrl-C` to quit.

To start in another view:

```bash
chronogit --view history
chronogit --view graph
chronogit --view code
```

To install the current checkout instead of the published release, run this from
the repository root:

```bash
cargo install --path . --locked
```

See the [getting-started guide](docs/src/content/docs/guides/getting-started.md)
for platform support, upgrade notes, and the complete first-run walkthrough.

## What you can explore

- **Changes:** inspect tracked and untracked unstaged work.
- **History:** read commits, full messages, changed files, trees, and patches.
- **Graph:** follow commit parent relationships across branch lanes.
- **Code:** browse the working tree and read syntax-highlighted files.
- **Search:** find repository files or fixed text, then inspect the matching
  file and its history.

Optional language-server profiles add hover, symbol, and definition navigation
for trusted repositories. Language servers are never downloaded or started
unless you explicitly enable them.

## Documentation

- [Getting started](docs/src/content/docs/guides/getting-started.md)
- [Changes](docs/src/content/docs/guides/changes.md),
  [history](docs/src/content/docs/guides/history.md),
  [code browsing](docs/src/content/docs/guides/code-viewer.md), and
  [search](docs/src/content/docs/guides/search.md)
- [Navigation and layout](docs/src/content/docs/guides/navigation.md)
- [CLI reference](docs/src/content/docs/reference/cli.md) and
  [keymap configuration](docs/src/content/docs/reference/keymap.md)
- [Safety, limits, and non-goals](docs/src/content/docs/reference/safety-and-limits.md)
- [Troubleshooting](docs/src/content/docs/troubleshooting/common-problems.md)
- [Using ChronoGit alongside coding agents](docs/src/content/docs/guides/agents.md)

Japanese documentation is available under
[`docs/src/content/docs/ja/`](docs/src/content/docs/ja/).

## `vim-navigation`

This workspace also contains `vim-navigation`, a terminal-framework-independent
Rust library for Vim-compatible motion and modal text-input state. ChronoGit
users do not need to configure it. Library users should start with the
[`vim-navigation` README](crates/vim-navigation/README.md) and its
[compatibility contract](crates/vim-navigation/COMPATIBILITY.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the contributor workflow and
[DEVELOPMENT.md](DEVELOPMENT.md) for architecture and module boundaries.

### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version 2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>
