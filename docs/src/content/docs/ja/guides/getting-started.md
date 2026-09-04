---
title: はじめに
description: CargoでChronoGitをインストールし、最初のリポジトリを開きます。
tags:
  - インストール
  - クイックスタート
  - ターミナル
sidebar:
  order: 1
---

ChronoGitは、Gitの変更、履歴、ワークツリーのソースコードを調べる読み取り専用のターミナルUIです。このガイドでは、Cargoでインストールし、リポジトリを変更せずに開きます。

:::note[配布状況]
`0.4.0`のmanifestはcrates.ioへ公開できる状態です。このversionをregistryから取得できるようになるまでは、信頼できるチェックアウトからインストールしてください。
:::

## 必要な環境

- LinuxまたはmacOS
- Cargoを含むRust `1.88`以降
- `PATH`から実行できるGit
- 80列×24行以上の対話型ターミナル
- bareではないGitリポジトリ

Windows、bareリポジトリ、パイプ、出力をキャプチャするコマンド、バックグラウンドセッションは`0.4.0`ではサポートされません。

## crates.ioからインストールする

公開済みcrateをlockされた依存versionでインストールします。

```sh title="ターミナル"
cargo install chronogit --locked
```

## チェックアウトからインストールする

ChronoGitリポジトリのルートで実行します。

```sh title="ターミナル"
cargo install --path . --locked
```

どちらの方法を使った場合も、インストールしたバイナリを確認します。

```sh title="ターミナル"
chronogit --version
# chronogit 0.4.0
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

標準の**Changes**ビューは未ステージの作業を表示します。目的に合わせて**History**、**Graph**、ワークツリーの**Code** viewerから直接起動することもできます。

```sh title="ターミナル"
chronogit /path/to/repository --view history
chronogit /path/to/repository --view graph
chronogit /path/to/repository --view code
```

起動後も`\1`でChanges、`\2`でHistory、`\3`でGraph、`\4`でCodeへ移動できます。最初の3つがGitワークフロー、Codeが独立したソース閲覧ワークフローです。どのメインビューでも`\f`でファイル、`\g`でワークツリー文字列を検索できます。キー一覧は`F1`、閉じる/戻る操作は`q` / `Esc`、終了は`Q` / `Ctrl-C`です。

## 次に読む

- [未ステージの変更を調べる](/ja/guides/changes/)
- [コミット履歴、メッセージ、ツリーをたどる](/ja/guides/history/)
- [ワークツリーのソースコードを閲覧する](/ja/guides/code-viewer/)
- [ファイル、内容、ファイル単位の履歴を検索する](/ja/guides/search/)
- [キー操作と画面レイアウトを覚える](/ja/guides/navigation/)
- [読み取り専用の保証とリソース上限を確認する](/ja/reference/safety-and-limits/)

起動できない場合は[トラブルシューティング](/ja/troubleshooting/common-problems/)を参照してください。
