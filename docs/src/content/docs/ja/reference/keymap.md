---
title: キーマップ設定
description: 任意のXDGキーマップファイルでChronoGitのキー割り当てを変更します。
tags:
  - キーボード
  - 設定
  - リファレンス
sidebar:
  order: 2
---

設定ファイルがない場合、ChronoGitは組み込みキーを使います。一部のactionを変更するには、同梱の`config/keymap.conf`を`$XDG_CONFIG_HOME/chronogit/keymap.conf`または`~/.config/chronogit/keymap.conf`へコピーします。別のファイルを明示する場合は`chronogit --keymap PATH`を使います。

```ini
[bindings]
show_graph = x
show_code = c
file_search = ctrl-p
content_search = space s
close = q, esc
quit = Q
```

各`action = keys`行は、そのactionの標準割り当てをすべて置き換えます。連続キーは空白、代替キーはカンマで区切ります。単一文字、`space`、`enter`、`esc`、`backspace`、`tab`、`up`、`down`、`left`、`right`、`home`、`end`、`f1`〜`f255`と、それらに`ctrl-`または`alt-`を付けた名前を使えます。連続キーは750 msで期限切れになります。

| action名 | 用途 |
| --- | --- |
| `quit`, `show_changes`, `show_history`, `show_graph`, `show_code` | アプリとビューの選択 |
| `focus_previous`, `focus_next` | ペインフォーカス |
| `move_up`, `move_down`, `move_top`, `move_bottom` | 選択またはカーソル移動（`j` / `k`、`gg` / `G`） |
| `half_page_up`, `half_page_down`, `scroll_left`, `scroll_right` | 表示範囲の移動 |
| `cursor_left`, `cursor_right` | Code focus中はcursor移動（`h` / `l`とLeft / Right）、それ以外はpane移動 |
| `lsp_hover` | Code cursor位置のhover解説を開閉（`K`） |
| `go_to_definition`, `go_to_implementation` | LSPの定義 / 実装（`gd` / `gi`） |
| `go_to_type_definition`, `go_to_declaration` | LSPの型定義 / 宣言（`gy` / `gD`） |
| `semantic_back`, `semantic_forward` | 古い / 新しいsemantic locationへ移動（`Ctrl-o` / `Ctrl-i`。Tabは端末互換のforward alias） |
| `refresh`, `activate`, `close` | 現在ビューの操作 |
| `toggle_message`, `toggle_details`, `toggle_tree`, `toggle_help` | 履歴とヘルプ表示 |
| `file_search`, `content_search` | リポジトリ全体の検索 |
| `search_forward`, `search_backward`, `next_match`, `previous_match` | 差分またはCodeファイルのフロート内検索 |

未知のaction/キー、読めない明示ファイル、キーの重複、別の割り当てのprefixになる割り当ては拒否し、raw mode開始前にエラーを表示します。標準では`close`が`q`と`Esc`、`quit`が大文字の`Q`です。`quit`を置き換えても、安全な緊急終了用の`Ctrl-C`は予約されたままです。文字入力中は`Enter`、`Ctrl-j`、`Ctrl-k`、`Esc`、Backspace、`Ctrl-C`が予約され、印字可能な`q`と大文字`Q`はクエリ文字として入力できます。アプリ内ヘルプはカスタム設定ではなく組み込み標準キーを表示します。
