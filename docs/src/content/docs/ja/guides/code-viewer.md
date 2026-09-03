---
title: ソースコードを閲覧する
description: ワークツリーのファイル階層をたどり、ソースファイル全文を読みます。
tags:
  - コード
  - ファイル
  - ナビゲーション
sidebar:
  order: 4
---

ChronoGitは、従来のGitレビュー画面とワークツリーのCode viewerを分けています。標準起動ではこれまでどおりChangesを開きます。差分やコミットではなくリポジトリを閲覧するときは`4`を押すか、`chronogit --view code`で起動します。

## ファイルツリーを操作する

上段には追跡済みファイルと、ignoreされていない未追跡ファイルが表示されます。各階層では名前順のディレクトリを先に、その後へ名前順のファイルを表示します。大きなリポジトリでも見渡せるように、ディレクトリは折り畳んだ状態から始まります。

1. `j` / `k`、矢印キー、`g` / `G`、Home/Endで移動します。
2. ディレクトリ上で`Enter`を押すと、直下の項目を展開または折り畳みます。
3. ファイルへ移動すると、現在のワークツリー内容を下段へ読み込みます。
4. `l`または`Ctrl-j`でコードペインへ、`h`または`Ctrl-k`でツリーへフォーカスを移します。

`r`を押すと、Gitが返す現在の追跡済み・非ignoreファイル一覧からツリーを作り直します。削除済みの追跡対象は一覧に残り、開くと利用できない旨の要約を表示します。バイナリはデコードせず、シンボリックリンクはたどらずに表示します。

## ファイル全文を読む

ツリーのファイル上、または下段へフォーカスした状態で`Enter`を押すと、ほぼ全画面のコードウィンドウが開きます。このウィンドウは意図的に差分フロートと同じキーマップを使います。

- `j` / `k`、`g` / `G`、`Ctrl-d` / `Ctrl-u`で現在行マーカーを移動します。
- `zh` / `zl`で横スクロールします。
- `/`と`?`でsmart-caseの前方/後方検索を開始し、`n` / `N`で末尾/先頭を折り返しながら繰り返します。
- `Enter`、`q`、`Esc`でウィンドウを閉じ、2ペインのCodeビューへ戻ります。

認識できるソース種別には組み込みのsyntax定義を使います。読み取りは8 MiBが上限で、それを超えるtextは無制限に保持せずtruncated表示になります。

## コード閲覧中に検索する

Code viewerでもGit画面と同じく、`Space f`でファイルパス、`Space g`で固定文字列を検索できます。結果を開くとCodeへ直接戻り、選択ファイルが見えるまで親ディレクトリを展開して、現在内容を読み込みます。内容検索では現在行マーカーを一致行へ配置します。

## セマンティックジャンプの状況

定義、実装、型定義、宣言へのジャンプは実現可能ですが、現在のファイル読み取り機能には含まれません。Gitとsyntax highlightだけではsymbolを解決できないため、Language Server Protocol clientが必要です。

将来の実装では、LSP transportをdomain modelの外に置き、project設定からlanguage serverを検出し、`didOpen`/`didChange`でdocumentを同期します。serverが合意したencodingで現在のbyte/character位置を変換し、`textDocument/definition`、`implementation`、`typeDefinition`、`declaration`をtyped application effectとして要求し、1件または複数のlocationを表示します。さらにserver lifecycle、timeout、cancel、古いresponseの破棄、multi-root、未対応language、リポジトリ外locationの扱いが必要です。これらの境界とfallbackを実装するまでは、Codeはeditor/IDEではなく読み取り専用のtext viewerです。
