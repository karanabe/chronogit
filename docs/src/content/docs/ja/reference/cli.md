---
title: CLIリファレンス
description: ChronoGitのコマンド構文、引数、オプション、起動時の動作です。
tags:
  - CLI
  - リファレンス
sidebar:
  order: 1
---

## 構文

```text
chronogit [OPTIONS] [PATH]
```

## 引数とオプション

| 入力 | デフォルト | 意味 |
| --- | --- | --- |
| `[PATH]` | `.` | リポジトリルートまたはその下のディレクトリ |
| `--view changes\|history\|graph\|code` | `changes` | 最初に開くビュー |
| `--config PATH` | 存在する場合はXDGパス | 表示設定ファイル |
| `--scrolloff LINES` | 設定ファイルの値、未指定なら`2` | カーソル上下の余白行数。`0`で無効。設定ファイルより優先 |
| `--keymap PATH` | 存在する場合はXDGパス | 明示するキーマップ設定ファイル |
| `--lsp PROFILE` | 無効 | 信頼する外部language server profileを有効化。複数回指定可能 |
| `--lsp-config PATH` | 存在する場合はXDGパス | 明示するtrusted user-level LSP profile file |
| `-h`, `--help` | — | ヘルプを出力して終了 |
| `-V`, `--version` | — | バージョンを出力して終了 |

TUIを始める前に`PATH`を解決します。パスは存在するディレクトリで、bareではないGitワークツリーに属する必要があります。`PATH`が入れ子のディレクトリやlinked worktreeを指していても、Gitが検出したリポジトリルートを表示します。

## 表示設定

`$XDG_CONFIG_HOME/chronogit/config.toml`、XDG未設定時は`~/.config/chronogit/config.toml`を読み込みます。`--config PATH`で別のファイルを指定できます。

```toml
scrolloff = 2
```

`scrolloff`は0以上の整数で、ファイル・コミット一覧、diff、ソースコードなどの上下の余白を指定します。短いペインではカーソル行を残せる範囲まで縮め、先頭・末尾では存在する行だけを表示します。起動時に`chronogit --scrolloff 3`で上書きできます。

標準パスにファイルがなければ既定値を使います。明示したファイルの欠落、不正なTOML、未知の設定名、負数や整数以外の値はターミナル初期化前にエラーになります。キーマップは引き続き`keymap.conf`で設定します。

## 例

```sh
# 現在のリポジトリをChangesで開く
chronogit

# 明示したリポジトリをHistoryで開く
chronogit /srv/project --view history

# Graphとプロジェクト固有キーマップで開く
chronogit /srv/project --view graph --keymap ./keymap.conf

# ワークツリーのソース閲覧から開始
chronogit /srv/project --view code

# Rustのsemantic navigationとdocument symbol
chronogit /srv/project --view code --lsp rust-analyzer

# Rust、Java、Pythonを含むpolyglot repository
chronogit /srv/project --view code \
  --lsp rust-analyzer --lsp jdtls --lsp pyright

# ヘルプとバージョンには対話型TTYが不要
chronogit --help
chronogit --version
```

## 終了時の動作

ヘルプ、バージョン、`Q`、`Ctrl-C`による正常終了は成功を返します。リポジトリ、キーマップ、ターミナルの起動失敗は、`chronogit:`で始まる診断と取得できた原因チェーンを標準エラーへ出力し、失敗を返します。明示した`--keymap`は存在し有効である必要があります。標準XDGファイルがなければ組み込みキーを使います。

リポジトリ由来の制御文字は、診断に出す前にエスケープします。起動後の復旧可能なGitエラーはアプリを終了せず、影響するペインまたはフッターに表示します。

`--lsp`はprojectを信頼する明示操作で、serverをdownloadしません。組み込みIDは`rust-analyzer`、`jdtls`、`pyright`、`basedpyright`、`pylsp`です。同じ拡張子のprofileを2つ有効化しても起動はできますが、semantic navigationとdocument-symbol要求はambiguousとして拒否します。明示した`--lsp-config`は存在し、schemaとcommand validationを通る必要があります。暗黙pathは`$XDG_CONFIG_HOME/chronogit/lsp.toml`、fallbackは`~/.config/chronogit/lsp.toml`です。

リポジトリ検出後は、標準入力と標準出力の両方が対話型でなければなりません。stdinからコマンドを読んだり、安定した機械可読表現を出力したりはしません。
