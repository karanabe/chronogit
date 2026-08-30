---
name: chronogit
description: Prepare an exact ChronoGit command for the user to run in a separate interactive terminal when they want to inspect working-tree changes, commit history, messages, or trees personally in a read-only TUI. Use for explicit ChronoGit requests or a clear request for human-controlled visual review; do not use merely because changes exist, for machine-readable output, or for Git mutations.
---

# ChronoGit

Use the `chronogit` CLI when the user wants to explore Git changes or history personally in a separate interactive terminal. This skill is Codex-first and can also be installed in Claude Code or Grok Build. The agent prepares the command but does not launch, view, or operate the TUI.

## Trigger boundary

Prepare a ChronoGit command when the user explicitly names it, asks to open an interactive Git diff/history viewer, or asks to inspect the agent's changes personally in a TUI.

Do not prepare a command only because files changed, as an internal step in the agent's own review, or when the user expects a textual summary. Use structured Git commands for machine analysis and answer in the conversation instead.

## Separate-terminal handoff

1. Confirm that `chronogit` is available on `PATH`. If it is not, provide the appropriate install command before the launch command.
2. Resolve the intended repository path. Pass it explicitly; do not guess between multiple repositories.
3. Choose `changes` for current unstaged work or inspection after agent edits. Choose `history` for commits, commit messages, changed files, or repository trees.
4. Give the user one exact, shell-safe command to run in a separate interactive terminal window, tab, split, or `tmux` pane:

   ```text
   chronogit <repository-path> --view changes
   chronogit <repository-path> --view history
   ```

5. Tell the user that the agent cannot see or operate the TUI, and that `q` or `Ctrl-C` closes it.
6. Do not execute the command in an agent command runner, backend PTY, background task, or output-capture pipeline. Those sessions cannot transfer TUI keyboard control to the user.
7. If the user asks to open ChronoGit again, provide a fresh command with the same repository and view unless they request another target.

There is no suspended ChronoGit session to resume after exit; the user reruns the command instead. Use structured Git commands for any inspection the agent itself must perform.

## Useful keys

- `1` / `2`: changes / history
- `h` / `l` or `Ctrl-k` / `Ctrl-j`: focus previous / next pane
- `j` / `k`, `g` / `G`, `Ctrl-u` / `Ctrl-d`: navigate
- `m`: full commit-message overlay
- `b`: history diff / body layout
- `t`: changed files / commit tree
- `Enter` or `Space`: expand a tree directory, or open/close a file diff
- `F1`: help
- `q` / `Ctrl-C`: exit

ChronoGit is read-only. It has no stage, restore, commit, checkout, reset, or branch action. Do not infer authorization for any separate Git mutation from a request to launch it.
