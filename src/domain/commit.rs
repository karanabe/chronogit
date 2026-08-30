use std::fmt::{self, Display, Formatter};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObjectId(String);

impl ObjectId {
    pub fn parse(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("object ID must be a full 40- or 64-character hexadecimal value");
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn short(&self) -> &str {
        let end = self.0.len().min(10);
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitSummary {
    id: ObjectId,
    parents: Vec<ObjectId>,
    author: String,
    authored_at: String,
    subject: String,
}

impl CommitSummary {
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

    #[must_use]
    pub fn id(&self) -> &ObjectId {
        &self.id
    }

    #[must_use]
    pub fn parents(&self) -> &[ObjectId] {
        &self.parents
    }

    #[must_use]
    pub fn author(&self) -> &str {
        &self.author
    }

    #[must_use]
    pub fn authored_at(&self) -> &str {
        &self.authored_at
    }

    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    #[must_use]
    pub fn baseline(&self) -> CommitBaseline {
        self.parents
            .first()
            .cloned()
            .map_or(CommitBaseline::EmptyTree, CommitBaseline::FirstParent)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CommitBaseline {
    EmptyTree,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitMessage(String);

impl CommitMessage {
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn body(&self) -> &str {
        self.0.split_once('\n').map_or("", |(_, remainder)| {
            remainder.strip_prefix('\n').unwrap_or(remainder)
        })
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
