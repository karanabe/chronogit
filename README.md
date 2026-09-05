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

ChronoGit is prepared as `0.5.0`. Linux and macOS are supported. Windows, bare repositories, and non-interactive terminals are not supported yet.

The manifest is prepared to publish this release to crates.io. Publishing remains a separate maintainer action.

Upgrading from `0.4.0` changes the default keys: `\` is the leader, pane focus uses `Ctrl-w` sequences, and `Enter` moves within open text documents. Review the [upgrade notes](docs/src/content/docs/guides/getting-started.md#upgrade-from-040), especially if you use a custom keymap.

## Requirements

- Rust 1.88 or newer
- Git available on `PATH`
- An interactive terminal of at least 80x24
- Optional: a user-installed language server for semantic Code navigation

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
chronogit [PATH] [--view changes|history|graph|code] [--keymap PATH] [--lsp PROFILE]...
```

`PATH` may be a repository root or any directory below it. It defaults to the current directory. `--view` defaults to `changes`, preserving the existing landing workflow; `--keymap` overrides the optional XDG keymap path.

## Workflows

### Read unstaged changes

Start in Changes view with `chronogit`. The left pane contains tracked and untracked worktree changes. Select a file to see its `index → working tree` diff. Recognized source files use syntax highlighting; additions and removals retain muted diff backgrounds without replacing token colors.

Staged-only files are intentionally hidden. A file with both staged and unstaged edits shows only the unstaged part.

### Read commit history

Press `\2`, or start with `chronogit --view history`. History uses three full-width rows—commits, changed files/tree, then diff—so commit subjects and paths retain the terminal width. Select a commit and press `Enter` to focus Changed files, select a file, then press `Enter` to open its patch in a large floating diff.

- A root commit is compared with the empty tree.
- A normal commit is compared with its parent.
- A merge commit is compared with its first parent.

The active comparison is shown in both the diff pane title and the footer.

### Read a commit message or tree

- Press `\m` to open the selected commit's complete message in a floating overlay. Press `\m` again, `q`, or `Esc` to close it.
- Press `\b` to switch History to a three-row body layout: the same commit list, commit body, and changed files. Press `\b` again to return to the diff layout.
- Press `\t` to switch between changed files and the selected commit's tree.
- Press `Enter` to expand a directory or open a selected file in the floating diff. An unchanged tree file reports that it has no change in the selected commit.
- The floating diff supports Vim normal-mode movement: counts, `h` / `j` / `k` / `l`, word motions, line and buffer motions, character find/till, sentence/paragraph/section motions, matching pairs, viewport positioning, and `[c` / `]c` change-block jumps. The character cursor is drawn without replacing syntax colors. `/` and `?` search; `n` / `N`, `*` / `#`, and `g*` / `g#` repeat or derive searches. `Enter` acts like `+` and moves to the next line's first nonblank; use `q` to close the diff immediately. `Esc` first clears visible search highlights, then closes on the next press.

Symlinks and submodules are identified in the tree. ChronoGit does not enter a submodule repository.

### Follow the Git graph

Press `\3`, or start with `chronogit --view graph`. The graph uses commit parent relationships to display active branch lanes. `\m` opens the selected commit message. `Enter` opens a floating two-row detail window over the graph, with changed files above the selected file's diff; another `Enter` opens the complete diff. Press `q` to return one level at a time. `Esc` first dismisses active diff search highlights, then returns.

### Browse the complete working tree

Press `\4`, or start with `chronogit --view code`, to enter the Code viewer. The upper pane is an expandable tree containing tracked files and non-ignored untracked files; the lower pane previews the selected file with line numbers and syntax highlighting. Press `Enter` on a directory to expand or collapse it. Use `Ctrl-w h` / `Ctrl-w k` and `Ctrl-w j` / `Ctrl-w l` to move between the tree and code panes.

Press `Enter` on a file in the tree, or from the code pane, to open the current content in a large floating view. It shares the read-only Vim movement vocabulary with diffs, including word motions, counts, character searches, marks, the jump list, and viewport commands. `Enter` moves down inside the document; `q` returns immediately. `Esc` first dismisses search highlights, then returns. `\f` and `\g` search from the Code viewer; selecting a result reveals the file in the tree and opens its current content at the matching line when available.

In focused Code content, Vim movements operate on a UTF-8-safe character cursor. Set a mark with `m{letter}`, jump linewise with `'{letter}`, or jump to its exact column with `` `{letter}``. With an explicitly enabled language server, `K` opens hover information; `gd`, `gi`, `gy`, and `gD` navigate to definition, implementation, type definition, and declaration. `Ctrl-o` / `Ctrl-i` traverse a shared jump list containing Vim motions, marks, searches, and LSP targets. Results outside the repository, including virtual `jdt:` documents, are reported but never passed to the file reader.

### Search files or working-tree text

Press `\f` from any main view to find tracked and untracked file names. Press `\g` for a fixed-text search across non-binary working-tree content. Results update after every inserted or deleted query character. Press `Enter` or `Ctrl-j` to focus Results, choose a result with `j` / `k`, and press `Enter` again to open it. Press `Ctrl-w k` from Results to return to Search, edit the current query, and run another live search.

While entering a search query, `q` and `Q` are ordinary query characters. Use `Esc` to cancel the prompt and `Ctrl-C` to quit.

The file view shows its commit history above its current working-tree content. Changing the selected history commit replaces the lower pane with that commit's first-parent diff. `Enter` opens the current content or diff full-screen; `q` closes the float and then returns to the originating view; `Esc` first dismisses active diff search highlights when present.

## Keys

| Key | Action |
|---|---|
| `q` | Close the current float or go back immediately |
| `Esc` | Cancel input; in Diff/Code dismiss search highlights first, then close/back |
| `Q` / `Ctrl-C` | Quit |
| `\1` / `\2` / `\3` / `\4` | Changes / History / Graph / Code |
| `\f` / `\g` | Search repository files / working-tree text |
| `Ctrl-w h/k` / `Ctrl-w j/l` | Focus the previous / next pane; `Ctrl-w W/w` and arrow aliases work too |
| `[count]h/j/k/l`, arrows | Character / line movement; counts apply throughout |
| `[count]Space`, `[count]Backspace` / `Ctrl-H` | Character movement that wraps across lines like Vim's default `'whichwrap'` |
| `w/W/e/E`, `b/B/ge/gE` | Word / WORD movement |
| `0`, `^`, `$`, `g_`, `gg`, `G`, `go`, `%` | Line, buffer, byte-offset, and pair movement |
| `f/F/t/T{char}`, `;` / `,` | Find/till a character and repeat/reverse |
| `(`/`)`, `{`/`}`, `[[`/`]]`, `[]`/`][` | Sentence, paragraph, and section movement |
| `Ctrl-d/u`, `Ctrl-f/b`, `zt/zz/zb`, `zh/zl/zH/zL/zs/ze` | Vertical and horizontal viewport movement |
| `m{char}`, `'{char}`, `` `{char}`` | Set and jump to a Code mark |
| `K` | Toggle LSP hover information; `j` / `k` scroll it |
| `gd` / `gi` | Definition / implementation |
| `gy` / `gD` | Type definition / declaration |
| `[count]Ctrl-o` / `[count]Ctrl-i` | Older / newer Vim or LSP jump location |
| `r` | Refresh the current view |
| `\m` / `\b` / `\t` | Full message / History body layout / commit tree |
| `Enter` | Select/open an item; in an open text document, move like `+` |
| `/` / `?`, `n` / `N`, `*` / `#`, `g*` / `g#` | Search within the active text document |
| `F1` | Toggle in-app help |

Diff and Code highlight each matched string: the current match has a yellow background, other matches are underlined, and the cursor stays cyan. Default `Esc` removes only search decoration, keeping the query, direction, focus, cursor and scroll position. `n` / `N` or a confirmed search restores it. With no highlights, `Esc` follows the existing close/back behavior; `q` always closes/backs immediately outside input. Prompt and find/till/mark cancellation and frontmost help, hover or repository search take priority.

History always stacks its three panes vertically at the supported terminal sizes. In Changes, widths below 110 columns show the focused pane at full width; use `Ctrl-w h` and `Ctrl-w l` to move between panes.

In document searches (`/` or `?`), Backspace deletes the last character. Deleting the last character leaves an empty prompt so you can type a replacement; press Backspace once more to cancel. Esc cancels at any point. Cancellation removes the input cursor (`█`) and keeps the document, focus, cursor, scroll position and previous confirmed search, including its direction and highlight visibility. `n` / `N` resumes that search. Enter in an empty prompt reuses the previous query. A retained search status such as `/word 1/3` is not input mode. Repository searches (`\f` / `\g`) keep their existing behavior.

## Keymap configuration

ChronoGit loads `$XDG_CONFIG_HOME/chronogit/keymap.conf`, falling back to `~/.config/chronogit/keymap.conf`, when that file exists. Copy [`config/keymap.conf`](config/keymap.conf) and uncomment only the actions you want to replace, or pass another file with `--keymap PATH`. For example:

```ini
[bindings]
show_graph = x
show_code = c
file_search = ctrl-p
content_search = space s
# Optional: immediate close even on Esc (replaces its dismissal behavior)
# close = q, esc
quit = Q
```

An explicit `close` assignment replaces both default close keys. Every assigned key closes immediately, including an explicitly listed `esc`; omitting `esc` removes its default action so it can be rebound. Leave `close` unset to keep the default two-step Esc.

Key sequences are space-separated and alternatives are comma-separated. Invalid, duplicate, or ambiguous bindings fail before raw terminal mode starts. `Ctrl-C` always remains available for safe exit. The complete action and key syntax is in the [keymap reference](docs/src/content/docs/reference/keymap.md).

## Optional language-server navigation

LSP is off unless at least one `--lsp PROFILE` is supplied. ChronoGit ships client profiles, not language-server binaries:

```bash
chronogit --view code --lsp rust-analyzer
chronogit --view code --lsp jdtls
chronogit --view code --lsp pyright
chronogit --view code --lsp basedpyright
chronogit --view code --lsp pylsp

# A polyglot repository
chronogit --view code --lsp rust-analyzer --lsp jdtls --lsp pyright
```

Install the selected executable separately using the upstream instructions for [rust-analyzer](https://rust-analyzer.github.io/book/installation.html), [Eclipse JDT LS](https://github.com/eclipse-jdtls/eclipse.jdt.ls), [Pyright](https://github.com/microsoft/pyright), [basedpyright](https://docs.basedpyright.com/latest/installation/), or [Python LSP Server](https://github.com/python-lsp/python-lsp-server), and put it on `PATH`. JDT LS uses its `jdtls` wrapper and currently needs a Java 21+ runtime. Pyright and basedpyright use their `*-langserver --stdio` commands; Python LSP Server uses `pylsp`. Do not enable multiple Python profiles together: ChronoGit refuses ambiguous routing instead of selecting one implicitly. At startup, ChronoGit only validates and selects profiles. The matching process starts lazily on the first hover/navigation request, after extension and workspace-root routing, and is reused until eviction or shutdown.

Built-ins can be replaced or extended only from the trusted user-level `$XDG_CONFIG_HOME/chronogit/lsp.toml` (or `~/.config/chronogit/lsp.toml`), or `--lsp-config PATH`. Start from [`config/lsp.toml`](config/lsp.toml). Repository-local command configuration is never loaded. A profile is data, so future languages do not require another client implementation:

```toml
[servers.gopls]
language_id = "go"
extensions = ["go"]
command = ["gopls"]
root_markers = ["go.mod", "go.work"]
```

Commands are direct argument arrays, not shell strings. Supported whole-argument placeholders are `{workspace_root}`, `{workspace_data}`, `{workspace_config}`, and `{cache_dir}`; partial interpolation is rejected. The workspace data/configuration placeholders require `workspace_data = true`.

## Read-only and failure behavior

ChronoGit only invokes an allowlisted set of Git read commands. It never stages, restores, commits, checks out, resets, or updates references. Commands are executed without a shell, paths are passed after `--`, optional Git locks are disabled, and external diff, textconv, pager, and fsmonitor programs are disabled. Code-viewer file reads remain rooted at the discovered worktree and do not follow symbolic links.

An explicitly enabled language server is a separate trust boundary. It receives the repository workspace and current source text, and may execute project tooling or create caches/build artifacts according to that server and project configuration. Enable LSP only for repositories you trust. ChronoGit starts no server by default, downloads nothing, uses no repository-provided server command, and keeps JDT workspace data outside the repository.

Git output is bounded. A text diff larger than 8 MiB is terminated and displayed as truncated instead of growing memory without limit; current file reads are also capped at 8 MiB. A Git command that runs longer than 30 seconds is terminated with a recoverable error. Binary changes and files are shown as a summary.

Startup errors are printed before raw terminal mode is enabled. During the TUI, recoverable Git errors are shown in the affected pane. Normal exit, errors, Ctrl-C, and panics restore the alternate screen, cursor, mouse capture, and raw mode.

## Non-goals

ChronoGit does not stage, restore, commit, reset, check out, or otherwise mutate a repository. It also does not provide staged-change, remote, pull-request, blame, stash, editor, plugin, or machine-readable export features. Semantic navigation is limited to complete current-working-tree text files and repository-contained `file:` URI results; dependency, standard-library, archive, and virtual-document source is not opened.

## Troubleshooting

- `an interactive TTY is required`: run `chronogit` directly in a terminal, not in a pipe, background task, or captured command.
- `repository path is not a directory` or repository discovery fails: pass an existing non-bare Git repository or a directory below it.
- `Terminal too small`: resize to at least 80 columns by 24 rows. `Q` and `Ctrl-C` still quit safely.
- A pane shows a Git error: correct the repository or permission problem, then press `r` to retry the current view.
- A diff is truncated or a command times out: inspect a smaller target; the 8 MiB output and 30-second process limits are intentional safety boundaries.
- `LSP is disabled`: restart with one or more trusted `--lsp PROFILE` options.
- A server cannot start: install the selected external binary and verify it is on `PATH`; errors remain recoverable inside the TUI.
- Multiple Python servers match: enable exactly one of `pyright`, `basedpyright`, or `pylsp` for `.py`/`.pyi` files.

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

Use `~/.claude/skills/` for Claude Code or `~/.grok/skills/` for Grok Build. Invoke it explicitly as `$chronogit` in Codex or `/chronogit` in Claude Code and Grok Build. It also matches natural-language requests to let the user inspect current changes, commit history, or source code interactively; it does not prepare a command merely because an agent edited a file or needs to summarize a diff.

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

- Developer documentation:
  [`docs/src/content/docs/index.mdx`](docs/src/content/docs/index.mdx)
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
