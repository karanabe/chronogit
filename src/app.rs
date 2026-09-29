//! Application state, user intent, asynchronous effects, and state transitions.
//!
//! [`AppState`] is the authoritative Git and Code workflow model. Callers feed it [`Action`] values
//! from the terminal and [`Event`] values from [`EffectExecutor`]. Each update
//! returns typed [`AppEffect`] values for repository and optional LSP work.

mod action;
mod branches;
mod code_view;
mod effect;
mod model;
mod repository_search;
mod search;
mod semantic_navigation;
mod source_view;
mod update;
#[cfg(test)]
mod viewport_tests;
mod vim;

pub(crate) use source_view::display_line as full_file_display_line;
pub(crate) use vim::scroll_top;

pub(crate) const FALLBACK_HALF_PAGE_LINES: isize = 10;
pub(crate) const HORIZONTAL_SCROLL_COLUMNS: usize = 4;
pub(crate) const PAGE_OVERLAP_LINES: usize = 2;

pub use crate::domain::SemanticNavigationKind;
pub use action::{Action, Event, JumpHistory, MarkJumpTarget};
pub use effect::{AppEffect, EffectExecutor, GitEffect, LspEffect};
pub use model::{
    AppState, AppView, CommitLoadMode, DocumentRevision, ErrorNotice, FocusedPane, HistoryPanel,
    LoadState, LspAvailability, Overlay, RepositorySearchKind, RequestId, VisibleTreeEntry,
};
pub(crate) use model::{
    CodeEntryKind, CursorColumnPolicy, FullFileDeletion, FullFileMode, HistoryContinuation,
    HistoryPreview, HorizontalDirection, SourceDiffContext, VerticalEdge, VisibleCodeEntry,
};
pub use search::SearchDirection;
pub(crate) use search::{SearchScope, SearchState};
pub use vim_navigation::{
    CountSource as VimCountSource, Motion as VimMotion, MotionKind as VimMotionKind,
};
