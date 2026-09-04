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
file_search = alt-p
content_search = alt-s
close = q, esc
quit = Q
```

各`action = keys`行は、そのactionの標準割り当てをすべて置き換えます。連続キーは空白、代替キーはカンマで区切ります。単一文字、`space`、`comma`、`enter`、`esc`、`backspace`、`tab`、`up`、`down`、`left`、`right`、`home`、`end`、`pageup`、`pagedown`、`f1`〜`f255`と、`ctrl-`、`alt-`、`shift-`の組み合わせを使えます。通常の連続キーは750 msで期限切れになります。find/tillまたはmarkのように文字引数が必要なactionは、その文字か`Esc`が入力されるまで待ちます。

修飾キーなしの`1`〜`9`はcount専用で、割り当ての先頭には使えません。`\ 3`のようなleader連続キー、または`alt-3`のような修飾キーを使います。カンマ自体を割り当てるには、代替キーの区切りと区別できる`comma`を使います。

| action名 | 用途 |
| --- | --- |
| `quit`, `show_changes`, `show_history`, `show_graph`, `show_code` | アプリとビューの選択 |
| `focus_previous`, `focus_next` | ペインフォーカス |
| `move_up`, `move_down`, `move_top`, `move_bottom`, `move_bottom_end`, `cursor_left`, `cursor_right`, `cursor_left_wrap`, `cursor_right_wrap` | 基本的な行・list・buffer・文字移動 |
| `line_start`, `first_non_blank`, `line_end`, `last_non_blank` | 論理行内の列移動 |
| `screen_line_start`, `screen_first_non_blank`, `screen_line_end`, `screen_last_non_blank`, `screen_middle`, `line_middle`, `column`, `byte_offset`, `buffer_percentage` | 画面・指定列・byte・割合移動 |
| `word_forward`, `word_backward`, `word_end_forward`, `word_end_backward` | `w`、`b`、`e`、`ge`のword motion |
| `big_word_forward`, `big_word_backward`, `big_word_end_forward`, `big_word_end_backward` | 空白区切りWORD motion |
| `find_forward`, `find_backward`, `till_forward`, `till_backward`, `repeat_character_search`, `reverse_character_search` | `f`、`F`、`t`、`T`、`;`、`,` |
| `previous_line_first_non_blank`, `next_line_first_non_blank`, `counted_line_first_non_blank` | `-`、`+`、`_` |
| `sentence_forward`, `sentence_backward`, `paragraph_forward`, `paragraph_backward` | 文・段落境界 |
| `section_start_backward`, `section_start_forward`, `section_end_backward`, `section_end_forward` | section境界 |
| `matching_pair`, `matching_pair_backward`, `unmatched_paren_backward`, `unmatched_brace_backward`, `unmatched_paren_forward`, `unmatched_brace_forward` | 正方向/逆方向の対応pairと未対応delimiter |
| `method_start_backward`, `method_end_backward`, `method_start_forward`, `method_end_forward` | method brace motion |
| `preprocessor_backward`, `preprocessor_forward`, `comment_backward`, `comment_forward` | preprocessor・C comment境界 |
| `window_top`, `window_middle`, `window_bottom`, `previous_diff_change`, `next_diff_change` | 表示window行・diff変更ブロック |
| `half_page_up`, `half_page_down`, `page_up`, `page_down`, `scroll_line_up`, `scroll_line_down`, `scroll_left`, `scroll_right` | 縦・横scroll |
| `cursor_to_window_top`, `cursor_to_window_middle`, `cursor_to_window_bottom` | 列を維持する`zt`、`zz`、`zb`のviewport配置 |
| `cursor_to_window_top_first_non_blank`, `cursor_to_window_middle_first_non_blank`, `cursor_to_window_bottom_first_non_blank` | 最初の非空白へ移る`z<CR>`、`z.`、`z-`のviewport配置 |
| `next_window_top`, `previous_window_bottom` | `z+`と`z^`のviewport配置 |
| `scroll_half_screen_left`, `scroll_half_screen_right`, `cursor_to_window_left`, `cursor_to_window_right` | 拡張横scroll |
| `set_mark`, `jump_mark_line`, `jump_mark_exact`, `jump_mark_line_without_history`, `jump_mark_exact_without_history` | 次の文字を読み、jump historyの更新有無を選んでCode markを設定 / jump |
| `previous_mark_line`, `previous_mark_exact`, `next_mark_line`, `next_mark_exact` | count対応の前 / 次の小文字Code mark検索 |
| `lsp_hover` | Code cursor位置のhover解説を開閉（`K`） |
| `go_to_definition`, `go_to_implementation` | LSPの定義 / 実装（`gd` / `gi`） |
| `go_to_type_definition`, `go_to_declaration` | LSPの型定義 / 宣言（`gy` / `gD`） |
| `semantic_back`, `semantic_forward` | 古い / 新しい共有Vim・LSP jump位置へ移動（`Ctrl-o` / `Ctrl-i`。Tabは端末互換alias） |
| `refresh`, `activate`, `close` | 現在ビューの操作 |
| `toggle_message`, `toggle_details`, `toggle_tree`, `toggle_help` | 履歴とヘルプ表示 |
| `file_search`, `content_search` | リポジトリ全体の検索 |
| `search_forward`, `search_backward`, `next_match`, `previous_match` | アクティブなテキスト内のprompt検索とcount付き反復 |
| `search_word_forward`, `search_word_backward`, `search_partial_word_forward`, `search_partial_word_backward` | `*`、`#`、`g*`、`g#`のword由来検索 |

未知のaction/キー、読めない明示ファイル、キーの重複、別の割り当てのprefixになる割り当ては拒否し、raw mode開始前にエラーを表示します。標準では`close`が`q`と`Esc`、`quit`が大文字の`Q`です。`quit`を置き換えても、安全な緊急終了用の`Ctrl-C`は予約されたままです。文字入力中は`Enter`、`Ctrl-j`、`Ctrl-k`、`Esc`、Backspace、`Ctrl-C`が予約され、印字可能な`q`と大文字`Q`はクエリ文字として入力できます。アプリ内ヘルプはカスタム設定ではなく組み込み標準キーを表示します。
