//! Parsing NUL-delimited repository file and fixed-text match records.

use bstr::ByteSlice;

use crate::domain::{RepoPath, SearchHit};
use crate::git::GitError;

pub(crate) fn parse_file_paths(input: &[u8]) -> Result<Vec<SearchHit>, GitError> {
    input
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .map(|field| {
            RepoPath::from_bytes(field.to_vec())
                .map(SearchHit::file)
                .map_err(|detail| GitError::parse("repository file list", detail))
        })
        .collect()
}

pub(crate) fn parse_grep_matches(mut input: &[u8]) -> Result<Vec<SearchHit>, GitError> {
    let mut matches = Vec::new();
    while !input.is_empty() {
        let Some(path_end) = input.find_byte(0) else {
            return Err(GitError::parse(
                "content search",
                "result is missing the path separator",
            ));
        };
        let path = RepoPath::from_bytes(input[..path_end].to_vec())
            .map_err(|detail| GitError::parse("content search path", detail))?;
        input = &input[path_end + 1..];

        let Some(line_end) = input.find_byte(0) else {
            return Err(GitError::parse(
                "content search",
                "result is missing the line-number separator",
            ));
        };
        let line = std::str::from_utf8(&input[..line_end])
            .map_err(|_| GitError::parse("content search", "line number is not ASCII"))?
            .parse::<u32>()
            .map_err(|_| GitError::parse("content search", "line number is invalid"))?;
        input = &input[line_end + 1..];

        let preview_end = input.find_byte(b'\n').unwrap_or(input.len());
        let preview = input[..preview_end].to_str_lossy().into_owned();
        matches.push(SearchHit::content(path, line, preview));
        input = if preview_end == input.len() {
            &[]
        } else {
            &input[preview_end + 1..]
        };
    }
    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::{parse_file_paths, parse_grep_matches};

    #[test]
    fn parses_nul_delimited_files_and_grep_records() {
        let files = parse_file_paths(b"src/main.rs\0weird\nname.txt\0")
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(files.len(), 2);
        assert_eq!(files[1].path().as_bytes(), b"weird\nname.txt");

        let matches = parse_grep_matches(b"src/main.rs\x0012\0let needle = true;\n")
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line(), Some(12));
        assert_eq!(matches[0].preview(), "let needle = true;");
    }

    #[test]
    fn rejects_incomplete_grep_records() {
        assert!(parse_grep_matches(b"src/main.rs\0not-a-line\0text\n").is_err());
        assert!(parse_grep_matches(b"src/main.rs").is_err());
    }
}
