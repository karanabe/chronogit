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

ChronoGitは、従来のGitレビュー画面とワークツリーのCode viewerを分けています。標準起動ではこれまでどおりChangesを開きます。差分やコミットではなくリポジトリを閲覧するときは`\4`を押すか、`chronogit --view code`で起動します。

## ファイルツリーを操作する

上段には追跡済みファイルと、ignoreされていない未追跡ファイルが表示されます。各階層では名前順のディレクトリを先に、その後へ名前順のファイルを表示します。大きなリポジトリでも見渡せるように、ディレクトリは折り畳んだ状態から始まります。

1. `j` / `k`、矢印キー、`gg` / `G`、Home/Endで移動します。
2. ディレクトリ上で`Enter`を押すと、直下の項目を展開または折り畳みます。
3. ファイルへ移動すると、現在のワークツリー内容を下段へ読み込みます。
4. `Ctrl-w j` / `Ctrl-w l`でコードペインへ移ります。コード本文へフォーカスした後の`h` / `l`は文字カーソルを動かし、`Ctrl-w h` / `Ctrl-w k`でツリーへ戻ります。

`r`を押すと、Gitが返す現在の追跡済み・非ignoreファイル一覧からツリーを作り直します。削除済みの追跡対象は一覧に残り、開くと利用できない旨の要約を表示します。バイナリはデコードせず、シンボリックリンクはたどらずに表示します。

## ファイル全文を読む

ツリーのファイル上、または下段へフォーカスした状態で`Enter`を押すと、ほぼ全画面のコードウィンドウが開きます。このウィンドウは意図的に差分フロートと同じキーマップを使います。

- normal-mode motionでは数値countを使えます。`h` / `j` / `k` / `l`と矢印はUTF-8安全なcursorを動かし、短い行をまたいでも希望表示列を維持します。
- `w` / `W` / `e` / `E`と`b` / `B` / `ge` / `gE`はVimのword / WORD境界を実装します。`f` / `F` / `t` / `T`は文字引数を受け、`;` / `,`で同方向 / 逆方向へ反復します。
- 論理行、buffer、文、段落、section、delimiter、method、preprocessor、comment motionは標準Vimキーを使います。`go`は1始まりのbyte offsetを受けます。
- `Ctrl-d` / `Ctrl-u`、`Ctrl-f` / `Ctrl-b`、`Ctrl-e` / `Ctrl-y`、`z` familyでviewportを縦横に移動・配置します。
- `/`と`?`でsmart-caseの前方 / 後方検索を開始します。`n` / `N`はcount対応で、`*` / `#`はcursor位置のword、`g*` / `g#`は部分一致を検索します。
- `m{char}`でmarkを設定し、`'{char}`で最初の非空白、`` `{char}``で正確な列へjumpします。`g`を前置するとjump listを維持し、`['` / `` [` ``と`]'` / `` ]` ``で前後の小文字markを探します。Code file間でも利用できます。
- `Enter`は次行の最初の非空白へ移動します。`q`でwindowを即座に閉じ、Codeの2ペイン表示へ戻ります。`Esc`は検索強調を先に解除し、次のEscで閉じます。

認識できるソース種別には組み込みのsyntax定義を使います。読み取りは8 MiBが上限で、それを超えるtextは無制限に保持せずtruncated表示になります。

## コード閲覧中に検索する

Code viewerでもGit画面と同じく、`\f`でファイルパス、`\g`で固定文字列を検索できます。結果を開くとCodeへ直接戻り、選択ファイルが見えるまで親ディレクトリを展開して、現在内容を読み込みます。内容検索では現在行マーカーを一致行へ配置します。

## LSPでsymbol間を移動する

language server navigationは標準では無効です。信頼できるリポジトリでのみprofileを明示します。

```sh
chronogit --view code --lsp rust-analyzer
chronogit --view code --lsp jdtls
chronogit --view code --lsp pyright
chronogit --view code --lsp rust-analyzer --lsp jdtls --lsp pyright
```

ChronoGitは実行ファイルをinstallしません。[rust-analyzer](https://rust-analyzer.github.io/book/installation.html)、[Eclipse JDT LS](https://github.com/eclipse-jdtls/eclipse.jdt.ls)、[Pyright](https://github.com/microsoft/pyright)、[basedpyright](https://docs.basedpyright.com/latest/installation/)、[Python LSP Server](https://github.com/python-lsp/python-lsp-server)の公式手順で導入してください。組み込みprofileは`rust-analyzer`、隔離されたconfiguration/data directoryを使う`jdtls` wrapper、`pyright-langserver --stdio`、`basedpyright-langserver --stdio`、`pylsp`を使い、現在のJDT LSの実行にはJava 21以降が必要です。2つのPython serverなど、同じ拡張子を複数の有効profileが担当する場合、暗黙選択せずambiguity noticeを表示します。

`h` / `l`または左右矢印でsymbol上へcursorを置き、次を使います。

- `K`: hover解説を開く / 閉じる
- `gd`: 定義
- `gi`: 実装
- `gy`: 型定義
- `gD`: 宣言
- `[count]Ctrl-o`: 古いVim / LSP jump位置へ戻る
- `[count]Ctrl-i`: 新しいVim / LSP jump位置へ進む

hoverはフロートウィンドウに表示します。`j` / `k`で説明文をスクロールし、`K`、`q`、`Esc`のいずれかで閉じます。hoverと各navigationが利用できるかはinitialize後のserver capabilityで決めます。serverがindex中は一時的に情報なしを返す場合があるため、フロートを閉じ、index完了後に同じ操作を再実行します。navigation結果が1件なら直接開き、複数なら`j` / `k`で選ぶlistを開きます。`Enter`で開き、`q` / `Esc`でlistを閉じます。LSP target、長距離Vim motion、mark jump、検索は1つのjump listを共有し、`Ctrl-o`後の新しいjumpは新しい側の分岐を破棄します。端末が`Ctrl-i`とTabを区別できない場合、Tabも同じ「進む」操作になります。0件、未対応capability、起動失敗、timeout、crashは終了せずnoticeになります。

対象は完全なUTF-8のcurrent working-tree fileだけです。repository内の`file:`結果だけをrooted no-follow readerで開きます。repository外のstandard library/dependencyと`jdt:`などのvirtual URIはunsupportedとして表示し、repository pathとして解釈しません。

profileごとのroot markerを最も近い親から探し、なければrepository rootを使います。sessionはprofileとworkspace rootの組でlazy startし、最大4 processです。5つ目は最終利用が最も古いsessionを終了します。Rust、Java、Python、user定義languageはすべて同じ標準request経路を使います。

## language serverの配置とロード手順

language server実行ファイルとChronoGitのprofile fileは別物です。

1. serverはChronoGitの外へinstallします。profileに書かれた実行ファイルを`PATH`へ置くか、trusted user-levelの`lsp.toml`で絶対pathへ上書きします。
2. 組み込みprofile IDをそのまま使うか、`$XDG_CONFIG_HOME/chronogit/lsp.toml`（通常は`~/.config/chronogit/lsp.toml`）へcustom/上書きprofileを置きます。repository-localなserver commandは読みません。
3. 1つ以上の`--lsp PROFILE`を付けてChronoGitを起動します。この時点では組み込み値とuser設定を検証し、明示したprofileだけを保持しますが、server processはまだ起動しません。
4. 対応するsource fileへフォーカスしてhoverまたはnavigationを実行すると、extensionから有効profileを1つ選び、最寄りのroot markerを探します。そのworkspaceをcurrent directoryとしてprocessを起動し、LSP `initialize`、表示中fileの`didOpen`/`didChange`、capability確認、request送信の順に進みます。
5. 同じ`(profile, workspace root)`への次回requestはsessionを再利用します。終了時には常駐sessionをshutdownします。JDT LSが必要とする書き込み可能なworkspace dataはrepository外の一時directoryへ隔離します。

例えば`rust-analyzer`が`PATH`にあれば`lsp.toml`は不要で、`--lsp rust-analyzer`が組み込みcommandを選びます。別の場所にある場合は同梱の`config/lsp.toml`から該当tableをXDG fileへコピーし、`command`の先頭を絶対pathへ置き換えます。Javaの`jdtls`、Pythonの`pyright-langserver`、`basedpyright-langserver`、`pylsp`も同じworkflowです。
