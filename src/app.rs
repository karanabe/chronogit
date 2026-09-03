//! Application state, user intent, asynchronous effects, and state transitions.
//!
//! [`AppState`] is the authoritative Git and Code workflow model. Callers feed it [`Action`] values
//! from the terminal and [`Event`] values from [`EffectExecutor`]. Each update
//! returns the [`GitEffect`] values required to continue loading repository data.

mod action;
mod code_view;
mod effect;
mod model;
mod repository_search;
mod search;
mod update;

pub use action::{Action, Event};
pub use effect::{EffectExecutor, GitEffect};
pub use model::{
    AppState, AppView, ErrorNotice, FocusedPane, HistoryPanel, LoadState, Overlay,
    RepositorySearchKind, RequestId, VisibleTreeEntry,
};
pub(crate) use model::{CodeEntryKind, VisibleCodeEntry};
pub use search::SearchDirection;
pub(crate) use search::SearchState;
