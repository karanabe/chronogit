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

閲覧操作は読み取り専用ですが、ブランチ切替はワークツリーを更新します。この確認には使い捨てリポジトリを使用してください。

## Pane focus操作と入力byte

ChronoGit revision、OS、terminal、multiplexer（使用しない場合も含む）、画面寸法、有効なkeymapを記録します。ChronoGitを起動する前に、物理`Ctrl-h`、`Ctrl-j`、`Ctrl-k`、`Ctrl-l`、Backspaceの順で5つの入力byteを記録します。

```sh title="ターミナル"
pane_stty=$(stty -g)
trap 'stty "$pane_stty"' 0 1 2 15
stty raw -echo
od -An -t x1 -N 5
stty "$pane_stty"
trap - 0 1 2 15
```

区別できる一般的な結果は`08 0a 0b 0c 7f`です。Backspaceも`08`の場合、その環境では物理Backspaceと`Ctrl-h`を区別できないと記録し、両動作を個別に確認済みとは扱わないでください。

1. 組み込みkeymapで、Changes、History / Commit details、Graph / Graph details、File history、Codeの各画面において単独controlキーを1つずつ押し、focus元とfocus先を記録します。`Ctrl-h`と`Ctrl-k`がそれぞれ前、`Ctrl-j`と`Ctrl-l`がそれぞれ次へ移り、すべての端で停止し、Graph本体は単一paneに留まることを確認します。
2. Changesの80〜109列と110列以上で繰り返します。狭幅ではfocusとともに表示paneが切り替わり、広幅ではfocus borderだけが変わることを確認します。対応最小size未満でもfocus commandがChronoGitの外へ出ないことを確認します。
3. 代表的な移動を`Ctrl-w h/k/j/l`と文書化されたaliasでも繰り返します。無修飾`h/j/k/l`、矢印、別eventとして届くBackspace、残るcontrol motion aliasがfocus中の内容を従来どおり移動することを確認します。
4. リポジトリのSearch入力、Results、文書内検索入力、find/till待ち、mark待ち、Help、利用できる各単一text/hover/target overlayで、単独4キーを試します。入力と最前面overlayの動作が優先されることを確認します。リポジトリResultsでは、いずれの前操作もqueryを維持してSearchへ戻り、いずれの次操作もResultsに留まります。
5. `focus_previous`だけ、`focus_next`だけ、両方を置換する一時keymapで起動します。一部の単独controlだけを残す場合と、4キーすべてを別のキーへ移して残す`Ctrl-w` aliasを再列挙する場合を含めます。外した標準キーがfocusせず、重複・prefix競合はalternate screen開始前に失敗することを確認します。
6. session前後の`git status --short`、`git diff`、index、`HEAD`を比較し、ChronoGitがどれも変更していないことを確認します。

## Changesワークフロー

1. `chronogit /absolute/path/to/test-repository --view changes`を実行します。
2. 枠線、矢印、Unicodeファイル名が安定した列幅で表示されることを確認します。
3. 追加、削除、hunk、header、metadataの行を視覚的に区別できることを確認します。短いまたはほぼ空の追加・削除行とhunk行で、既存の背景がdiff contentの右端まで続き、枠の手前で止まることを確認します。context、header、metadata行に新しい背景が付いてはいけません。
4. `j`と`k`でファイルを素早く移動し、最後に表示される差分が最終選択と一致することを確認します。
   - diffの読み込み後に`Space v`を押し、対応するnew側の行でworking-tree file全文が開くことを確認します。Changesで追加行が強調され、削除行が元の位置へ赤色で挿入されることを確認します。`Space d`で通常のnew-state表示へ切り替えると削除行が消え、source位置は変わらないこと、`--lsp`なしの`Space s`はchooserを開かずdisabled noticeになることを確認します。
5. 140×40以上から約90×24へresizeします。複数paneがフォーカス中の1paneになり、単独`Ctrl-h/k/j/l`が文書どおりの各方向へfocusを変えること、diff行の背景が現在のcontent右端に追従して枠や隣接paneへ入らないことを確認します。従来の`Ctrl-w`形式でも繰り返します。
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
2. 140×40と80×24の両方で、commit、変更ファイル/ツリー、メッセージ全文が全幅の3段として表示され、長い件名とpathを読めることを確認します。
3. root、normal、merge commitを訪れ、footerとdiff titleが状況に応じて`empty tree`、`parent`、`first parent`を示すことを確認します。
4. commitペインにフォーカスした状態で`Enter`を押し、最下段がdiffへ切り替わり、選択中commitの変更ファイルへ直接フォーカスが移ることを確認します。`Ctrl-k`でCommitsへ戻ると、最下段も自動的にメッセージへ戻ることを確認します。続けて`Ctrl-l`または`Ctrl-w l`でペインを移動し、メッセージのままであることを確認します。
5. 変更されたtext/binary fileを選んで`Enter`を押し、大きなフロートでpatchまたはbinary summaryが開くことを確認します。textでは`Enter`が次行の最初の非空白へ移動することを確認し、`q`で閉じ、検索強調がない場合は`Esc`でも閉じます。
6. 種別を判別できるソースファイルを通常paneとfloating diffの両方で開きます。コードのトークンがシンタックスハイライトされ、追加・削除・hunkの背景がcontent右端まで続き、現在行のガターマーカーがコードの色を塗り替えないことを確認します。tab、wide character、長い行を含め、横スクロール後も本文が背景と独立して従来どおりclip・scrollすることを確認します。cacheされていない長いtext diffを開くと同時に`Ctrl-d`を押し、表示された直後にマーカーが半ページ移動していることを確認します。`j` / `k`でマーカーが1行ずつ目に見えて移動し、`Ctrl-u`も遅延なく上へ移動することを確認します。
7. countに加え、`w/W/e/E`、`b/B/ge/gE`、`0/^/$/g_`、`f/F/t/T`と`;` / `,`、`gg/G/%/go/H/M/L`、文・段落・section・delimiter motion、page/scroll/`z` motion、`[c` / `]c`を確認します。`/`、`?`、`n/N`、`*` / `#`、`g*` / `g#`も試します。
   - commit diffから`Space v`を押し、working treeではなく選択commitのfileが対応行で開くことを確認します。`Space d`を切り替えます。対応するtrusted LSP profileで`Space s`を使い、new側変更行を含むsymbolだけが並ぶこと、選択すると全文内のsymbolへjumpすること、symbolを選ばない全文行も使えることを確認します。
8. 差分オーバーレイを閉じ、`Space m`で最下段をメッセージ全文にしてフォーカスし、文字・word移動、スクロール、検索を確認します。`Space m`を2回押し、オーバーレイを開かず、フォーカスも変えずに差分とメッセージを切り替えられることを確認します。
9. `Space B`を押し、通常と同じcommit一覧、commit body、変更ファイルの3段を確認します。単独`Ctrl-h/k/j/l`を1つずつ使い、次に従来の`Ctrl-w`形式でもfocusを移します。上段のcommit変更時に残りの段が更新されることを確認し、bodyをscrollして下段ファイルのdiffを開きます。もう一度`Space B`を押して通常のHistoryへ戻ります。
10. `Space t`を押し、2階層のdirectoryを展開・折りたたみ、blobの差分を開きます。
11. `Ctrl-C`で終了し、`stty`比較とshell確認を繰り返します。

## Graphとリポジトリ検索

1. `Space 3`を押し、親レーンとコミット件名が見えることを確認します。`Space m`でメッセージ全文を開いて閉じます。
2. `Enter`を押し、まだ見えるGraphの上に枠付き2段ウィンドウが浮き、変更ファイルの下に選択差分があることを確認します。`Enter`で差分全文を開き、`q`で閉じ、もう一度`q`でGraphへ戻ります。`Esc`でも繰り返します。
3. Changes、History、Graphの各画面から`Space f`を実行し、既知のパスを1文字ずつ入力します。`Enter`前に結果が更新されることを確認し、`Enter`または入力中に予約された`Ctrl-j`でResultsへ移ります。Resultsから`Ctrl-h`、`Ctrl-k`、従来の`Ctrl-w k`を個別に使ってSearchへ戻り、維持されたクエリを編集してlive結果が再び更新されることを確認してから開きます。Resultsの`Ctrl-j/l`はResultsに留まることと、ファイル履歴の下に現在内容があることも確認します。
4. 履歴選択を変え、下段がそのコミットの差分へ切り替わることを確認します。差分全文を開いて閉じ、`q`または`Esc`で元のビューへ戻ります。
5. `Space g`で既知の文字列を入力し、編集と削除のたびにlive結果が追従することを確認します。結果を開き、現在内容の一致行が強調されることを確認します。promptを開き直し、Spaceと`jj`、`q`と大文字`Q`を含むクエリを入力して、すべてが挿入され結果が更新されることを確認します。`Esc`でpromptが閉じ、`Ctrl-C`で終了することも確認します。
6. 標準XDGキーマップと、有効なカスタム設定を`--keymap`へ渡した場合の両方で起動します。無効な明示ファイルはalternate screen開始前に失敗することを確認します。

## Codeワークフロー

1. `Space 4`を押し、追跡済みのルートファイルと折り畳まれたネストdirectoryがコードペインの上に表示されることを確認します。`--view code`で直接起動した場合も繰り返します。
2. ファイルへ移動し、現在のsyntax highlightされた内容が下段へ読み込まれることを確認します。複数ファイル間を素早く移動し、最後の内容が最後の選択と一致することを確認します。
3. directoryで`Enter`を押して2階層以上展開し、もう一度押すとすべての子孫が折り畳まれることを確認します。
4. 単独`Ctrl-h/k/j/l`を1つずつ使ってツリーとコード間を移動し、従来の`Ctrl-w`形式でも繰り返します。コードペインで完全なcount対応motion setと、短い行をまたぐ希望列の維持を確認します。
5. ツリーのファイルと下段の両方から`Enter`を押します。Code全文ウィンドウが開き、`Enter`は`+`と同様に移動し、検索が折り返し、`q`でCodeへ即座に戻り、`Esc`は検索強調があれば先に解除することを確認します。
6. language serverを有効にし、文字cursorをsymbolへ合わせます。`K`でhoverを開閉でき、`gd` / `gi` / `gy` / `gD`が4種類のsemantic targetを要求し、成功したjumpを`Ctrl-o` / `Ctrl-i`で前後移動できることを確認します。戻った後に新しいjumpを実行し、以前の進み先へ移動できなくなることも確認します。
   - フォーカス中のCode内容とそのfloatで`Space s`を押し、document-symbol listにsymbolを選ばない全文行があり、symbol選択で全文の該当位置へ移動し、`Space v`でも同じfileを直接開けることを確認します。file historyの現在内容でも繰り返します。
7. Codeから`Space f`と`Space g`を実行します。ネストした結果を開き、Codeへ直接戻ってツリー内のパスが展開され、内容一致行にmarkerが置かれることを確認します。小文字・大文字markを設定し、apostropheとbacktickでjumpし、file間を移動し、count付き`Ctrl-o` / `Ctrl-i`で統合履歴をたどります。
8. binary、symbolic link、削除済み追跡パス、8 MiBを超えるファイルを開きます。安全な要約またはtruncated markerが表示され、symbolic linkのtargetを読まないことを確認します。

## 検索強調の解除

版・revision、OS、端末、画面寸法、検索語、通常ペイン/フロート、キーマップを記録します。同じファイルと操作列で修正前後を比較し、報告時の環境を再現できなければ限界を残します。

1. Diff・Code・new-state全文表示の該当paneまたはfloatを140×40、80×24で試します。`/needle`、`Enter`、`n`、`Esc`と操作し、`?needle`と`N`でも繰り返します。Esc前後で周辺を読み、一致文字列だけに装飾が付き、現在と他の一致を見分けられることを確認します。行番号・余白に検索装飾がなく、syntax色・diffの追加/削除・カーソル・フォーカス・スクロール位置が保たれることも確認します。
2. 語を再入力せず`n` / `N`で再開し、countと折り返しを試します。再解除して別の検索を確定し、強調が戻ることを見ます。解除後の2回目のEscと強調中の`q`で従来のclose/backへ進むこと、一致なしで追加Escを要求しないことを比べます。
3. 直前の強調が表示中/解除済みの両方で、`/`・`?`入力とfind/till・markの文字待ちをEscで取り消します。入力だけを取り消すこと、最前面のhelp・リポジトリ検索（prompt/結果）を先に閉じることを確認します。hoverは既存のopt-in環境があれば確認し、未実施なら理由を残します。
4. 日本語・tab・一致した空白・同一行の複数一致を含め、一致の途中まで横スクロールします。見える一致範囲と装飾が対応し、周辺の本文を読み続けられることを確認します。
5. F1ヘルプと検索時のヒントを読みます。`close = x`、`close = q, esc`、`close = x`と`refresh = esc`の組み合わせも試し、割り当てと即時close・Esc再割り当ての結果を記録します。

各項目の結果と迷った点を記録してください。自動セル/キーテストやagentによる端末操作だけではmaintainerの見え方・利用確認済みとはしません。

## 空の文書内検索のキャンセル

80×24と140×40で、Code・Diff・現在のファイル本文・コミットメッセージの既存の通常ペインとフロートを、実端末のBackspaceキーで確認します。revision・端末・キーマップ・操作列と観察結果を記録します。

1. `/`、Backspace、`j` / `k`を試し、`?`でも繰り返します。入力カーソルが消え、フォーカス・本文カーソル・縦横のスクロールがキャンセル時に変わらず、その場で移動を再開できることを確認します。キャンセルにcloseや左移動が重ならないことも確認します。
2. `/a`、Backspace、別の語、Enterと、`/a`、Backspace、Backspaceを使い分けます。逆方向でも、日本語・空白・文字としての`/`・`?`を含めて試します。最後の文字の削除では入力を続け、次のBackspaceで終了することと、空promptのヒントを確認します。
3. 前回の検索強調が表示中・解除済みの両方で、同方向・逆方向のpromptを取り消します。位置・強調を比較し、`n` / `N`で再開します。状態欄が残っても入力カーソルは消えることを確認します。確定検索なし、Esc、空Enter、通常時のBackspaceとカスタムキーも試します。
4. リポジトリ検索の空クエリ・live編集・Search/Resultsの移動と、最前面のhelp・hover・候補一覧が従来どおり操作できることを確認します。

maintainerの利用結果は自動テストやagentによる端末操作とは分け、再入力や閲覧の再開で迷った点も記録します。

## サインオフ

自動テストだけでプラットフォームを完了扱いにしないでください。

| プラットフォーム | OS version | ターミナル | 色/Unicode/resize | `Q` cleanup | Ctrl-C cleanup | テスター/日付 |
| --- | --- | --- | --- | --- | --- | --- |
| Linux | | | | | | |
| macOS | | | | | | |
