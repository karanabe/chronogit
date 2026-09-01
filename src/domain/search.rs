use crate::domain::RepoPath;

/// One repository-wide file or content-search result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    path: RepoPath,
    line: Option<u32>,
    preview: String,
}

impl SearchHit {
    #[must_use]
    pub fn file(path: RepoPath) -> Self {
        Self {
            path,
            line: None,
            preview: String::new(),
        }
    }

    #[must_use]
    pub fn content(path: RepoPath, line: u32, preview: String) -> Self {
        Self {
            path,
            line: Some(line),
            preview,
        }
    }

    #[must_use]
    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    #[must_use]
    pub fn line(&self) -> Option<u32> {
        self.line
    }

    #[must_use]
    pub fn preview(&self) -> &str {
        &self.preview
    }
}

/// Bounded content read from a file in the current working tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileDocument {
    Text { lines: Vec<String>, truncated: bool },
    Binary { summary: String },
    Symlink { target: String },
    Unavailable { summary: String },
}

impl FileDocument {
    #[must_use]
    pub fn lines(&self) -> &[String] {
        match self {
            Self::Text { lines, .. } => lines,
            Self::Binary { .. } | Self::Symlink { .. } | Self::Unavailable { .. } => &[],
        }
    }

    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Binary { summary } => Some(summary),
            Self::Symlink { target } => Some(target),
            Self::Unavailable { summary } => Some(summary),
            Self::Text { .. } => None,
        }
    }

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
