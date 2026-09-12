//! Commit identifiers, summaries, comparison baselines, and full messages.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::num::NonZeroUsize;
use std::str::FromStr;

const SHA1_HEX_LENGTH: usize = 40;
const SHA256_HEX_LENGTH: usize = 64;
const ABBREVIATED_OBJECT_ID_LENGTH: usize = 10;

/// A non-empty page of commit summaries requested from Git.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CommitPage {
    skip: usize,
    limit: NonZeroUsize,
}

impl CommitPage {
    /// Creates a page after the number of skipped commits and its non-zero size are known.
    #[must_use]
    pub const fn new(skip: usize, limit: NonZeroUsize) -> Self {
        Self { skip, limit }
    }

    /// Returns the number of leading commits omitted from the query.
    #[must_use]
    pub const fn skip(self) -> usize {
        self.skip
    }

    /// Returns the maximum number of commits in the page.
    #[must_use]
    pub const fn limit(self) -> NonZeroUsize {
        self.limit
    }
}

/// A complete hexadecimal Git object identifier.
///
/// Both the 40-character SHA-1 and 64-character SHA-256 object formats are
/// accepted. Keeping the full identifier avoids ambiguous revision lookup.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObjectId(String);

/// A failed full Git object-ID validation rule.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectIdError {
    /// The identifier is neither a SHA-1 nor SHA-256 object width.
    InvalidLength(usize),
    /// At least one character is not hexadecimal ASCII.
    NonHexadecimal,
}

impl Display for ObjectIdError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => write!(
                formatter,
                "object ID must contain 40 or 64 hexadecimal characters, got {length}"
            ),
            Self::NonHexadecimal => {
                formatter.write_str("object ID must contain only hexadecimal ASCII characters")
            }
        }
    }
}

impl Error for ObjectIdError {}

impl ObjectId {
    /// Validates and stores a full Git object identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is not exactly 40 or 64 hexadecimal
    /// characters.
    pub fn parse(value: impl Into<String>) -> Result<Self, ObjectIdError> {
        let value = value.into();
        if !matches!(value.len(), SHA1_HEX_LENGTH | SHA256_HEX_LENGTH) {
            return Err(ObjectIdError::InvalidLength(value.len()));
        }
        if !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ObjectIdError::NonHexadecimal);
        }
        Ok(Self(value))
    }

    /// Returns the complete identifier as hexadecimal text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns at most the first ten hexadecimal characters for display.
    ///
    /// This abbreviation is presentation-only and must not be sent back to Git
    /// where an unambiguous full identifier is required.
    #[must_use]
    pub fn short(&self) -> &str {
        let end = self.0.len().min(ABBREVIATED_OBJECT_ID_LENGTH);
        &self.0[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::ObjectId;

    #[test]
    fn object_id_requires_a_full_sha1_or_sha256_value() {
        assert!(ObjectId::parse("a".repeat(40)).is_ok());
        assert!(ObjectId::parse("b".repeat(64)).is_ok());
        assert!(ObjectId::parse("abcd").is_err());
        assert!(ObjectId::parse("z".repeat(40)).is_err());
    }
}

impl Display for ObjectId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl AsRef<str> for ObjectId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl FromStr for ObjectId {
    type Err = ObjectIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl TryFrom<String> for ObjectId {
    type Error = ObjectIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<&str> for ObjectId {
    type Error = ObjectIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

/// The commit metadata needed by history, graph, and comparison views.
///
/// Parent order is retained because the first parent defines `ChronoGit`'s
/// comparison baseline for non-root and merge commits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitSummary {
    id: ObjectId,
    parents: Vec<ObjectId>,
    author: String,
    authored_at: String,
    subject: String,
}

impl CommitSummary {
    /// Creates a summary from one validated machine-output record.
    #[must_use]
    pub fn new(
        id: ObjectId,
        parents: Vec<ObjectId>,
        author: String,
        authored_at: String,
        subject: String,
    ) -> Self {
        Self {
            id,
            parents,
            author,
            authored_at,
            subject,
        }
    }

    /// Returns the commit object identifier.
    #[must_use]
    pub fn id(&self) -> &ObjectId {
        &self.id
    }

    /// Returns parent identifiers in Git's reported order.
    #[must_use]
    pub fn parents(&self) -> &[ObjectId] {
        &self.parents
    }

    /// Returns the display-ready author name.
    #[must_use]
    pub fn author(&self) -> &str {
        &self.author
    }

    /// Returns the authored timestamp as emitted by the configured Git format.
    #[must_use]
    pub fn authored_at(&self) -> &str {
        &self.authored_at
    }

    /// Returns the first line of the commit message.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// Chooses the comparison baseline used throughout `ChronoGit`.
    ///
    /// Root commits compare against an empty tree. Every other commit,
    /// including a merge, compares against its first parent.
    #[must_use]
    pub fn baseline(&self) -> CommitBaseline {
        self.parents
            .first()
            .cloned()
            .map_or(CommitBaseline::EmptyTree, CommitBaseline::FirstParent)
    }
}

/// The tree used as the older side of a commit comparison.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CommitBaseline {
    /// An empty tree, used when displaying a root commit.
    EmptyTree,
    /// The commit's first parent, including for merge commits.
    FirstParent(ObjectId),
}

impl Display for CommitBaseline {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTree => formatter.write_str("empty tree (root commit)"),
            Self::FirstParent(parent) => write!(formatter, "first parent {}", parent.short()),
        }
    }
}

/// A complete commit message, including its subject and optional body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitMessage(String);

impl CommitMessage {
    /// Wraps the message returned by Git without normalizing its contents.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// Returns the complete message.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the body after the first subject line and one optional blank line.
    #[must_use]
    pub fn body(&self) -> &str {
        self.0.split_once('\n').map_or("", |(_, remainder)| {
            remainder.strip_prefix('\n').unwrap_or(remainder)
        })
    }
}

impl AsRef<str> for CommitMessage {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for CommitMessage {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod message_tests {
    use super::CommitMessage;

    #[test]
    fn commit_body_excludes_the_subject_and_separator() {
        let message = CommitMessage::new("Subject\n\nBody line\nTrailer: value\n".to_owned());
        assert_eq!(message.body(), "Body line\nTrailer: value\n");
        assert_eq!(CommitMessage::new("Subject only\n".to_owned()).body(), "");
    }
}
