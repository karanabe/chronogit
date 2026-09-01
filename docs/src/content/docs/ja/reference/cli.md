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
| `--view changes\|history\|graph` | `changes` | 最初に開くビュー |
| `--keymap PATH` | 存在する場合はXDGパス | 明示するキーマップ設定ファイル |
| `-h`, `--help` | — | ヘルプを出力して終了 |
| `-V`, `--version` | — | バージョンを出力して終了 |

TUIを始める前に`PATH`を解決します。パスは存在するディレクトリで、bareではないGitワークツリーに属する必要があります。`PATH`が入れ子のディレクトリやlinked worktreeを指していても、Gitが検出したリポジトリルートを表示します。

## 例

```sh
# 現在のリポジトリをChangesで開く
chronogit

# 明示したリポジトリをHistoryで開く
chronogit /srv/project --view history

# Graphとプロジェクト固有キーマップで開く
chronogit /srv/project --view graph --keymap ./keymap.conf

# ヘルプとバージョンには対話型TTYが不要
chronogit --help
chronogit --version
```

## 終了時の動作

ヘルプ、バージョン、`q`、`Ctrl-C`による正常終了は成功を返します。リポジトリ、キーマップ、ターミナルの起動失敗は、`chronogit:`で始まる診断と取得できた原因チェーンを標準エラーへ出力し、失敗を返します。明示した`--keymap`は存在し有効である必要があります。標準XDGファイルがなければ組み込みキーを使います。

リポジトリ由来の制御文字は、診断に出す前にエスケープします。起動後の復旧可能なGitエラーはアプリを終了せず、影響するペインまたはフッターに表示します。

リポジトリ検出後は、標準入力と標準出力の両方が対話型でなければなりません。stdinからコマンドを読んだり、安定した機械可読表現を出力したりはしません。
