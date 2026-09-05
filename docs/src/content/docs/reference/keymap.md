---
title: Keymap configuration
description: Change ChronoGit key bindings with the optional XDG keymap file.
tags:
  - keyboard
  - configuration
  - reference
sidebar:
  order: 2
---

ChronoGit uses its built-in bindings when no configuration file exists. To override individual actions, copy the packaged `config/keymap.conf` to `$XDG_CONFIG_HOME/chronogit/keymap.conf` or `~/.config/chronogit/keymap.conf`. Use `chronogit --keymap PATH` to select another file explicitly.

```ini
[bindings]
show_graph = x
show_code = c
file_search = alt-p
content_search = alt-s
# close = q, esc
quit = Q
```

Each `action = keys` line replaces all defaults for that action. Separate a sequence with spaces and alternatives with commas. Supported names are single characters, `space`, `comma`, `enter`, `esc`, `backspace`, `tab`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`, `f1` through `f255`, and combinations of the `ctrl-`, `alt-`, or `shift-` prefixes. Ordinary sequences expire after 750 ms. An action that requires a character argument—find/till or a mark command—waits until that argument or `Esc` arrives.

Unmodified `1` through `9` are reserved for counts and cannot start a binding. Use a leader sequence such as `\ 3` or a modifier such as `alt-3`. Use `comma` to bind the comma key, since a literal comma separates alternatives.

| Action names | Purpose |
| --- | --- |
| `quit`, `show_changes`, `show_history`, `show_graph`, `show_code` | Application and view selection |
| `focus_previous`, `focus_next` | Pane focus |
| `move_up`, `move_down`, `move_top`, `move_bottom`, `move_bottom_end`, `cursor_left`, `cursor_right`, `cursor_left_wrap`, `cursor_right_wrap` | Basic line, list, buffer, and character movement |
| `line_start`, `first_non_blank`, `line_end`, `last_non_blank` | Logical-line columns |
| `screen_line_start`, `screen_first_non_blank`, `screen_line_end`, `screen_last_non_blank`, `screen_middle`, `line_middle`, `column`, `byte_offset`, `buffer_percentage` | Screen, requested-column, byte, and percentage movement |
| `word_forward`, `word_backward`, `word_end_forward`, `word_end_backward` | `w`, `b`, `e`, and `ge` word motions |
| `big_word_forward`, `big_word_backward`, `big_word_end_forward`, `big_word_end_backward` | Whitespace-delimited WORD motions |
| `find_forward`, `find_backward`, `till_forward`, `till_backward`, `repeat_character_search`, `reverse_character_search` | `f`, `F`, `t`, `T`, `;`, and `,` |
| `previous_line_first_non_blank`, `next_line_first_non_blank`, `counted_line_first_non_blank` | `-`, `+`, and `_` |
| `sentence_forward`, `sentence_backward`, `paragraph_forward`, `paragraph_backward` | Sentence and paragraph boundaries |
| `section_start_backward`, `section_start_forward`, `section_end_backward`, `section_end_forward` | Section boundaries |
| `matching_pair`, `matching_pair_backward`, `unmatched_paren_backward`, `unmatched_brace_backward`, `unmatched_paren_forward`, `unmatched_brace_forward` | Forward/reverse matching and unmatched delimiters |
| `method_start_backward`, `method_end_backward`, `method_start_forward`, `method_end_forward` | Method-brace motions |
| `preprocessor_backward`, `preprocessor_forward`, `comment_backward`, `comment_forward` | Preprocessor and C-comment boundaries |
| `window_top`, `window_middle`, `window_bottom`, `previous_diff_change`, `next_diff_change` | Visible-window lines and diff change blocks |
| `half_page_up`, `half_page_down`, `page_up`, `page_down`, `scroll_line_up`, `scroll_line_down`, `scroll_left`, `scroll_right` | Vertical and horizontal scrolling |
| `cursor_to_window_top`, `cursor_to_window_middle`, `cursor_to_window_bottom` | Column-preserving `zt`, `zz`, and `zb` viewport positioning |
| `cursor_to_window_top_first_non_blank`, `cursor_to_window_middle_first_non_blank`, `cursor_to_window_bottom_first_non_blank` | First-nonblank `z<CR>`, `z.`, and `z-` viewport positioning |
| `next_window_top`, `previous_window_bottom` | `z+` and `z^` viewport positioning |
| `scroll_half_screen_left`, `scroll_half_screen_right`, `cursor_to_window_left`, `cursor_to_window_right` | Extended horizontal scrolling |
| `set_mark`, `jump_mark_line`, `jump_mark_exact`, `jump_mark_line_without_history`, `jump_mark_exact_without_history` | Read a following character and set/jump to a Code mark, optionally preserving jump history |
| `previous_mark_line`, `previous_mark_exact`, `next_mark_line`, `next_mark_exact` | Count-aware previous/next lowercase Code mark scans |
| `lsp_hover` | Toggle hover information at the Code cursor (`K`) |
| `go_to_definition`, `go_to_implementation` | LSP definition / implementation (`gd` / `gi`) |
| `go_to_type_definition`, `go_to_declaration` | LSP type definition / declaration (`gy` / `gD`) |
| `semantic_back`, `semantic_forward` | Older / newer shared Vim/LSP jump location (`Ctrl-o` / `Ctrl-i`; Tab is the terminal-compatible forward alias) |
| `refresh`, `activate`, `close` | Current view operations |
| `toggle_message`, `toggle_details`, `toggle_tree`, `toggle_help` | History and help views |
| `file_search`, `content_search` | Repository-wide search |
| `search_forward`, `search_backward`, `next_match`, `previous_match` | Prompt search and count-aware repetition in the active text document |
| `search_word_forward`, `search_word_backward`, `search_partial_word_forward`, `search_partial_word_backward` | `*`, `#`, `g*`, and `g#` word-derived searches |

ChronoGit rejects an unknown action/key, an unreadable explicit file, duplicate keys, and a binding that is a prefix of another binding. These errors are reported before terminal raw mode starts. By default, `q` closes/backs immediately and `Esc` first dismisses active Diff/Code search highlights before close/back, while `quit` uses uppercase `Q`; `Ctrl-C` is always reserved as an emergency safe-exit binding even when `quit` is replaced. Query editing reserves `Enter`, `Ctrl-j`, `Ctrl-k`, `Esc`, Backspace, and `Ctrl-C`; printable `q` and uppercase `Q` remain available as query text. In-app help describes the built-in defaults, not custom bindings.

An explicit `close` assignment replaces both default `q` and `Esc` bindings. Every assigned key closes/backs immediately: even `close = q, esc` makes Esc close without dismissing highlights first. With `close = x`, default Esc is removed and can be bound to another action. Leave `close` unset to retain the default two-step Esc. Prompt and character-wait cancellation by Esc remains reserved regardless of configuration.

Document-search prompts reserve Backspace for deletion, or cancellation when already empty, regardless of custom normal-mode bindings. Deleting the last character leaves the prompt open for replacement input. Esc still cancels immediately, and empty Enter reuses the previous query. Repository-search Backspace continues editing its live query.
