//! Command-line parsing and conversion into application startup values.

use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};

use crate::app::AppView;

/// A read-only terminal history and diff explorer.
#[derive(Debug, Parser)]
#[command(name = "chronogit", version, about)]
pub struct Cli {
    /// Repository root or any directory below it.
    #[arg(value_name = "PATH", default_value = ".")]
    path: PathBuf,

    /// View to open first.
    #[arg(long, value_enum, default_value_t = InitialView::Changes)]
    view: InitialView,

    /// Keymap file. Defaults to $XDG_CONFIG_HOME/chronogit/keymap.conf when present.
    #[arg(long, value_name = "PATH")]
    keymap: Option<PathBuf>,
}

impl Cli {
    /// Returns the repository root candidate supplied by the user.
    ///
    /// The path may name the worktree root or any directory below it; repository
    /// discovery resolves the actual absolute root later.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Converts the CLI view choice into the corresponding application view.
    #[must_use]
    pub fn initial_view(&self) -> AppView {
        self.view.into()
    }

    /// Returns the explicitly requested keymap path, when supplied.
    #[must_use]
    pub fn keymap(&self) -> Option<&Path> {
        self.keymap.as_deref()
    }
}

/// Main view selected by the `--view` command-line option.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum InitialView {
    /// Show unstaged worktree changes.
    #[default]
    Changes,
    /// Show paged commit history and diffs.
    History,
    /// Show commit parent lanes.
    Graph,
}

impl From<InitialView> for AppView {
    fn from(value: InitialView) -> Self {
        match value {
            InitialView::Changes => Self::Changes,
            InitialView::History => Self::History,
            InitialView::Graph => Self::Graph,
        }
    }
}
