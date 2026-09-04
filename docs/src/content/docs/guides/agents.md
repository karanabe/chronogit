---
title: Use ChronoGit alongside coding agents
description: Set up the optional command-handoff skill and operate ChronoGit yourself in a separate terminal during coding-agent work.
tags:
  - agents
  - codex
  - claude
  - grok
  - integration
sidebar:
  order: 5
---

ChronoGit gives a human an interactive, read-only view of the repository while a coding agent handles the implementation work. The agent resolves the repository and gives the user an exact command, but it does not launch, view, or operate the TUI. Run ChronoGit in a separate terminal that you control.

## Command-handoff support

| Priority | Agent | Integration |
| --- | --- | --- |
| 1 | **OpenAI Codex** | Primary command-handoff target. The companion skill is designed and validated for Codex first. |
| 2 | **Claude Code** | The same `SKILL.md` can prepare commands as a Claude Code Agent Skill. |
| 3 | **Grok Build** | The same portable skill can prepare commands for Grok Build users. |

All three use the same ChronoGit CLI. The priority indicates the order in which this project documents and validates integrations, not a difference in repository access or TUI features.

## 1. Install ChronoGit and an agent

Install ChronoGit from a source checkout and confirm it is on `PATH`:

```sh title="Terminal"
cargo install --path /path/to/chronogit --locked
chronogit --version
```

Then install the agent you intend to use:

| Agent | Official setup |
| --- | --- |
| Codex | `curl -fsSL https://chatgpt.com/codex/install.sh \| sh`, then run `codex` and sign in. See the [Codex CLI guide](https://learn.chatgpt.com/docs/codex/cli). |
| Claude Code | `curl -fsSL https://claude.ai/install.sh \| bash`, then run `claude`. See the [Claude Code quickstart](https://code.claude.com/docs/en/quickstart). |
| Grok Build | `curl -fsSL https://x.ai/cli/install.sh \| bash`, then run `grok` and sign in. See the [Grok Build introduction](https://x.ai/news/grok-build-cli). |

The agent and the separate terminal must access the same repository path. The agent itself does not need an interactive TTY because it never runs ChronoGit. The terminal where the user runs the command must be interactive.

## 2. Install the companion skill

The shared skill is stored at `integrations/codex/chronogit`. The path keeps Codex as the first-class integration, while the skill itself follows the portable `SKILL.md` format used by all three agents.

Install it for the current user:

```sh title="Codex"
mkdir -p ~/.agents/skills
cp -R /path/to/chronogit/integrations/codex/chronogit ~/.agents/skills/
```

```sh title="Claude Code"
mkdir -p ~/.claude/skills
cp -R /path/to/chronogit/integrations/codex/chronogit ~/.claude/skills/
```

```sh title="Grok Build"
mkdir -p ~/.grok/skills
cp -R /path/to/chronogit/integrations/codex/chronogit ~/.grok/skills/
```

For a team-shared, repository-scoped installation, copy the same directory to `<repository>/.agents/skills/` for Codex, `<repository>/.claude/skills/` for Claude Code, or `<repository>/.grok/skills/` for Grok Build. Restart the agent if the newly installed skill does not appear.

Codex can list or mention skills with `/skills` or `$`; Claude Code and Grok Build expose installed skills as slash commands. The official [Codex skills documentation](https://learn.chatgpt.com/docs/build-skills) explains Codex's automatic and explicit skill activation.

## 3. Know when the skill provides a command

The companion skill prepares a ChronoGit command only when the user wants a human-controlled, interactive review.

| Request | Result |
| --- | --- |
| “Open my current changes in ChronoGit.” | Provide a `--view changes` command for unstaged working-tree changes. |
| “Let me inspect what you changed.” | Provide a `--view changes` command after the agent's edits. |
| “Show this repository's commits/history in ChronoGit.” | Provide a `--view history` command. |
| “Let me browse this repository's source in ChronoGit.” | Provide a `--view code` command. Add trusted `--lsp PROFILE` options only when the user requests semantic navigation. |
| “Summarize the diff” or “review this patch and answer here.” | Do not use ChronoGit; the agent should use structured Git output and respond with text. |

The skill does **not** prepare a command merely because an agent edited a file or ran `git diff`. Automatic skill matching should still require a clear request for the interactive TUI or for the user to inspect the change visually.

Explicit invocation removes any ambiguity:

```text title="Codex"
$chronogit Open the current repository in Changes view.
```

```text title="Claude Code or Grok Build"
/chronogit Open the current repository in Changes view.
```

The agent resolves the repository, passes its path explicitly, and returns one of the following. Copy it into a separate terminal that you control:

```sh title="Separate interactive terminal"
chronogit /path/to/repository --view changes
chronogit /path/to/repository --view history
chronogit /path/to/repository --view code
```

## 4. Move between the agent and ChronoGit

Keep the agent conversation and ChronoGit in separate terminals or windows:

1. Ask the agent for ChronoGit, explicitly or through the installed skill.
2. Copy the exact command from the response.
3. Open another terminal window, tab, split, or `tmux` pane in the same environment and run it.
4. Use that terminal's normal window, tab, or pane controls to switch back to the agent. There is no ChronoGit-specific switching key.
5. In ChronoGit, press `1`, `2`, `3`, or `4` to switch between Changes, History, Graph, and Code. Press `Q` or `Ctrl-C` to close the TUI and return to the shell.

The agent cannot receive your keystrokes, see the selected file, or read the TUI screen. Tell it what you found, or ask it to inspect the same change with structured Git commands if you need an answer in the conversation.

## 5. Open it again

Rerun the same command in the separate terminal. You can also say “Give me the ChronoGit command again” or invoke the skill again to regenerate it for the current repository and requested view. ChronoGit does not preserve a suspended TUI session.

If you closed the agent as well, resume its conversation first:

| Agent | Resume the agent conversation |
| --- | --- |
| Codex | Run `codex resume`, choose the saved chat, then request the ChronoGit command again. |
| Claude Code | Run `claude --continue` for the latest conversation in the current directory, or `claude --resume` to choose one, then request the command again. |
| Grok Build | Run `grok -c` for the most recent session, or use `/resume` in Grok, then request the command again. |

## Safety and unsuitable tasks

:::caution[No implied write permission]
A request to open ChronoGit authorizes only this read-only interface. It does not authorize a separate stage, restore, commit, checkout, reset, branch, or other Git mutation.
:::

Do not ask the agent to launch ChronoGit in an output-capture pipeline, non-interactive runner, backend PTY, or background task. Those environments cannot hand keyboard control to the user and may receive `an interactive TTY is required`. Use ordinary Git plumbing or another structured tool when an agent must parse output, compare many revisions automatically, or return a textual diff. ChronoGit has no JSON, export, batch, or non-interactive mode.
