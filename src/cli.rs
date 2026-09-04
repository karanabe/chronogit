//! Command-line parsing and conversion into application startup values.

use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};

use crate::app::AppView;

/// A read-only terminal Git history, diff, and source-code explorer.
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

    /// Enable a trusted external language-server profile (repeatable).
    #[arg(long = "lsp", value_name = "PROFILE")]
    lsp: Vec<String>,

    /// Trusted user-level LSP profile file. Defaults to XDG config when present.
    #[arg(long, value_name = "PATH")]
    lsp_config: Option<PathBuf>,
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

    /// Returns the explicitly enabled language-server profile identifiers.
    #[must_use]
    pub fn lsp_profiles(&self) -> &[String] {
        &self.lsp
    }

    /// Returns the explicitly selected trusted LSP configuration path.
    #[must_use]
    pub fn lsp_config(&self) -> Option<&Path> {
        self.lsp_config.as_deref()
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
    /// Browse the working-tree file tree and source code.
    Code,
}

impl From<InitialView> for AppView {
    fn from(value: InitialView) -> Self {
        match value {
            InitialView::Changes => Self::Changes,
            InitialView::History => Self::History,
            InitialView::Graph => Self::Graph,
            InitialView::Code => Self::Code,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Cli;
    use crate::app::AppView;

    #[test]
    fn parses_code_as_an_initial_view() {
        let cli = Cli::try_parse_from(["chronogit", "--view", "code"])
            .unwrap_or_else(|error| panic!("could not parse code view: {error}"));
        assert_eq!(cli.initial_view(), AppView::Code);
    }

    #[test]
    fn accepts_multiple_explicit_lsp_profiles() {
        let cli = Cli::try_parse_from([
            "chronogit",
            "--lsp",
            "rust-analyzer",
            "--lsp",
            "jdtls",
            "--lsp",
            "pyright",
        ])
        .unwrap_or_else(|error| panic!("could not parse LSP profiles: {error}"));
        assert_eq!(cli.lsp_profiles(), &["rust-analyzer", "jdtls", "pyright"]);
    }
}
