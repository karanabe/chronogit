//! Parsing NUL-delimited committed-file name-status records.

use crate::domain::{ChangeKind, ChangedFile, RepoPath};
use crate::git::GitError;

pub(crate) fn parse_changed_files(input: &[u8]) -> Result<Vec<ChangedFile>, GitError> {
    let tokens: Vec<&[u8]> = input
        .split(|byte| *byte == 0)
        .filter(|token| !token.is_empty())
        .collect();
    let mut files = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let status = tokens[index];
        index += 1;
        let code = status
            .first()
            .copied()
            .ok_or_else(|| GitError::parse("changed files", "empty status token"))?;
        let kind = kind(code)?;
        if matches!(kind, ChangeKind::Renamed | ChangeKind::Copied) {
            let original = next_path(&tokens, &mut index)?;
            let path = next_path(&tokens, &mut index)?;
            files.push(ChangedFile::new(path, Some(original), kind));
        } else {
            files.push(ChangedFile::new(
                next_path(&tokens, &mut index)?,
                None,
                kind,
            ));
        }
    }
    Ok(files)
}

fn next_path(tokens: &[&[u8]], index: &mut usize) -> Result<RepoPath, GitError> {
    let token = tokens
        .get(*index)
        .copied()
        .ok_or_else(|| GitError::parse("changed files", "status has no path"))?;
    *index += 1;
    RepoPath::from_bytes(token.to_vec()).map_err(|detail| GitError::parse("changed files", detail))
}

fn kind(code: u8) -> Result<ChangeKind, GitError> {
    match code {
        b'M' => Ok(ChangeKind::Modified),
        b'A' => Ok(ChangeKind::Added),
        b'D' => Ok(ChangeKind::Deleted),
        b'R' => Ok(ChangeKind::Renamed),
        b'C' => Ok(ChangeKind::Copied),
        b'T' => Ok(ChangeKind::TypeChanged),
        b'U' => Ok(ChangeKind::Unmerged),
        other => Err(GitError::parse(
            "changed files",
            format!("unknown status code {other:?}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_changed_files;

    #[test]
    fn parses_rename_triplet() {
        let files = parse_changed_files(b"R100\0old name\0new name\0")
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path().display(), "new name");
        assert_eq!(
            files[0].original_path().map(|path| path.display()),
            Some("old name".to_owned())
        );
    }

    #[test]
    fn rejects_unknown_and_incomplete_status_records() {
        assert!(parse_changed_files(b"X\0file\0").is_err());
        assert!(parse_changed_files(b"R100\0old\0").is_err());
    }
}
