# Vim compatibility contract

This document fixes the comparison boundary for `vim-navigation` 0.2.0. It is
part of the source contract; a successful build alone is not compatibility
evidence.

## Reference and conditions

- Reference: Vim 9.1.1244, tag `v9.1.1244`, commit
  `35cb38d34b69e4263f0eb9f78a676a5fb6d11250` from the official
  [`vim/vim`](https://github.com/vim/vim/tree/v9.1.1244) repository.
- Documents: `runtime/doc/motion.txt`, `runtime/doc/scroll.txt`, and the `[c` /
  `]c` commands in `runtime/doc/diff.txt` at that tag.
- Options: `nocompatible`, `cpoptions&vim`, `nowrap`, `nofoldenable`, empty
  `virtualedit`, `startofline`, `scrolloff=0`, `sidescrolloff=0`, `tabstop=4`,
  UTF-8, and Unix line endings. The `LeftWrap` and `RightWrap` cases explicitly
  enable the corresponding `whichwrap=b` and `whichwrap=s` flags.
- Display: an 80-column terminal with a 23-line Vim window. ChronoGit's text
  views do not soft-wrap; therefore `gj` / `gk` are verified as `j` / `k`
  aliases under `nowrap`. Horizontal screen motions use the recorded
  `leftcol`. Positions are converted from Vim's one-based line/column result
  to zero-based lines and UTF-8 byte columns.
- Fixture: the checked-in `FIXTURE` in `tests/vim_oracle.rs`, containing empty
  lines, punctuation, sentences, paragraphs, sections, nested delimiters,
  methods, preprocessor branches, comments, diff-like blocks, a tab, Unicode,
  and a 120-column line. Two additional cases use a short sentence fixture with
  multiple closing punctuation characters.

The executable oracle is intentionally not a default test dependency. To run
the exact comparison, build the reference Vim tag with normal features and
run:

```sh
VIM_ORACLE=/path/to/vim \
  cargo test -p vim-navigation --test vim_oracle -- --ignored
```

The test rejects a binary whose `--version` is not Vim 9.1 with patches
1-1244. It requires `script(1)` and `stty(1)` on Linux or macOS, fixes an
80x24 pseudo-terminal, and sets the locale and terminal type for reproducibility. Set
`VIM_ORACLE_CASE` to part of a case name when diagnosing one row.

## Supported inventory and evidence

The table maps the library's vocabulary to Vim commands. Evidence establishes
agreement for the checked fixtures under the options above, not full Vim
equivalence for every buffer or sequence. Aliases share one `MotionKind`;
callers own their key bindings. See the implementation limits below.

| Family | Vim commands represented by the contract | Evidence | Status |
| --- | --- | --- | --- |
| Character and screen column | `h`, Left, `l`, Right, Backspace/Ctrl-H and Space with `whichwrap`, `0`, Home, `^`, `$`, End, `g_`, `g0`, `g^`, `gm`, `gM`, `g$`, `g<End>`, <code>\|</code>, `go` | executable Vim oracle and unit boundary tests | Exercised under the stated limits |
| Logical/display line and file | `k`, Up, Ctrl-P, `j`, Down, Ctrl-J/Ctrl-N, `gk`/`gj`, `-`, `+`/Enter, `_`, `gg`/Ctrl-Home, `G`, Ctrl-End, `{count}%`, `H`, `M`, `L` | executable Vim oracle; `MotionState` verifies counted `G` and `%` reinterpretation | Exercised under the stated limits |
| Word and character search | `w`, `W`, `e`, `E`, `b`, `B`, `ge`, `gE`, `f`, `F`, `t`, `T`, `;`, `,` | executable Vim oracle for direct motions; `MotionState` and ChronoGit key-adapter tests for target/repeat state | Exercised under the stated limits |
| Sentence, paragraph, and section | `(`, `)`, `{`, `}`, `[[`, `]]`, `[]`, `][` | executable Vim oracle and empty-line/count unit tests | Exercised under the stated limits |
| Built-in structural | `%`, `[(`, `[{`, `])`, `]}`, `[m`, `[M`, `]m`, `]M`, `[#`, `]#`, `[*`/`[/`, `]*`/`]/` | executable Vim oracle for delimiter, method, preprocessor, and comment forms; nested unit tests | Exercised under the stated limits |
| Search-derived | `/`, `?`, `n`, `N`, `*`, `#`, `g*`, `g#` | `MotionKind` signals plus ChronoGit's retained smart-case/wraparound search-state tests | Exercised through ChronoGit adapter |
| Marks and jump history | `m{mark}`, `'{mark}`, `` `{mark}``, `g'{mark}`, `` g`{mark}``, `['`, `` [` ``, `]'`, `` ]` ``, counted Ctrl-O/Ctrl-I | ChronoGit key, mark-store, cross-position, count, and jump-history tests | Exercised through ChronoGit adapter |
| Vertical viewport | Ctrl-D/U, Ctrl-F/B, PageDown/PageUp, Ctrl-E/Y, `zt`, `z<CR>`, `zz`, `z.`, `zb`, `z-`, `z+`, `z^` | executable Vim oracle compares cursor and `topline` | Exercised under the stated limits |
| Horizontal viewport | `zh`, `zl`, `zH`, `zL`, `zs`, `ze` | executable Vim oracle compares cursor and `leftcol` | Exercised under the stated limits |
| Diff navigation | `[c`, `]c` | pure diff-block unit tests and ChronoGit diff adapter/render tests | Exercised through ChronoGit adapter |

The executable suite currently compares 85 direct cursor/viewport cases.
The ordinary crate and ChronoGit suites also exercise stateful/contextual rows.
Caller-adapter evidence describes ChronoGit's integration, not functionality
automatically supplied to another user of this library.

ChronoGit's default application keymap reserves Space as a leader for views,
repository search, message, layout, and tree actions. It therefore does not
also bind standalone Space to `RightWrap` in that normal context; `l` and Right
remain the default non-wrapping right movements, and a custom map can bind
`cursor_right_wrap` to a non-conflicting key such as backslash. Search prompts
treat Space as query text. This is an adapter choice, not a reduction of the
library vocabulary: `MotionKind::RightWrap`, its `whichwrap=s` comparison, and
all 85 oracle cases remain present.

## Explicit boundaries

The following functionality is outside this library's contract:

- operators, operator-pending result ranges, Visual selections, registers,
  macros, Ex ranges, and mouse actions are editor/UI features rather than the
  standalone read-only motion contract;
- change/insert/Visual special marks (`'[`, `']`, `'<`, `'>`, `'.`, `'^`),
  the change list (`g;`, `g,`), and cross-file viminfo marks require editing or
  Vim session/file lifecycle that the caller does not delegate to this crate;
- folds, spell movement, tags, quickfix, and plugin-defined movements require
  resources outside the borrowed plain-text buffer;
- `g%` is supplied by Vim's optional `matchit` plugin, not core
  `motion.txt`. `MatchingPairBackward` provides ChronoGit's existing reverse
  delimiter/comment/preprocessor extension, but arbitrary plugin
  `b:match_words` expressions are not claimed as core Vim compatibility;
- soft-wrapped display-line behavior is not claimed under the fixed `nowrap`
  comparison. A future wrapping renderer must supply a display-row contract
  and add `wrap` oracle cases before claiming that separate condition.

## Implementation limits

- Movement and editing operate on Unicode scalar values, not grapheme clusters.
  Word classes use Rust's whitespace/alphanumeric predicates plus underscore;
  Vim's configurable `iskeyword` and combining-character rules are not modeled.
- Delimiter matching scans literal delimiters without Vim's quote/escape-aware
  syntax rules. Method motions scan braces, section motions scan braces at
  column zero, and paragraphs use empty lines; language-specific method
  recognition and Vim's `sections`/`paragraphs` macro options are not modeled.
- An explicit Ctrl-D/U count applies to that motion only. The library does not
  retain Vim's `scroll` option for subsequent uncounted Ctrl-D/U commands.
- Diff motions identify adjacent `+`/`-` text blocks, excluding file headers.
  They do not calculate Vim diff mode's comparisons between multiple buffers.
- Search and mark `MotionKind` variants are caller signals; `apply` does not
  execute them. Named marks, jump history, and search prompts must be handled
  by the application. Character-search repeats must go through `MotionState`.

## Modal input and ownership

`MotionState` owns only incomplete Normal-mode counts and find/till state.
`EditableBuffer` separately owns an explicitly mutable Normal/Insert buffer.
It covers `i`, `a`, `I`, `A`, `o`, `O`, Unicode scalar input, newline,
Backspace, Delete, content-preserving Escape, and a configurable Insert escape
sequence that defaults to `jj`. A possible prefix is inserted immediately; the
buffer retains only recognition state, so a lone `j`, a mismatch, editing key,
focus/target change followed by `flush_pending_input`, or caller abandonment
cannot lose text. Completing `jj` removes the escape prefix and uses the same
Normal cursor normalization as Esc. `InsertEscapeSequence::disabled()` permits
literal `jj`, and `InsertEscapeSequence::new` replaces it. Tests cover these
paths with multiline input, UTF-8 boundaries, and full byte limits.

ChronoGit never creates `EditableBuffer`. Its Git, diff, source, and commit
text stays read-only. ChronoGit owns terminal key events, resource-aware marks
and jump locations, search query/history/highlights, LSP positions, and list
selection; those adapters consume the crate's motion/state contract without
moving those application resources into the generic crate. In particular,
ChronoGit's repository and document search prompts do not use
`EditableBuffer`, so `jj` remains query text and existing confirmation,
cancellation, and highlight behavior is unchanged.
