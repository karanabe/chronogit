//! Reusable layers for the ChronoGit terminal application.
//!
//! ChronoGit is a read-only Git history and working-tree source explorer. This crate exposes the
//! domain model, the bounded Git adapter, the application state machine, and
//! the terminal presentation layer used by the `chronogit` binary.
//!
//! The modules follow a one-way dependency flow:
//!
//! - [`domain`] owns validated values and has no process or terminal I/O.
//! - [`git`] translates read-only Git output into domain values.
//! - [`lsp`] owns optional language-server process and protocol boundaries.
//! - [`app`] turns user intent into state transitions and typed Git/LSP effects.
//! - [`tui`] maps terminal input to actions and renders application state.
//!
//! Most embedders start at [`git::GitService`] when they need repository data,
//! or at [`app::AppState`] and [`tui::run`] when constructing the complete TUI.
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//!
//! use chronogit::git::{GitService, SystemGitRunner};
//!
//! # fn main() -> Result<(), chronogit::git::GitError> {
//! let repository = GitService::discover(SystemGitRunner, Path::new("."))?;
//! for change in repository.changes()? {
//!     println!("{} {}", change.kind().label(), change.path());
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

pub mod app;
pub mod cli;
pub mod domain;
pub mod error;
pub mod git;
pub mod lsp;
pub mod tui;
