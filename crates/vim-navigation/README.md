<br />
<h1 align="center">vim-navigation</h1>
<h3 align="center">Reusable Vim-compatible navigation and modal text-input state for Rust applications.</h3>
<br />
<br />

`vim-navigation` is a terminal-framework-independent Rust library for
Vim-compatible cursor and viewport motion, Normal-mode command state, and an
explicitly opt-in Normal/Insert text buffer. It does not perform terminal I/O,
read files, or know about an application's documents.

The [compatibility contract](https://github.com/karanabe/chronogit/blob/master/crates/vim-navigation/COMPATIBILITY.md)
lists the tested motions and limits, including scalar-based Unicode movement
and simplified structural scans.

## Installation

Requires Rust 1.88 or newer. For a published release, add:

```toml
[dependencies]
vim-navigation = "0.1.0"
```

To try unreleased source before its version is available on crates.io, use the
repository instead:

```toml
[dependencies]
vim-navigation = { git = "https://github.com/karanabe/chronogit" }
```

The library is developed in the ChronoGit workspace. ChronoGit's own manifest
uses both `version` and `path`: Cargo uses the local crate in the checkout and
the versioned registry dependency in a published package. External consumers
do not need the workspace's directory layout.

## Major concepts

- `motion` owns zero-based `Cursor` coordinates, `Viewport` display geometry,
  the `MotionKind` vocabulary, explicit/implicit `CountSource`, and pure motion
  over borrowed logical lines.
- `command` owns a saturating count, an incomplete find/till command, its
  character argument, and `;`/`,` repetition through `MotionState`.
- `editor` owns the separate mutable `EditableBuffer`, its Normal/Insert mode,
  input vocabulary, byte limit, and edit outcomes.

All public types remain available from the crate root for concise imports; the
three public module paths expose the responsibility boundaries for reference
documentation. Application-owned key bindings, search history, marks, jump
lists, file access, and rendering stay outside the crate.

## Read-only motion

`Cursor::byte_column` is a UTF-8 byte offset. Viewport `left`, `width`, and
`gutter` are terminal display cells. Tabs use four-cell stops, and Unicode
display width is measured independently of its byte length.

```rust
use vim_navigation::{Cursor, Motion, MotionKind, Viewport, apply};

let lines = ["alpha beta", "gamma"];
let before = lines;
let mut viewport = Viewport::new(0, 0, 2, 20, 0);
let cursor = apply(
    &lines,
    Cursor::new(0, 0),
    &mut viewport,
    Motion::new(MotionKind::WordForward),
);

assert_eq!(cursor, Cursor::new(0, 6));
assert_eq!(lines, before);
```

`apply` accepts empty input and clamps invalid line or byte coordinates to a
safe Normal-mode position. It updates the viewport and retained desired display
column, but cannot mutate the borrowed lines.

## Counts and character arguments

Feed decimal digits and completed key bindings into `MotionState`. A find/till
template first reports that it needs a character, then produces a complete
motion for `apply`:

```rust
use vim_navigation::{Motion, MotionKind, MotionResolution, MotionState};

let mut commands = MotionState::new();
assert!(commands.push_count_digit('2'));
assert_eq!(
    commands.finish(Motion::new(MotionKind::FindForward)),
    MotionResolution::AwaitingTarget,
);
let Some(motion) = commands.accept_target('a') else {
    panic!("the preceding find motion is waiting for a target");
};
assert_eq!(motion.count(), 2);
assert_eq!(motion.target(), Some('a'));
```

Counts saturate at `usize::MAX`; a leading zero remains available for the Vim
`0` motion. Callers own terminal event parsing and should reset an incomplete
command when mode or focus changes.

When a caller constructs an already-resolved counted motion directly, the
count origin remains explicit in the type instead of a positional boolean:

```rust
use vim_navigation::{CountSource, Motion, MotionKind};

let motion = Motion::new(MotionKind::WordForward).counted(3, CountSource::Explicit);
assert_eq!(motion.count(), 3);
assert!(motion.has_explicit_count());
```

## Opt-in editing

Applications that intentionally permit editing can own an `EditableBuffer`.
Read-only applications do not need to depend on or construct this type.

```rust
use vim_navigation::{EditOutcome, EditableBuffer, EditorInput, Mode, Viewport};

let mut buffer = EditableBuffer::new("ab", Viewport::new(0, 0, 2, 20, 0));
assert_eq!(buffer.handle(EditorInput::Append), EditOutcome::ModeChanged);
assert_eq!(buffer.handle(EditorInput::Character('界')), EditOutcome::Changed);
assert_eq!(buffer.handle(EditorInput::Character('j')), EditOutcome::Changed);
assert_eq!(buffer.handle(EditorInput::Character('j')), EditOutcome::ModeChanged);
assert_eq!(buffer.mode(), Mode::Normal);
assert_eq!(buffer.text(), "a界b");
```

Use `EditableBuffer::with_byte_limit` for bounded input. Invalid mode/input
combinations, line separators passed as characters, boundary deletion, and
limit-exceeding insertion return `EditOutcome::Ignored` rather than panicking.
The default Insert escape sequence is `jj`; the first `j` is inserted
immediately and only its candidate status is retained, so a lone prefix never
waits behind a timeout or disappears on a focus change. The second `j` removes
the escape prefix and applies the same cursor normalization as Esc. Call
`flush_pending_input` before changing an input target, use
`InsertEscapeSequence::disabled()` for literal `jj`, or construct a replacement
such as `InsertEscapeSequence::new("jk")`.

`cargo test -p vim-navigation --doc` verifies the Rust examples in this README
and the API documentation, including the multi-line editor example.

## ChronoGit boundary and compatibility

ChronoGit's adapter converts application
positions and pane geometry, applies crate motion to borrowed document lines,
and retains application-specific search and mark state. ChronoGit never creates
`EditableBuffer`, so source, diff, Git-object, and commit text remain read-only.
Its normal application context reserves Space as a leader, so its default
adapter omits standalone Space/`RightWrap` while keeping `l` and Right. Search
prompts treat Space and `jj` as literal text. These application choices do not
remove the library's Vim Space motion or default editable-buffer `jj` contract.
See the [ChronoGit architecture](https://github.com/karanabe/chronogit/blob/master/docs/src/content/docs/developer/architecture.md)
for that integration boundary.

[`COMPATIBILITY.md`](https://github.com/karanabe/chronogit/blob/master/crates/vim-navigation/COMPATIBILITY.md) records the fixed Vim reference and
comparison conditions, supported command inventory, executable
85-case Vim oracle, stateful adapter evidence, and explicit exclusions. A
successful Rust build alone is not treated as Vim-compatibility evidence.

### License

<sup>
Licensed under either of <a href="https://github.com/karanabe/chronogit/blob/master/crates/vim-navigation/LICENSE-APACHE">Apache License, Version 2.0</a> or <a href="https://github.com/karanabe/chronogit/blob/master/crates/vim-navigation/LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>
