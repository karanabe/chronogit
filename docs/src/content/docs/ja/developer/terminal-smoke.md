---
title: 手動ターミナルスモークテスト
description: LinuxとmacOSで描画、操作、ターミナル復元を検証します。
tags:
  - テスト
  - ターミナル
  - チェックリスト
sidebar:
  order: 3
---

リリースへ署名する前に、LinuxとmacOSの両方でこのチェックリストを実行します。キャプチャされたCIコマンドではなく、カラー対応・UTF-8 localeのターミナルを使います。結果とともにターミナルアプリとOS versionを記録してください。

## 準備

クリーンなChronoGitチェックアウトで実行します。

```sh title="ターミナル"
cargo install --path . --locked
before_stty=$(stty -g)
printf 'locale=%s term=%s\n' "${LC_ALL:-${LANG:-unset}}" "${TERM:-unset}"
```

次を含むbareではないテストリポジトリを選びます。

- 追加行と削除行のある未ステージのテキスト変更
- Unicodeファイル名とUnicode内容
- root、normal、merge commitを少なくとも1つずつ
- バイナリファイルを変更するcommit
- 2階層以上のディレクトリ

ChronoGitは読み取り専用ですが、fixtureを安全に調整しやすい使い捨てリポジトリを推奨します。

## Changesワークフロー

1. `chronogit /absolute/path/to/test-repository --view changes`を実行します。
2. 枠線、矢印、Unicodeファイル名が安定した列幅で表示されることを確認します。
3. 追加、削除、hunk、header、metadataの行を視覚的に区別できることを確認します。
4. `j`と`k`でファイルを素早く移動し、最後に表示される差分が最終選択と一致することを確認します。
5. 140×40以上から約90×24へresizeします。複数ペインがフォーカス中の1ペインになり、`h`/`l`でフォーカスが変わることを確認します。
6. 80×24未満へresizeします。最小サイズの案内と終了ヒントがcrashなしで表示されることを確認し、元に戻します。
7. `F1`でhelpを開き、`q`で閉じてから、大文字の`Q`で終了します。

シェルへ戻った後に実行します。

```sh title="ターミナル"
test "$(stty -g)" = "$before_stty"
printf 'terminal accepts normal input after Q\n'
```

入力が通常どおりechoされ、cursorが表示され、mouse selectionが動作し、以前の画面内容が復元されたことを確認します。

## Historyワークフロー

1. `chronogit /absolute/path/to/test-repository --view history`を実行します。
2. 140×40と80×24の両方で、commit、変更ファイル/ツリー、diffが全幅の3段として表示され、長い件名とpathを読めることを確認します。
3. root、normal、merge commitを訪れ、footerとdiff titleが状況に応じて`empty tree`、`parent`、`first parent`を示すことを確認します。
4. commitペインにフォーカスした状態で`Enter`を押し、選択中commitの変更ファイルへ直接フォーカスが移ることを確認します。
5. 変更されたtext/binary fileを選んで`Enter`を押し、大きなフロートでpatchまたはbinary summaryが開くことを確認します。`q`、`Esc`、`Enter`のそれぞれで閉じることを確認します。
6. 種別を判別できるソースファイルを開き、コードのトークンがシンタックスハイライトされ、追加・削除に控えめな緑・赤の背景が付き、現在行のガターマーカーがコードの色を塗り替えないことを確認します。cacheされていない長いtext diffを開くと同時に`Ctrl-d`を押し、表示された直後にマーカーが半ページ移動していることを確認します。`j` / `k`でマーカーが1行ずつ目に見えて移動し、`Ctrl-u`も遅延なく上へ移動することを確認します。
7. 長いtext diffで`gg` / `G`により先頭/末尾へ移動します。`/`で前方、`?`で後方検索し、`n` / `N`が強調された一致を折り返すことを確認します。
8. `m`を押し、commit message全文のoverlayを`j` / `k`でscrollし、`m`、`q`、`Esc`のそれぞれで閉じることを確認します。
9. `b`を押し、通常と同じcommit一覧、commit body、変更ファイルの3段を確認します。`Ctrl-j` / `Ctrl-k`でフォーカスを移し、上段のcommit変更時に残りの段が更新されることを確認し、bodyをscrollして下段ファイルのdiffを開きます。もう一度`b`を押して通常のHistoryへ戻ります。
10. `t`を押し、2階層のdirectoryを展開・折りたたみ、blobの差分を開きます。
11. `Ctrl-C`で終了し、`stty`比較とshell確認を繰り返します。

## Graphとリポジトリ検索

1. `3`を押し、親レーンとコミット件名が見えることを確認します。`m`でメッセージ全文を開いて閉じます。
2. `Enter`を押し、まだ見えるGraphの上に枠付き2段ウィンドウが浮き、変更ファイルの下に選択差分があることを確認します。`Enter`で差分全文を開き、`q`で閉じ、もう一度`q`でGraphへ戻ります。`Esc`でも繰り返します。
3. Changes、History、Graphの各画面から`Space f`を実行し、既知のパスを1文字ずつ入力します。`Enter`前に結果が更新されることを確認し、`Enter`または`Ctrl-j`でResultsへ移ってから`Ctrl-k`でSearchへ戻ります。クエリを編集してlive結果が再び更新されることを確認してから開き、ファイル履歴の下に現在内容があることを確認します。
4. 履歴選択を変え、下段がそのコミットの差分へ切り替わることを確認します。差分全文を開いて閉じ、`q`または`Esc`で元のビューへ戻ります。
5. `Space g`で既知の文字列を入力し、編集と削除のたびにlive結果が追従することを確認します。結果を開き、現在内容の一致行が強調されることを確認します。promptを開き直して`q`と大文字`Q`の両方を含むクエリを入力し、どちらも挿入されて結果が更新されることを確認します。`Esc`でpromptが閉じ、`Ctrl-C`で終了することも確認します。
6. 標準XDGキーマップと、有効なカスタム設定を`--keymap`へ渡した場合の両方で起動します。無効な明示ファイルはalternate screen開始前に失敗することを確認します。

## Codeワークフロー

1. `4`を押し、追跡済みのルートファイルと折り畳まれたネストdirectoryがコードペインの上に表示されることを確認します。`--view code`で直接起動した場合も繰り返します。
2. ファイルへ移動し、現在のsyntax highlightされた内容が下段へ読み込まれることを確認します。複数ファイル間を素早く移動し、最後の内容が最後の選択と一致することを確認します。
3. directoryで`Enter`を押して2階層以上展開し、もう一度押すとすべての子孫が折り畳まれることを確認します。
4. `h` / `l`と`Ctrl-k` / `Ctrl-j`の両方でツリーとコード間を移動します。コードペインで`j` / `k`、`gg` / `G`、`Ctrl-u` / `Ctrl-d`、`zh` / `zl`を試します。
5. ツリーのファイルと下段の両方から`Enter`を押します。Code全文ウィンドウが開き、`/` / `?`と`n` / `N`が折り返し検索を行い、`Enter`、`q`、`Esc`のそれぞれでCodeへ戻ることを確認します。
6. language serverを有効にし、文字cursorをsymbolへ合わせます。`K`でhoverを開閉でき、`gd` / `gi` / `gy` / `gD`が4種類のsemantic targetを要求し、成功したjumpを`Ctrl-o` / `Ctrl-i`で前後移動できることを確認します。戻った後に新しいjumpを実行し、以前の進み先へ移動できなくなることも確認します。
7. Codeから`Space f`と`Space g`を実行します。ネストした結果を開き、Codeへ直接戻ってツリー内のパスが展開され、内容一致行にmarkerが置かれることを確認します。
8. binary、symbolic link、削除済み追跡パス、8 MiBを超えるファイルを開きます。安全な要約またはtruncated markerが表示され、symbolic linkのtargetを読まないことを確認します。

## サインオフ

自動テストだけでプラットフォームを完了扱いにしないでください。

| プラットフォーム | OS version | ターミナル | 色/Unicode/resize | `Q` cleanup | Ctrl-C cleanup | テスター/日付 |
| --- | --- | --- | --- | --- | --- | --- |
| Linux | | | | | | |
| macOS | | | | | | |
