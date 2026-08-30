use bstr::ByteSlice;

use crate::domain::{CommitSummary, ObjectId};
use crate::git::GitError;

pub(crate) fn parse_commits(input: &[u8]) -> Result<Vec<CommitSummary>, GitError> {
    let mut fields: Vec<&[u8]> = input.split(|byte| *byte == 0).collect();
    if fields.last().is_some_and(|field| field.is_empty()) {
        fields.pop();
    }
    if fields.is_empty() {
        return Ok(Vec::new());
    }
    if !fields.len().is_multiple_of(5) {
        return Err(GitError::parse(
            "commit log",
            format!("expected groups of 5 fields, got {}", fields.len()),
        ));
    }

    fields
        .chunks_exact(5)
        .map(|chunk| {
            let id = parse_oid(chunk[0])?;
            let parents = chunk[1]
                .split(|byte| *byte == b' ')
                .filter(|value| !value.is_empty())
                .map(parse_oid)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(CommitSummary::new(
                id,
                parents,
                chunk[2].to_str_lossy().into_owned(),
                chunk[3].to_str_lossy().into_owned(),
                chunk[4].to_str_lossy().into_owned(),
            ))
        })
        .collect()
}

fn parse_oid(bytes: &[u8]) -> Result<ObjectId, GitError> {
    let value = std::str::from_utf8(bytes)
        .map_err(|_| GitError::parse("commit log", "object ID is not ASCII"))?;
    ObjectId::parse(value.to_owned()).map_err(|detail| GitError::parse("commit log", detail))
}

#[cfg(test)]
mod tests {
    use super::parse_commits;

    #[test]
    fn parses_root_and_merge_commits() {
        let input = concat!(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\0",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb cccccccccccccccccccccccccccccccccccccccc\0",
            "Ada\0",
            "2026-01-01T00:00:00Z\0",
            "merge\0",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\0",
            "\0",
            "Ada\0",
            "2025-01-01T00:00:00Z\0",
            "root\0"
        );
        let commits = parse_commits(input.as_bytes()).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].parents().len(), 2);
        assert!(commits[1].parents().is_empty());
    }

    #[test]
    fn rejects_incomplete_or_invalid_commit_records() {
        assert!(parse_commits(b"one\0two\0").is_err());
        assert!(
            parse_commits(concat!("not-an-oid\0\0Ada\0", "2026\0subject\0").as_bytes()).is_err()
        );
    }
}
