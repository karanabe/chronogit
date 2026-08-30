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
}

impl Cli {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn initial_view(&self) -> AppView {
        self.view.into()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum InitialView {
    #[default]
    Changes,
    History,
}

impl From<InitialView> for AppView {
    fn from(value: InitialView) -> Self {
        match value {
            InitialView::Changes => Self::Changes,
            InitialView::History => Self::History,
        }
    }
}
