//! Application state, user intent, asynchronous effects, and state transitions.
//!
//! [`AppState`] is the authoritative Git and Code workflow model. Callers feed it [`Action`] values
//! from the terminal and [`Event`] values from [`EffectExecutor`]. Each update
//! returns typed [`AppEffect`] values for repository and optional LSP work.

mod action;
mod code_view;
mod effect;
mod model;
mod repository_search;
mod search;
mod semantic_navigation;
mod update;
mod vim;

pub use crate::domain::SemanticNavigationKind;
pub use action::{Action, Event, VimMotion, VimMotionKind};
pub use effect::{AppEffect, EffectExecutor, GitEffect, LspEffect};
pub use model::{
    AppState, AppView, ErrorNotice, FocusedPane, HistoryPanel, LoadState, Overlay,
    RepositorySearchKind, RequestId, VisibleTreeEntry,
};
pub(crate) use model::{CodeEntryKind, VisibleCodeEntry};
pub use search::SearchDirection;
pub(crate) use search::SearchState;
