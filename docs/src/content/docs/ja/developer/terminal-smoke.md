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
7. `F1`でhelpを開閉し、`q`で終了します。

シェルへ戻った後に実行します。

```sh title="ターミナル"
test "$(stty -g)" = "$before_stty"
printf 'terminal accepts normal input after q\n'
```

入力が通常どおりechoされ、cursorが表示され、mouse selectionが動作し、以前の画面内容が復元されたことを確認します。

## Historyワークフロー

1. `chronogit /absolute/path/to/test-repository --view history`を実行します。
2. 140×40と80×24の両方で、commit、変更ファイル/ツリー、diffが全幅の3段として表示され、長い件名とpathを読めることを確認します。
3. root、normal、merge commitを訪れ、footerとdiff titleが状況に応じて`empty tree`、`parent`、`first parent`を示すことを確認します。
4. commitペインにフォーカスした状態で`Enter`を押し、選択中commitの変更ファイルへ直接フォーカスが移ることを確認します。
5. 変更されたtext/binary fileを選んで`Enter`を押し、大きなフロートでpatchまたはbinary summaryが開くことを確認します。もう一度`Enter`を押して閉じることを確認します。
6. cacheされていない長いtext diffを開くと同時に`Ctrl-d`を押し、表示された直後に強調中の現在行が半ページ移動していることを確認します。`j` / `k`で強調が1行ずつ目に見えて移動し、`Ctrl-u`も遅延なく上へ移動することを確認します。
7. 長いtext diffで`g` / `G`により先頭/末尾へ移動します。`/`で前方、`?`で後方検索し、`n` / `N`が強調された一致を折り返すことを確認します。
8. `m`を押し、commit message全文のoverlayを`j` / `k`でscrollし、`m`と`Esc`のそれぞれで閉じることを確認します。
9. `b`を押し、通常と同じcommit一覧、commit body、変更ファイルの3段を確認します。`Ctrl-j` / `Ctrl-k`でフォーカスを移し、上段のcommit変更時に残りの段が更新されることを確認し、bodyをscrollして下段ファイルのdiffを開きます。もう一度`b`を押して通常のHistoryへ戻ります。
10. `t`を押し、2階層のdirectoryを展開・折りたたみ、blobの差分を開きます。
11. `Ctrl-C`で終了し、`stty`比較とshell確認を繰り返します。

## Graphとリポジトリ検索

1. `3`を押し、親レーンとコミット件名が見えることを確認します。`m`でメッセージ全文を開いて閉じます。
2. `Enter`を押し、変更ファイルの下に選択差分があることを確認します。`Enter`で差分全文を開いて閉じ、`Esc`でGraphへ戻ります。
3. Changes、History、Graphの各画面から`Space f`を実行し、既知のパスを検索して開きます。ファイル履歴の下に現在内容があることを確認します。
4. 履歴選択を変え、下段がそのコミットの差分へ切り替わることを確認します。差分全文を開いて閉じ、`Esc`で元のビューへ戻ります。
5. `Space g`で既知の文字列を検索して結果を開き、現在内容の一致行が強調されることを確認します。
6. 標準XDGキーマップと、有効なカスタム設定を`--keymap`へ渡した場合の両方で起動します。無効な明示ファイルはalternate screen開始前に失敗することを確認します。

## サインオフ

自動テストだけでプラットフォームを完了扱いにしないでください。

| プラットフォーム | OS version | ターミナル | 色/Unicode/resize | `q` cleanup | Ctrl-C cleanup | テスター/日付 |
| --- | --- | --- | --- | --- | --- | --- |
| Linux | | | | | | |
| macOS | | | | | | |
