//! Vim-compatible navigation and explicit modal text input.
//!
//! `vim-navigation` has no terminal-framework, filesystem, or application
//! dependency. Its three modules have deliberately different owners:
//!
//! - [`motion`] applies completed read-only cursor and viewport movements to a
//!   borrowed text snapshot;
//! - [`command`] resolves Normal-mode counts, character arguments, and
//!   character-search repetition without parsing terminal events;
//! - [`editor`] is the separate opt-in owner of mutable Normal/Insert text.
//!
//! Search histories, marks, jump lists, key bindings, and application resources
//! stay with the caller. Positions use zero-based line numbers and UTF-8 byte
//! columns; viewport columns use terminal display cells and four-cell tab stops.
//!
//! # 1. Apply a read-only motion
//!
//! ```
//! use vim_navigation::{Cursor, Motion, MotionKind, Viewport, apply};
//!
//! let lines = ["alpha beta", "gamma"];
//! let before = lines;
//! let mut viewport = Viewport::new(0, 0, 2, 20, 0);
//! let cursor = apply(
//!     &lines,
//!     Cursor::new(0, 0),
//!     &mut viewport,
//!     Motion::new(MotionKind::WordForward),
//! );
//!
//! assert_eq!(cursor, Cursor::new(0, 6));
//! assert_eq!(lines, before); // `apply` only borrows the snapshot.
//! ```
//!
//! # 2. Resolve a count and character argument
//!
//! ```
//! use vim_navigation::{Motion, MotionKind, MotionResolution, MotionState};
//!
//! let mut commands = MotionState::new();
//! assert!(commands.push_count_digit('2'));
//! assert_eq!(
//!     commands.finish(Motion::new(MotionKind::FindForward)),
//!     MotionResolution::AwaitingTarget,
//! );
//! let Some(find) = commands.accept_target('a') else {
//!     panic!("the preceding find motion is waiting for a target");
//! };
//! assert_eq!(find.count(), 2);
//! assert_eq!(find.target(), Some('a'));
//! ```
//!
//! # 3. Opt into Normal/Insert editing
//!
//! ```
//! use vim_navigation::{EditOutcome, EditableBuffer, EditorInput, Mode, Viewport};
//!
//! let mut buffer = EditableBuffer::new("ab", Viewport::new(0, 0, 2, 20, 0));
//! assert_eq!(buffer.handle(EditorInput::Append), EditOutcome::ModeChanged);
//! assert_eq!(buffer.handle(EditorInput::Character('界')), EditOutcome::Changed);
//! assert_eq!(buffer.handle(EditorInput::Newline), EditOutcome::Changed);
//! assert_eq!(buffer.handle(EditorInput::Character('x')), EditOutcome::Changed);
//! assert_eq!(buffer.handle(EditorInput::Character('j')), EditOutcome::Changed);
//! assert_eq!(buffer.handle(EditorInput::Character('j')), EditOutcome::ModeChanged);
//!
//! assert_eq!(buffer.mode(), Mode::Normal);
//! assert_eq!(buffer.text(), "a界\nxb");
//! ```

pub mod command;
pub mod editor;
pub mod motion;

pub use command::{MotionResolution, MotionState};
pub use editor::{
    EditOutcome, EditableBuffer, EditorInput, InsertEscapeSequence, InsertEscapeSequenceError, Mode,
};
pub use motion::{CountSource, Cursor, Motion, MotionKind, Viewport, apply, reveal};

// Keep the published README's examples executable alongside the API examples.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
mod readme_examples {}
