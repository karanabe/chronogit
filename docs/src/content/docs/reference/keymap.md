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
file_search = ctrl-p
content_search = space s
close = q, esc
quit = Q
```

Each `action = keys` line replaces all defaults for that action. Separate a sequence with spaces and alternatives with commas. Supported names are single characters, `space`, `enter`, `esc`, `backspace`, `up`, `down`, `left`, `right`, `home`, `end`, `f1` through `f255`, and `ctrl-` or `alt-` plus one of those keys. Sequences expire after 750 ms.

| Action names | Purpose |
| --- | --- |
| `quit`, `show_changes`, `show_history`, `show_graph`, `show_code` | Application and view selection |
| `focus_previous`, `focus_next` | Pane focus |
| `move_up`, `move_down`, `move_top`, `move_bottom` | Selection or cursor movement |
| `half_page_up`, `half_page_down`, `scroll_left`, `scroll_right` | Viewport movement |
| `refresh`, `activate`, `close` | Current view operations |
| `toggle_message`, `toggle_details`, `toggle_tree`, `toggle_help` | History and help views |
| `file_search`, `content_search` | Repository-wide search |
| `search_forward`, `search_backward`, `next_match`, `previous_match` | Search inside a floating diff or Code file |

ChronoGit rejects an unknown action/key, an unreadable explicit file, duplicate keys, and a binding that is a prefix of another binding. These errors are reported before terminal raw mode starts. By default, `close` uses `q` and `Esc`, while `quit` uses uppercase `Q`; `Ctrl-C` is always reserved as an emergency safe-exit binding even when `quit` is replaced. Query editing reserves `Enter`, `Ctrl-j`, `Ctrl-k`, `Esc`, Backspace, and `Ctrl-C`; printable `q` and uppercase `Q` remain available as query text. In-app help describes the built-in defaults, not custom bindings.
