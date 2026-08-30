---
title: コーディングエージェントから使う
description: Codexを第一対象とするコンパニオンスキルを設定し、Claude CodeやGrok Buildから別ターミナル用のChronoGitコマンドを準備します。
tags:
  - エージェント
  - Codex
  - Claude
  - Grok
  - 連携
sidebar:
  order: 5
---

ChronoGitは、コーディングエージェントが実装を進めている間に、人間がリポジトリを対話的かつ読み取り専用で確認する画面です。エージェントはリポジトリを解決して正確なコマンドを案内しますが、TUIの起動、表示、操作はできません。ユーザー自身が操作できる別ターミナルでChronoGitを実行します。

## サポート優先順位

| 優先度 | エージェント | 連携方針 |
| --- | --- | --- |
| 1 | **OpenAI Codex** | 第一対象です。コンパニオンスキルとコマンド引き渡しは、まずCodex向けに設計・検証します。 |
| 2 | **Claude Code** | 同じ`SKILL.md`から別ターミナル用のコマンドを準備できます。 |
| 3 | **Grok Build** | 同じ移植可能なスキルからGrok Buildユーザー向けのコマンドを準備できます。 |

3つとも同じChronoGit CLIを使います。この順位はドキュメントと検証の優先順であり、リポジトリへのアクセス範囲やTUI機能の差ではありません。

## 1. ChronoGitとエージェントをインストールする

ソースのcheckoutからChronoGitをインストールし、`PATH`上にあることを確認します。

```sh title="ターミナル"
cargo install --path /path/to/chronogit --locked
chronogit --version
```

次に利用するエージェントをインストールします。

| エージェント | 公式セットアップ |
| --- | --- |
| Codex | `curl -fsSL https://chatgpt.com/codex/install.sh \| sh`を実行し、`codex`を起動してサインインします。[Codex CLIガイド](https://learn.chatgpt.com/docs/codex/cli)も参照してください。 |
| Claude Code | `curl -fsSL https://claude.ai/install.sh \| bash`を実行し、`claude`を起動します。[Claude Code quickstart](https://code.claude.com/docs/en/quickstart)も参照してください。 |
| Grok Build | `curl -fsSL https://x.ai/cli/install.sh \| bash`を実行し、`grok`を起動してサインインします。[Grok Buildの紹介](https://x.ai/news/grok-build-cli)も参照してください。 |

エージェントと別ターミナルの両方から、同じリポジトリpathへアクセスできる必要があります。エージェント自身はChronoGitを実行しないため対話型TTYを必要としません。ユーザーがコマンドを実行する別ターミナルには対話型TTYが必要です。

## 2. コンパニオンスキルをインストールする

共通スキルは`integrations/codex/chronogit`にあります。Codexを第一対象とするためこのpathを維持していますが、スキル自体は3つのエージェントで使える移植可能な`SKILL.md`形式です。

現在のユーザー向けにインストールします。

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

チームで共有するリポジトリ単位の設定では、同じディレクトリをCodexなら`<repository>/.agents/skills/`、Claude Codeなら`<repository>/.claude/skills/`、Grok Buildなら`<repository>/.grok/skills/`へコピーします。追加したスキルが表示されない場合はエージェントを再起動してください。

Codexでは`/skills`または`$`からスキルを一覧・指定できます。Claude CodeとGrok Buildでは、インストール済みスキルをslash commandとして指定できます。Codexの自動・明示起動については、公式の[Codex skillsドキュメント](https://learn.chatgpt.com/docs/build-skills)を参照してください。

## 3. スキルがコマンドを案内する場面を知る

コンパニオンスキルは、ユーザー自身が対話画面で確認したい場合だけChronoGitコマンドを準備します。

| 依頼 | 動作 |
| --- | --- |
| 「現在の変更をChronoGitで開いて」 | 未ステージの作業ツリー変更を見る`--view changes`コマンドを案内します。 |
| 「あなたが変更した内容を自分で確認したい」 | エージェントの編集後に`--view changes`コマンドを案内します。 |
| 「このリポジトリのコミット履歴をChronoGitで見せて」 | `--view history`コマンドを案内します。 |
| 「差分を要約して」「このpatchをレビューして回答して」 | ChronoGitは使わず、エージェントが構造化されたGit出力を読み、文章で回答します。 |

エージェントがファイルを編集した、または`git diff`を実行したという理由だけではコマンドを案内しません。スキルの自動判定でも、対話型TUIやユーザー自身による視覚的確認が明確に求められている必要があります。

確実に起動したい場合はスキルを明示します。

```text title="Codex"
$chronogit 現在のリポジトリをChangesで開いてください。
```

```text title="Claude Code / Grok Build"
/chronogit 現在のリポジトリをChangesで開いてください。
```

エージェントは対象リポジトリを解決してpathを明示し、次のいずれかを回答します。ユーザー自身が操作できる別ターミナルへコピーして実行してください。

```sh title="別の対話型ターミナル"
chronogit /path/to/repository --view changes
chronogit /path/to/repository --view history
```

## 4. エージェントとChronoGitを行き来する

エージェントの会話とChronoGitを別のターミナルまたはwindowで開きます。

1. インストールしたスキルを明示するか、文章でChronoGitコマンドを依頼します。
2. 回答にある正確なコマンドをコピーします。
3. 同じ環境で別のターミナルwindow、tab、split、または`tmux` paneを開いて実行します。
4. エージェントへ戻るときは、そのターミナルの通常のwindow、tab、pane切り替えを使います。ChronoGit固有の切り替えキーはありません。
5. ChronoGit内では`1`または`2`でChangesとHistoryを切り替えます。`q`または`Ctrl-C`でTUIを閉じ、shellへ戻ります。

エージェントはChronoGitへ入力したキー、選択中のファイル、TUI画面を見ることができません。確認結果をエージェントへ伝えるか、会話内で回答が必要な場合は、同じ変更を構造化されたGitコマンドで調べるよう依頼してください。

## 5. もう一度開く

別ターミナルで同じコマンドを再実行します。「ChronoGitコマンドをもう一度教えて」と依頼するか、スキルを再度指定すれば、現在のリポジトリと指定viewに合わせて再生成できます。ChronoGit自体は閉じたTUI sessionを保持しません。

エージェントも終了した場合は、先に会話を再開します。

| エージェント | 会話の再開方法 |
| --- | --- |
| Codex | `codex resume`を実行して保存済みchatを選び、ChronoGitコマンドをもう一度依頼します。 |
| Claude Code | 現在のdirectoryにある直近の会話は`claude --continue`、選択する場合は`claude --resume`で再開し、コマンドをもう一度依頼します。 |
| Grok Build | 直近のsessionは`grok -c`、Grok内では`/resume`で再開し、コマンドをもう一度依頼します。 |

## 安全性と適さない用途

:::caution[書き込み権限を意味しない]
ChronoGitを開く依頼が許可するのは、この読み取り専用インターフェースだけです。別途ステージ、復元、コミット、チェックアウト、リセット、ブランチ作成などを行う権限にはなりません。
:::

エージェントへ、出力をキャプチャするパイプ、非対話型runner、バックエンドPTY、バックグラウンドタスクでChronoGitを起動させないでください。そのような環境ではユーザーへキー操作を引き渡せず、`an interactive TTY is required`を返すこともあります。エージェントが出力を解析する、多数のrevisionを自動比較する、テキスト差分を返す必要がある場合は、Git plumbingなどの構造化されたツールを使います。ChronoGitにJSON、export、batch、非対話modeはありません。
