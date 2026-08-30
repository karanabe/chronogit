use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use bstr::{BString, ByteSlice};

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RepositoryRoot(PathBuf);

impl RepositoryRoot {
    pub fn new(path: PathBuf) -> Result<Self, &'static str> {
        if path.is_absolute() {
            Ok(Self(path))
        } else {
            Err("repository root must be absolute")
        }
    }

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

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RepoPath(BString);

impl RepoPath {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, &'static str> {
        if bytes.is_empty() {
            return Err("repository path must not be empty");
        }
        if bytes.contains(&0) {
            return Err("repository path must not contain NUL");
        }
        if bytes.starts_with(b"/") {
            return Err("repository path must be relative");
        }
        if bytes
            .split(|byte| *byte == b'/')
            .any(|component| component.is_empty() || matches!(component, b"." | b".."))
        {
            return Err("repository path must not contain empty, dot, or parent components");
        }
        Ok(Self(BString::from(bytes)))
    }

    #[must_use]
    pub fn root_marker() -> Self {
        Self(BString::from(Vec::<u8>::new()))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    #[must_use]
    pub fn display(&self) -> String {
        self.0.to_str_lossy().into_owned()
    }

    #[must_use]
    pub fn join(&self, child: &RepoPath) -> RepoPath {
        if self.0.is_empty() {
            return child.clone();
        }
        let mut bytes = self.0.to_vec();
        bytes.push(b'/');
        bytes.extend_from_slice(child.as_bytes());
        Self(BString::from(bytes))
    }

    #[cfg(unix)]
    #[must_use]
    pub fn to_os_string(&self) -> OsString {
        OsString::from_vec(self.0.to_vec())
    }
}

impl Display for RepoPath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.display())
    }
}

#[cfg(test)]
mod tests {
    use super::RepoPath;

    #[test]
    fn path_rejects_absolute_and_nul() {
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
