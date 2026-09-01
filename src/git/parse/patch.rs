//! Parsing unified patches into display lines and tracked hunk positions.

use bstr::ByteSlice;

use crate::domain::{DiffDocument, DiffLine, DiffLineKind, LineNumber};

pub(crate) fn parse_patch(input: &[u8], truncated: bool) -> DiffDocument {
    if input.is_empty() {
        return DiffDocument::Empty {
            message: "No change for this target.".to_owned(),
        };
    }
    let text = input.to_str_lossy();
    if text.lines().any(|line| {
        line.starts_with("Binary files ")
            || line == "GIT binary patch"
            || line.starts_with("Binary file ")
    }) {
        return DiffDocument::Binary {
            summary: text
                .lines()
                .find(|line| line.contains("Binary"))
                .unwrap_or("Binary content differs")
                .to_owned(),
        };
    }

    let mut old = None;
    let mut new = None;
    let lines = text
        .lines()
        .map(|line| {
            if line.starts_with("@@") {
                if let Some((old_start, new_start)) = hunk_starts(line) {
                    old = Some(old_start);
                    new = Some(new_start);
                }
                return DiffLine::new(DiffLineKind::Hunk, None, None, line.to_owned());
            }
            if line.starts_with("diff --git")
                || line.starts_with("index ")
                || line.starts_with("--- ")
                || line.starts_with("+++ ")
                || line.starts_with("new file mode ")
                || line.starts_with("deleted file mode ")
                || line.starts_with("similarity index ")
                || line.starts_with("rename from ")
                || line.starts_with("rename to ")
            {
                return DiffLine::new(DiffLineKind::Header, None, None, line.to_owned());
            }
            if line.starts_with('\\') {
                return DiffLine::new(DiffLineKind::Meta, None, None, line.to_owned());
            }
            if line.starts_with('+') {
                let current = new.map(LineNumber::new);
                new = new.map(|value| value.saturating_add(1));
                return DiffLine::new(DiffLineKind::Added, None, current, line.to_owned());
            }
            if line.starts_with('-') {
                let current = old.map(LineNumber::new);
                old = old.map(|value| value.saturating_add(1));
                return DiffLine::new(DiffLineKind::Removed, current, None, line.to_owned());
            }
            let old_current = old.map(LineNumber::new);
            let new_current = new.map(LineNumber::new);
            old = old.map(|value| value.saturating_add(1));
            new = new.map(|value| value.saturating_add(1));
            DiffLine::new(
                DiffLineKind::Context,
                old_current,
                new_current,
                line.to_owned(),
            )
        })
        .collect();

    if truncated {
        DiffDocument::Truncated {
            lines,
            bytes: input.len(),
        }
    } else {
        DiffDocument::Text {
            lines,
            bytes: input.len(),
        }
    }
}

fn hunk_starts(line: &str) -> Option<(u32, u32)> {
    let mut fields = line.split_whitespace();
    if fields.next()? != "@@" {
        return None;
    }
    let old = parse_range(fields.next()?, '-')?;
    let new = parse_range(fields.next()?, '+')?;
    Some((old, new))
}

fn parse_range(value: &str, prefix: char) -> Option<u32> {
    value.strip_prefix(prefix)?.split(',').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::parse_patch;
    use crate::domain::{DiffDocument, DiffLineKind};

    #[test]
    fn tracks_old_and_new_line_numbers() {
        let patch = parse_patch(b"@@ -2,2 +2,2 @@\n old\n-removed\n+added\n", false);
        let DiffDocument::Text { lines, .. } = patch else {
            panic!("expected text diff");
        };
        assert_eq!(lines[2].kind(), DiffLineKind::Removed);
        assert_eq!(lines[2].old_line().map(|line| line.value()), Some(3));
        assert_eq!(lines[3].new_line().map(|line| line.value()), Some(3));
    }

    #[test]
    fn classifies_the_no_newline_marker_as_metadata() {
        let patch = parse_patch(
            b"@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file\n",
            false,
        );
        let DiffDocument::Text { lines, .. } = patch else {
            panic!("expected text diff");
        };
        assert_eq!(
            lines.last().map(|line| line.kind()),
            Some(DiffLineKind::Meta)
        );
    }
}
