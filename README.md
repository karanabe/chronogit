<br />
<p align="center">
  <img src="https://raw.githubusercontent.com/karanabe/chronogit/master/docs/src/assets/ChronoGitLogo.png" alt="ChronoGit" width="520" />
</p>
<h1 align="center">ChronoGit</h1>
<h3 align="center">A read-only terminal UI for exploring Git history, diffs, and source code.</h3>
<br />
<br />

ChronoGit is a read-only terminal UI with two complementary workflows: Git history and a working-tree code viewer. It keeps unstaged changes, commit history and graph, repository search, full commit messages, commit trees, and syntax-highlighted source in one Vim-oriented interface.

## Status

ChronoGit is at `0.3.0`. Linux and macOS are supported. Windows, bare repositories, and non-interactive terminals are not supported yet.

The manifest is prepared to publish this release to crates.io. Publishing remains a separate maintainer action.

## Requirements

- Rust 1.88 or newer
- Git available on `PATH`
- An interactive terminal of at least 80x24

## Install

Install the published crate from crates.io:

```bash
cargo install chronogit --locked
```

Or install from a checkout:

```bash
cargo install --path . --locked
```

The `chronogit` binary can then be launched from any directory:

```bash
chronogit [PATH] [--view changes|history|graph|code] [--keymap PATH]
```

`PATH` may be a repository root or any directory below it. It defaults to the current directory. `--view` defaults to `changes`, preserving the existing landing workflow; `--keymap` overrides the optional XDG keymap path.

## Workflows

### Read unstaged changes

Start in Changes view with `chronogit`. The left pane contains tracked and untracked worktree changes. Select a file to see its `index → working tree` diff. Recognized source files use syntax highlighting; additions and removals retain muted diff backgrounds without replacing token colors.

Staged-only files are intentionally hidden. A file with both staged and unstaged edits shows only the unstaged part.

### Read commit history

Press `2`, or start with `chronogit --view history`. History uses three full-width rows—commits, changed files/tree, then diff—so commit subjects and paths retain the terminal width. Select a commit and press `Enter` to focus Changed files, select a file, then press `Enter` to open its patch in a large floating diff.

- A root commit is compared with the empty tree.
- A normal commit is compared with its parent.
- A merge commit is compared with its first parent.

The active comparison is shown in both the diff pane title and the footer.

### Read a commit message or tree

- Press `m` to open the selected commit's complete message in a floating overlay. Press `m` again, `q`, or `Esc` to close it.
- Press `b` to switch History to a three-row body layout: the same commit list, commit body, and changed files. Press `b` again to return to the diff layout.
- Press `t` to switch between changed files and the selected commit's tree.
- Press `Enter` to expand a directory or open a selected file in the floating diff. An unchanged tree file reports that it has no change in the selected commit.
- In the floating diff, use `j` / `k` to move the current-line marker down / up and `Ctrl-d` / `Ctrl-u` to move half a page. The marker stays in the gutter instead of recoloring the code row. Navigation entered while the diff is loading is applied as soon as it appears. Use `/` for forward search, `?` for backward search, and `n` / `N` for the next / previous match. Search wraps at the ends; lowercase queries ignore case, while a query containing uppercase is case-sensitive. Press `Enter` again, `q`, or `Esc` to close the diff.

Symlinks and submodules are identified in the tree. ChronoGit does not enter a submodule repository.

### Follow the Git graph

Press `3`, or start with `chronogit --view graph`. The graph uses commit parent relationships to display active branch lanes. `m` opens the selected commit message. `Enter` opens a floating two-row detail window over the graph, with changed files above the selected file's diff; another `Enter` opens the complete diff. Press `q` or `Esc` to return one level at a time.

### Browse the complete working tree

Press `4`, or start with `chronogit --view code`, to enter the Code viewer. The upper pane is an expandable tree containing tracked files and non-ignored untracked files; the lower pane previews the selected file with line numbers and syntax highlighting. Press `Enter` on a directory to expand or collapse it. Use `h` / `l` or `Ctrl-k` / `Ctrl-j` to move between the tree and code panes.

Press `Enter` on a file in the tree, or from the code pane, to open the current content in a large floating view. It uses the same navigation and search keys as a floating diff: `j` / `k`, `g` / `G`, `Ctrl-u` / `Ctrl-d`, `zh` / `zl`, `/` / `?`, and `n` / `N`. Press `Enter`, `q`, or `Esc` to return. `Space f` and `Space g` search from the Code viewer; selecting a result reveals the file in the tree and opens its current content at the matching line when available.

### Search files or working-tree text

Press `Space f` from any main view to find tracked and untracked file names. Press `Space g` for a fixed-text search across non-binary working-tree content. Results update after every inserted or deleted query character. Press `Enter` or `Ctrl-j` to focus Results, choose a result with `j` / `k`, and press `Enter` again to open it. Press `Ctrl-k` from Results to return to Search, edit the current query, and run another live search.

While entering a search query, `q` and `Q` are ordinary query characters. Use `Esc` to cancel the prompt and `Ctrl-C` to quit.

The file view shows its commit history above its current working-tree content. Changing the selected history commit replaces the lower pane with that commit's first-parent diff. `Enter` opens the current content or diff full-screen; `q` or `Esc` closes the float and then returns to the originating view.

## Keys

| Key | Action |
|---|---|
| `q` / `Esc` | Close the current float or go back |
| `Q` / `Ctrl-C` | Quit |
| `1` / `2` / `3` | Git workflow: Changes / History / Graph |
| `4` | Code viewer |
| `Space f` / `Space g` | Search repository files / working-tree text |
| `h` / `l` | Focus the previous / next pane |
| `Ctrl-k` / `Ctrl-j` | Focus the previous / next pane |
| `j` / `k` | Move or scroll down / up |
| `g` / `G` | Move to first / last item |
| `Ctrl-d` / `Ctrl-u` | Move or scroll half a page |
| `zh` / `zl` | Scroll a diff or code document horizontally |
| `r` | Refresh the current view |
| `m` | Toggle the full commit-message overlay |
| `b` | Toggle History's diff / body layout |
| `t` | Toggle changed files / commit tree |
| `Enter` | Select/open the current item, or close a floating full view |
| `/` / `?` | Search a floating diff or code document forward / backward |
| `n` / `N` | Go to the next / previous search match |
| `F1` | Toggle in-app help |

History always stacks its three panes vertically at the supported terminal sizes. In Changes, widths below 110 columns show the focused pane at full width; use `h` and `l` to move between panes.

## Keymap configuration

ChronoGit loads `$XDG_CONFIG_HOME/chronogit/keymap.conf`, falling back to `~/.config/chronogit/keymap.conf`, when that file exists. Copy [`config/keymap.conf`](config/keymap.conf) and uncomment only the actions you want to replace, or pass another file with `--keymap PATH`. For example:

```ini
[bindings]
show_graph = x
show_code = c
file_search = ctrl-p
content_search = space s
close = q, esc
quit = Q
```

Key sequences are space-separated and alternatives are comma-separated. Invalid, duplicate, or ambiguous bindings fail before raw terminal mode starts. `Ctrl-C` always remains available for safe exit. The complete action and key syntax is in the [keymap reference](docs/src/content/docs/reference/keymap.md).

## Read-only and failure behavior

ChronoGit only invokes an allowlisted set of Git read commands. It never stages, restores, commits, checks out, resets, or updates references. Commands are executed without a shell, paths are passed after `--`, optional Git locks are disabled, and external diff, textconv, pager, and fsmonitor programs are disabled. Code-viewer file reads remain rooted at the discovered worktree and do not follow symbolic links.

Git output is bounded. A text diff larger than 8 MiB is terminated and displayed as truncated instead of growing memory without limit; current file reads are also capped at 8 MiB. A Git command that runs longer than 30 seconds is terminated with a recoverable error. Binary changes and files are shown as a summary.

Startup errors are printed before raw terminal mode is enabled. During the TUI, recoverable Git errors are shown in the affected pane. Normal exit, errors, Ctrl-C, and panics restore the alternate screen, cursor, mouse capture, and raw mode.

## Non-goals

ChronoGit does not stage, restore, commit, reset, check out, or otherwise mutate a repository. It also does not provide staged-change, remote, pull-request, blame, stash, editor, plugin, or machine-readable export features. The Code viewer does not yet provide language-semantic definition, implementation, type-definition, or declaration jumps; those require a future Language Server Protocol integration.

## Troubleshooting

- `an interactive TTY is required`: run `chronogit` directly in a terminal, not in a pipe, background task, or captured command.
- `repository path is not a directory` or repository discovery fails: pass an existing non-bare Git repository or a directory below it.
- `Terminal too small`: resize to at least 80 columns by 24 rows. `Q` and `Ctrl-C` still quit safely.
- A pane shows a Git error: correct the repository or permission problem, then press `r` to retry the current view.
- A diff is truncated or a command times out: inspect a smaller target; the 8 MiB output and 30-second process limits are intentional safety boundaries.

## Use alongside coding agents

ChronoGit can serve as a human-controlled review companion while a coding agent works. An optional command-handoff skill targets **OpenAI Codex** first, with **Claude Code** and **Grok Build** also supported. The agent resolves the repository and prepares an exact command for the user to run in a separate terminal; it does not launch, view, or operate the TUI:

```bash
chronogit /path/to/repository --view changes
```

The shared skill is in [`integrations/codex/chronogit`](integrations/codex/chronogit). Install it for Codex with:

```bash
mkdir -p ~/.agents/skills
cp -R integrations/codex/chronogit ~/.agents/skills/
```

Use `~/.claude/skills/` for Claude Code or `~/.grok/skills/` for Grok Build. Invoke it explicitly as `$chronogit` in Codex or `/chronogit` in Claude Code and Grok Build. It also matches natural-language requests to let the user inspect current changes or commit history interactively; it does not prepare a command merely because an agent edited a file or needs to summarize a diff.

Open another terminal window, tab, split, or `tmux` pane and run the command there. The agent cannot see or operate the TUI. Switch between the agent and that terminal with the terminal application's normal controls, press `Q` or `Ctrl-C` to close ChronoGit, and rerun the command to open it again. See the complete [coding-agent setup and switching guide](docs/src/content/docs/guides/agents.md).

The skill grants no additional permissions and does not turn the TUI into a machine-readable protocol. The separate terminal must provide an interactive TTY.

## Development

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features --tests --benches -- -D warnings
cargo test --all-features
cargo build --release
cargo package --locked
```

## Documentation

- English user and developer documentation:
  [`docs/src/content/docs/index.mdx`](docs/src/content/docs/index.mdx)
- 日本語のユーザー・開発者ドキュメント:
  [`docs/src/content/docs/ja/index.mdx`](docs/src/content/docs/ja/index.mdx)
- Contributor workflow and pull request expectations:
  [`CONTRIBUTING.md`](CONTRIBUTING.md)
- Architecture notes and module boundaries:
  [`DEVELOPMENT.md`](DEVELOPMENT.md)

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
