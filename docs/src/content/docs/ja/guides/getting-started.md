---
title: はじめに
description: チェックアウトからChronoGitをインストールし、最初のリポジトリを開きます。
tags:
  - インストール
  - クイックスタート
  - ターミナル
sidebar:
  order: 1
---

ChronoGitは、未ステージのワークツリー変更とコミット履歴を調べる読み取り専用のターミナルUIです。このガイドでは、ソースのチェックアウトからバージョン`0.1.0`をインストールし、リポジトリを変更せずに開きます。

:::note[配布状況]
`0.1.0`ではCargoレジストリへの公開が無効です。公開の配布方法が案内されるまでは、信頼できるチェックアウトからインストールしてください。
:::

## 必要な環境

- LinuxまたはmacOS
- Cargoを含むRust `1.88`以降
- `PATH`から実行できるGit
- 80列×24行以上の対話型ターミナル
- bareではないGitリポジトリ

Windows、bareリポジトリ、パイプ、出力をキャプチャするコマンド、バックグラウンドセッションは`0.1.0`ではサポートされません。

## チェックアウトからインストールする

ChronoGitリポジトリのルートで実行します。

```sh title="ターミナル"
cargo install --path . --locked
```

インストールしたバイナリを確認します。

```sh title="ターミナル"
chronogit --version
# chronogit 0.1.0
```

## リポジトリを開く

対象リポジトリ内で起動します。

```sh title="ターミナル"
cd /path/to/repository
chronogit
```

リポジトリルートや、その下のディレクトリを明示することもできます。

```sh title="ターミナル"
chronogit /path/to/repository/subdirectory
```

ChronoGitはGitにワークツリーのルートを問い合わせるため、指定したサブディレクトリだけでなくリポジトリ全体を表示します。`PATH`を省略すると現在のディレクトリが使われます。

## 最初のビューを選ぶ

標準の**Changes**ビューは未ステージの作業を表示します。コミットから見たい場合は**History**で起動します。

```sh title="ターミナル"
chronogit /path/to/repository --view history
```

起動後も`1`でChanges、`2`でHistoryへ移動できます。キー一覧は`F1`、終了は`q`または`Ctrl-C`です。

## 次に読む

- [未ステージの変更を調べる](/ja/guides/changes/)
- [コミット履歴、メッセージ、ツリーをたどる](/ja/guides/history/)
- [キー操作と画面レイアウトを覚える](/ja/guides/navigation/)
- [読み取り専用の保証とリソース上限を確認する](/ja/reference/safety-and-limits/)

起動できない場合は[トラブルシューティング](/ja/troubleshooting/common-problems/)を参照してください。
