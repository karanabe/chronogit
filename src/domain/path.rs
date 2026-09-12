//! Validated repository roots and repository-relative byte paths.

use std::error::Error;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use bstr::{BString, ByteSlice};

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

/// An absolute path to the discovered worktree root.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RepositoryRoot(PathBuf);

/// Error returned when a repository root is not absolute.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepositoryRootError;

impl Display for RepositoryRootError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository root must be absolute")
    }
}

impl Error for RepositoryRootError {}

impl RepositoryRoot {
    /// Creates a repository root after checking that the path is absolute.
    ///
    /// # Errors
    ///
    /// Returns an error for relative paths. Existence and repository membership
    /// are established separately by [`crate::git::GitService::discover`].
    pub fn new(path: PathBuf) -> Result<Self, RepositoryRootError> {
        if path.is_absolute() {
            Ok(Self(path))
        } else {
            Err(RepositoryRootError)
        }
    }

    /// Returns the absolute filesystem path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl Display for RepositoryRoot {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(formatter)
    }
}

impl AsRef<Path> for RepositoryRoot {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl TryFrom<PathBuf> for RepositoryRoot {
    type Error = RepositoryRootError;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        Self::new(path)
    }
}

/// A validated path relative to a [`RepositoryRoot`].
///
/// The value retains Git's raw bytes on Unix. It cannot be empty, absolute,
/// contain NUL, or contain empty, `.` or `..` components.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RepoPath(BString);

/// A failed [`RepoPath`] validation rule.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepoPathError {
    /// The path has no components.
    Empty,
    /// The path contains a NUL byte and cannot cross process boundaries.
    ContainsNul,
    /// The path begins at a filesystem root rather than the repository root.
    Absolute,
    /// The path contains an empty, current-directory, or parent component.
    InvalidComponent,
}

impl Display for RepoPathError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "repository path must not be empty",
            Self::ContainsNul => "repository path must not contain NUL",
            Self::Absolute => "repository path must be relative",
            Self::InvalidComponent => {
                "repository path must not contain empty, dot, or parent components"
            }
        })
    }
}

impl Error for RepoPathError {}

impl RepoPath {
    /// Validates and stores a repository-relative byte path.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty or absolute path, a NUL byte, or an empty,
    /// current-directory, or parent-directory component.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, RepoPathError> {
        if bytes.is_empty() {
            return Err(RepoPathError::Empty);
        }
        if bytes.contains(&0) {
            return Err(RepoPathError::ContainsNul);
        }
        if bytes.starts_with(b"/") {
            return Err(RepoPathError::Absolute);
        }
        if bytes
            .split(|byte| *byte == b'/')
            .any(|component| component.is_empty() || matches!(component, b"." | b".."))
        {
            return Err(RepoPathError::InvalidComponent);
        }
        Ok(Self(BString::from(bytes)))
    }

    /// Returns the original Git path bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    /// Returns a lossy UTF-8 representation intended only for presentation.
    #[must_use]
    pub fn display(&self) -> String {
        self.0.to_str_lossy().into_owned()
    }

    /// Appends a validated child path while preserving raw bytes.
    #[must_use]
    pub fn join(&self, child: &Self) -> Self {
        let mut bytes = self.0.to_vec();
        bytes.push(b'/');
        bytes.extend_from_slice(child.as_bytes());
        Self(BString::from(bytes))
    }

    #[cfg(unix)]
    /// Converts the raw path bytes into a Unix operating-system string.
    #[must_use]
    pub fn to_os_string(&self) -> OsString {
        OsString::from_vec(self.0.to_vec())
    }
}

impl Display for RepoPath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        self.0.to_str_lossy().fmt(formatter)
    }
}

impl AsRef<[u8]> for RepoPath {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl TryFrom<Vec<u8>> for RepoPath {
    type Error = RepoPathError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        Self::from_bytes(bytes)
    }
}

impl TryFrom<&[u8]> for RepoPath {
    type Error = RepoPathError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        Self::from_bytes(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::RepoPath;

    #[test]
    fn path_rejects_absolute_and_nul() {
        assert!(RepoPath::from_bytes(Vec::new()).is_err());
        assert!(RepoPath::from_bytes(b"/tmp/file".to_vec()).is_err());
        assert!(RepoPath::from_bytes(b"bad\0path".to_vec()).is_err());
        assert!(RepoPath::from_bytes(b"../outside".to_vec()).is_err());
        assert!(RepoPath::from_bytes(b"src/../outside".to_vec()).is_err());
    }

    #[test]
    fn path_join_preserves_raw_bytes() {
        let parent =
            RepoPath::from_bytes(b"src".to_vec()).unwrap_or_else(|error| panic!("{error}"));
        let child =
            RepoPath::from_bytes(vec![b'f', 0xff]).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            parent.join(&child).as_bytes(),
            &[b's', b'r', b'c', b'/', b'f', 0xff]
        );
    }
}
