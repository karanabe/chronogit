use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;

use crate::git::GitError;
use crate::tui::keymap::KeyMapError;

#[derive(Debug)]
pub enum AppError {
    Git(GitError),
    Io(io::Error),
    NonInteractiveTerminal,
    KeyMap(KeyMapError),
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
        }
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Git(source) => Some(source),
            Self::Io(source) => Some(source),
            Self::KeyMap(source) => Some(source),
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
