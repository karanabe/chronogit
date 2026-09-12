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
このチェックアウトでは`0.6.0`を準備しています。crates.ioから取得できるようになるまでは、以下の動作を使う場合は信頼できるチェックアウトからインストールしてください。
:::

## 必要な環境

- LinuxまたはmacOS
- Cargoを含むRust `1.88`以降
- `PATH`から実行できるGit
- 80列×24行以上の対話型ターミナル
- bareではないGitリポジトリ

Windows、bareリポジトリ、パイプ、出力をキャプチャするコマンド、バックグラウンドセッションは`0.6.0`ではサポートされません。

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
# chronogit 0.6.0
```

## 0.5.0から更新する

`0.6.0`ではapplication leaderをバックスラッシュからSpaceへ変更し、単独のpane操作と、差分から完全なファイルや変更symbolへ移るflowを追加しました。

- ビュー切替は`Space 1`から`Space 4`、リポジトリ検索は`Space f` / `Space g`を使います。Spaceがapplication leaderで、標準の右移動は`l`またはRightです。
- 前後のペインへのフォーカス移動は、単独の`Ctrl-h` / `Ctrl-k`と`Ctrl-j` / `Ctrl-l`を使います。既存の`Ctrl-w h/k/j/l` sequenceも維持しています。リポジトリ検索のResultsでは、いずれかの前pane操作でクエリ編集へ戻ります。
- 差分では`Space v`で完全なnew-state fileを開き、`Space d`で注釈付きChanges表示を切り替えます。language serverを有効にしている場合は、`Space s`で変更symbolのcontextを開きます。

独自のキーマップは[キーマップリファレンス](/ja/reference/keymap/)に照らして確認してください。設定したactionは標準キーをすべて置き換え、`focus_previous` / `focus_next`では単独キーと`Ctrl-w` aliasも置換対象です。

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

起動後も`Space 1`でChanges、`Space 2`でHistory、`Space 3`でGraph、`Space 4`でCodeへ移動できます。最初の3つがGitワークフロー、Codeが独立したソース閲覧ワークフローです。どのメインビューでも`Space f`でファイル、`Space g`でワークツリー文字列を検索できます。キー一覧は`F1`、閉じる/戻る操作は`q` / `Esc`、終了は`Q` / `Ctrl-C`です。

## 次に読む

- [未ステージの変更を調べる](/ja/guides/changes/)
- [コミット履歴、メッセージ、ツリーをたどる](/ja/guides/history/)
- [ワークツリーのソースコードを閲覧する](/ja/guides/code-viewer/)
- [ファイル、内容、ファイル単位の履歴を検索する](/ja/guides/search/)
- [キー操作と画面レイアウトを覚える](/ja/guides/navigation/)
- [読み取り専用の保証とリソース上限を確認する](/ja/reference/safety-and-limits/)

起動できない場合は[トラブルシューティング](/ja/troubleshooting/common-problems/)を参照してください。
