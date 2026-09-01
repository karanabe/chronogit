//! Repository search results and bounded working-tree file contents.

use crate::domain::RepoPath;

/// One repository-wide file or content-search result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    path: RepoPath,
    line: Option<u32>,
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
    pub fn content(path: RepoPath, line: u32, preview: String) -> Self {
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
    pub fn line(&self) -> Option<u32> {
        self.line
    }

    /// Returns the content preview, or an empty string for file results.
    #[must_use]
    pub fn preview(&self) -> &str {
        &self.preview
    }
}

/// Bounded content read from a file in the current working tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileDocument {
    /// Text content split into display lines.
    Text {
        /// Lines retained up to the configured read limit.
        lines: Vec<String>,
        /// Whether the file continued beyond the retained bytes.
        truncated: bool,
    },
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
    /// Returns text lines, or an empty slice for non-text outcomes.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        match self {
            Self::Text { lines, .. } => lines,
            Self::Binary { .. } | Self::Symlink { .. } | Self::Unavailable { .. } => &[],
        }
    }

    /// Returns the display message for a non-text outcome.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Binary { summary } => Some(summary),
            Self::Symlink { target } => Some(target),
            Self::Unavailable { summary } => Some(summary),
            Self::Text { .. } => None,
        }
    }

    /// Reports whether a text file crossed the configured read limit.
    #[must_use]
    pub fn is_truncated(&self) -> bool {
        matches!(
            self,
            Self::Text {
                truncated: true,
                ..
            }
        )
    }
}
