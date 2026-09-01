//! Parsing direct `ls-tree` children without losing path bytes.

use crate::domain::{ObjectId, RepoPath, TreeEntry, TreeKind};
use crate::git::GitError;

pub(crate) fn parse_tree_entries(input: &[u8]) -> Result<Vec<TreeEntry>, GitError> {
    input
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .map(parse_entry)
        .collect()
}

fn parse_entry(record: &[u8]) -> Result<TreeEntry, GitError> {
    let tab = record
        .iter()
        .position(|byte| *byte == b'\t')
        .ok_or_else(|| GitError::parse("tree", "entry has no path separator"))?;
    let metadata = std::str::from_utf8(&record[..tab])
        .map_err(|_| GitError::parse("tree", "entry metadata is not ASCII"))?;
    let mut fields = metadata.split(' ');
    let mode = fields
        .next()
        .ok_or_else(|| GitError::parse("tree", "entry has no mode"))?;
    let object_type = fields
        .next()
        .ok_or_else(|| GitError::parse("tree", "entry has no object type"))?;
    let object_id = fields
        .next()
        .ok_or_else(|| GitError::parse("tree", "entry has no object ID"))?;
    if fields.next().is_some() {
        return Err(GitError::parse("tree", "entry has extra metadata"));
    }
    let kind = match (object_type, mode) {
        ("tree", _) => TreeKind::Directory,
        ("commit", _) => TreeKind::Submodule,
        ("blob", "120000") => TreeKind::Symlink,
        ("blob", _) => TreeKind::File,
        _ => {
            return Err(GitError::parse(
                "tree",
                format!("unsupported object type {object_type}"),
            ));
        }
    };
    Ok(TreeEntry::new(
        ObjectId::parse(object_id.to_owned()).map_err(|detail| GitError::parse("tree", detail))?,
        mode.to_owned(),
        kind,
        RepoPath::from_bytes(record[tab + 1..].to_vec())
            .map_err(|detail| GitError::parse("tree", detail))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::parse_tree_entries;

    #[test]
    fn rejects_malformed_tree_entries() {
        assert!(parse_tree_entries(b"100644 blob missing-tab\0").is_err());
        assert!(
            parse_tree_entries(b"100644 mystery aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\tfile\0")
                .is_err()
        );
    }
}
