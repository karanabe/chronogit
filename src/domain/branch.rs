//! Existing local branch identities, preserving Git's original name bytes.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

/// An existing local branch returned by repository enumeration.
///
/// Construction is restricted to the Git boundary. Display text is lossy UTF-8;
/// switching uses the original bytes, including for non-UTF-8 branch names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalBranch {
    name: Vec<u8>,
    current: bool,
}

impl LocalBranch {
    pub(crate) fn from_ref(reference: &[u8], current: bool) -> Option<Self> {
        let name = reference.strip_prefix(b"refs/heads/")?;
        if name.is_empty()
            || name.starts_with(b"-")
            || name == b"HEAD"
            || name.ends_with(b".")
            || name.windows(2).any(|part| matches!(part, b".." | b"@{"))
            || name
                .iter()
                .any(|byte| *byte <= b' ' || *byte == 127 || b"~^:?*[\\".contains(byte))
            || name
                .split(|byte| *byte == b'/')
                .any(|part| part.is_empty() || part.starts_with(b".") || part.ends_with(b".lock"))
        {
            return None;
        }
        Some(Self {
            name: name.to_vec(),
            current,
        })
    }

    /// Returns the branch name for display; terminal renderers must sanitize it.
    #[must_use]
    pub fn display(&self) -> String {
        String::from_utf8_lossy(&self.name).into_owned()
    }

    /// Reports whether this branch was checked out when the list was read.
    #[must_use]
    pub const fn is_current(&self) -> bool {
        self.current
    }

    pub(crate) fn as_os_str(&self) -> &OsStr {
        OsStr::from_bytes(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::LocalBranch;

    #[test]
    fn branch_identities_accept_local_names_and_reject_argument_or_revision_syntax() {
        for reference in [
            b"refs/heads/topic".as_slice(),
            b"refs/heads/feature/topic",
            b"refs/heads/@",
            b"refs/heads/non-utf8-\xff",
        ] {
            assert!(LocalBranch::from_ref(reference, false).is_some());
        }
        for reference in [
            b"refs/remotes/origin/main".as_slice(),
            b"refs/heads/",
            b"refs/heads/--force",
            b"refs/heads/@{-1}",
            b"refs/heads/main~1",
            b"refs/heads/main\0--force",
            b"refs/heads/a..b",
            b"refs/heads/.hidden",
            b"refs/heads/a.lock",
            b"refs/heads/topic\n",
        ] {
            assert!(
                LocalBranch::from_ref(reference, false).is_none(),
                "{reference:?}"
            );
        }
    }
}
