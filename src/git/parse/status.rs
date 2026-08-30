use crate::domain::{ChangeKind, RepoPath, WorktreeChange};
use crate::git::GitError;

pub(crate) fn parse_status(input: &[u8]) -> Result<Vec<WorktreeChange>, GitError> {
    let records: Vec<&[u8]> = input.split(|byte| *byte == 0).collect();
    let mut changes = Vec::new();
    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        index += 1;
        if record.is_empty() {
            continue;
        }
        match record.first().copied() {
            Some(b'1') => {
                if worktree_status(record)? == b'.' {
                    continue;
                }
                let fields: Vec<&[u8]> = record.splitn(9, |byte| *byte == b' ').collect();
                let path = field(&fields, 8, "ordinary status path")?;
                changes.push(WorktreeChange::new(
                    repo_path(path, "ordinary status path")?,
                    None,
                    kind_from_status(worktree_status(record)?)?,
                ));
            }
            Some(b'2') => {
                if worktree_status(record)? == b'.' {
                    if index < records.len() {
                        index += 1;
                    }
                    continue;
                }
                let fields: Vec<&[u8]> = record.splitn(10, |byte| *byte == b' ').collect();
                let path = field(&fields, 9, "renamed status path")?;
                let original = records.get(index).copied().ok_or_else(|| {
                    GitError::parse("status", "rename record has no original path")
                })?;
                index += 1;
                changes.push(WorktreeChange::new(
                    repo_path(path, "renamed status path")?,
                    Some(repo_path(original, "original status path")?),
                    kind_from_status(worktree_status(record)?)?,
                ));
            }
            Some(b'u') => {
                let fields: Vec<&[u8]> = record.splitn(11, |byte| *byte == b' ').collect();
                let path = field(&fields, 10, "unmerged status path")?;
                changes.push(WorktreeChange::new(
                    repo_path(path, "unmerged status path")?,
                    None,
                    ChangeKind::Unmerged,
                ));
            }
            Some(b'?') if record.get(1) == Some(&b' ') => {
                changes.push(WorktreeChange::new(
                    repo_path(&record[2..], "untracked status path")?,
                    None,
                    ChangeKind::Untracked,
                ));
            }
            Some(b'!') => {}
            _ => {
                return Err(GitError::parse(
                    "status",
                    format!("unknown porcelain v2 record type: {record:?}"),
                ));
            }
        }
    }
    Ok(changes)
}

fn worktree_status(record: &[u8]) -> Result<u8, GitError> {
    record
        .get(3)
        .copied()
        .ok_or_else(|| GitError::parse("status", "record does not contain XY status"))
}

fn field<'a>(
    fields: &'a [&'a [u8]],
    index: usize,
    context: &'static str,
) -> Result<&'a [u8], GitError> {
    fields
        .get(index)
        .copied()
        .filter(|field| !field.is_empty())
        .ok_or_else(|| GitError::parse("status", format!("missing {context}")))
}

fn repo_path(bytes: &[u8], context: &'static str) -> Result<RepoPath, GitError> {
    RepoPath::from_bytes(bytes.to_vec())
        .map_err(|detail| GitError::parse("status", format!("invalid {context}: {detail}")))
}

fn kind_from_status(status: u8) -> Result<ChangeKind, GitError> {
    match status {
        b'M' => Ok(ChangeKind::Modified),
        b'A' => Ok(ChangeKind::Added),
        b'D' => Ok(ChangeKind::Deleted),
        b'R' => Ok(ChangeKind::Renamed),
        b'C' => Ok(ChangeKind::Copied),
        b'T' => Ok(ChangeKind::TypeChanged),
        b'U' => Ok(ChangeKind::Unmerged),
        other => Err(GitError::parse(
            "status",
            format!("unknown worktree status {other:?}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_status;
    use crate::domain::ChangeKind;

    #[test]
    fn excludes_staged_only_and_parses_untracked_and_rename() {
        let input = b"1 M. N... 100644 100644 100644 abc def staged.txt\0\
1 .M N... 100644 100644 100644 abc def modified.txt\0\
2 .R N... 100644 100644 100644 abc def R100 new name.txt\0old name.txt\0\
? new.txt\0";
        let changes = parse_status(input).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].kind(), ChangeKind::Modified);
        assert_eq!(changes[1].kind(), ChangeKind::Renamed);
        assert_eq!(changes[2].kind(), ChangeKind::Untracked);
    }

    #[test]
    fn rejects_unknown_or_incomplete_records() {
        assert!(parse_status(b"1 .X N... 100644 100644 100644 abc def file\0").is_err());
        assert!(parse_status(b"2 .R incomplete\0").is_err());
        assert!(parse_status(b"x unsupported\0").is_err());
    }
}
