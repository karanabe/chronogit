//! Errors returned by top-level application startup and the terminal loop.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;

use crate::git::GitError;
use crate::lsp::LspConfigError;
use crate::tui::keymap::KeyMapError;

/// A failure that prevents startup or terminates the interactive application.
///
/// Recoverable Git failures inside the TUI are stored in application load state
/// instead. This error is reserved for boundary failures that the binary must
/// report after restoring terminal state.
#[derive(Debug)]
pub enum AppError {
    /// Repository discovery or Git execution failed.
    Git(GitError),
    /// Terminal input, output, or signal handling failed.
    Io(io::Error),
    /// Standard input or output is not attached to an interactive terminal.
    NonInteractiveTerminal,
    /// The optional or explicit keymap could not be loaded or validated.
    KeyMap(KeyMapError),
    /// The optional or explicit trusted LSP profile file was invalid.
    LspConfig(LspConfigError),
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Git(_) => formatter.write_str("Git operation failed"),
            Self::Io(_) => formatter.write_str("terminal I/O failed"),
            Self::NonInteractiveTerminal => {
                formatter.write_str("an interactive TTY is required; run chronogit in a terminal")
            }
            Self::KeyMap(_) => formatter.write_str("keymap configuration failed"),
            Self::LspConfig(_) => formatter.write_str("LSP configuration failed"),
        }
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Git(source) => Some(source),
            Self::Io(source) => Some(source),
            Self::KeyMap(source) => Some(source),
            Self::LspConfig(source) => Some(source),
            Self::NonInteractiveTerminal => None,
        }
    }
}

impl From<GitError> for AppError {
    fn from(value: GitError) -> Self {
        Self::Git(value)
    }
}

impl From<io::Error> for AppError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<KeyMapError> for AppError {
    fn from(value: KeyMapError) -> Self {
        Self::KeyMap(value)
    }
}

impl From<LspConfigError> for AppError {
    fn from(value: LspConfigError) -> Self {
        Self::LspConfig(value)
    }
}
