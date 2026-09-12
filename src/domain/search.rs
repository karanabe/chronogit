//! Repository search results and bounded working-tree file contents.

use crate::domain::{LineNumber, RepoPath};

/// One repository-wide file or content-search result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    path: RepoPath,
    line: Option<LineNumber>,
    preview: String,
}

impl SearchHit {
    /// Creates a file-name search result without a line preview.
    #[must_use]
    pub fn file(path: RepoPath) -> Self {
        Self {
            path,
            line: None,
            preview: String::new(),
        }
    }

    /// Creates a content-search result at a one-based line number.
    #[must_use]
    pub fn content(path: RepoPath, line: LineNumber, preview: String) -> Self {
        Self {
            path,
            line: Some(line),
            preview,
        }
    }

    /// Returns the repository-relative matched path.
    #[must_use]
    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    /// Returns the matched one-based line number for content results.
    #[must_use]
    pub fn line(&self) -> Option<LineNumber> {
        self.line
    }

    /// Returns the content preview, or an empty string for file results.
    #[must_use]
    pub fn preview(&self) -> &str {
        &self.preview
    }
}

/// Displayable text whose exact-source availability and truncation state are
/// internally consistent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextFileDocument {
    lines: Vec<String>,
    state: TextFileState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TextFileState {
    Exact(String),
    DisplayOnly,
    Truncated,
}

impl TextFileDocument {
    /// Creates complete, exact UTF-8 text suitable for language-server use.
    #[must_use]
    pub fn exact(source: String) -> Self {
        Self {
            lines: source.lines().map(ToOwned::to_owned).collect(),
            state: TextFileState::Exact(source),
        }
    }

    /// Creates complete display text without retaining authoritative source.
    #[must_use]
    pub fn display_only(lines: Vec<String>) -> Self {
        Self {
            lines,
            state: TextFileState::DisplayOnly,
        }
    }

    /// Creates a displayable prefix of a file that crossed the read limit.
    #[must_use]
    pub fn truncated(lines: Vec<String>) -> Self {
        Self {
            lines,
            state: TextFileState::Truncated,
        }
    }

    /// Returns retained display lines.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Returns exact text only when the complete UTF-8 source was retained.
    #[must_use]
    pub fn source(&self) -> Option<&str> {
        match &self.state {
            TextFileState::Exact(source) => Some(source),
            TextFileState::DisplayOnly | TextFileState::Truncated => None,
        }
    }

    /// Reports whether only a bounded prefix of the file is available.
    #[must_use]
    pub fn is_truncated(&self) -> bool {
        matches!(self.state, TextFileState::Truncated)
    }
}

/// Bounded content read from a repository file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileDocument {
    /// Text content with a consistent completeness state.
    Text(TextFileDocument),
    /// A binary file that is not decoded as terminal text.
    Binary {
        /// Human-readable file summary.
        summary: String,
    },
    /// A symbolic link represented by its target rather than target contents.
    Symlink {
        /// Lossy, display-ready link target.
        target: String,
    },
    /// Content that could not be represented, for example a special file.
    Unavailable {
        /// Human-readable reason for the unavailable content.
        summary: String,
    },
}

impl FileDocument {
    /// Creates complete, exact UTF-8 text suitable for language-server use.
    #[must_use]
    pub fn exact_text(source: impl Into<String>) -> Self {
        Self::Text(TextFileDocument::exact(source.into()))
    }

    /// Creates complete display text without retaining authoritative source.
    #[must_use]
    pub fn display_text(lines: Vec<String>) -> Self {
        Self::Text(TextFileDocument::display_only(lines))
    }

    /// Creates a displayable prefix of a file that crossed the read limit.
    #[must_use]
    pub fn truncated_text(lines: Vec<String>) -> Self {
        Self::Text(TextFileDocument::truncated(lines))
    }

    /// Returns text lines, or an empty slice for non-text outcomes.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        match self {
            Self::Text(document) => document.lines(),
            Self::Binary { .. } | Self::Symlink { .. } | Self::Unavailable { .. } => &[],
        }
    }

    /// Returns exact decoded text only when the complete document is available.
    ///
    /// Truncated and non-text documents deliberately return `None` so callers
    /// cannot synchronize a partial buffer as authoritative source.
    #[must_use]
    pub fn source(&self) -> Option<&str> {
        match self {
            Self::Text(document) => document.source(),
            Self::Binary { .. } | Self::Symlink { .. } | Self::Unavailable { .. } => None,
        }
    }

    /// Returns the display message for a non-text outcome.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Binary { summary } => Some(summary),
            Self::Symlink { target } => Some(target),
            Self::Unavailable { summary } => Some(summary),
            Self::Text(_) => None,
        }
    }

    /// Reports whether a text file crossed the configured read limit.
    #[must_use]
    pub fn is_truncated(&self) -> bool {
        matches!(self, Self::Text(document) if document.is_truncated())
    }
}

#[cfg(test)]
mod tests {
    use super::{FileDocument, TextFileDocument};

    #[test]
    fn incomplete_or_lossy_text_is_not_an_lsp_document() {
        for document in [
            FileDocument::Text(TextFileDocument::truncated(vec!["partial".to_owned()])),
            FileDocument::Text(TextFileDocument::display_only(vec![
                "replacement �".to_owned(),
            ])),
        ] {
            assert!(document.source().is_none());
        }
    }
}
