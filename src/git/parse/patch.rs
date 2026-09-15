//! Parsing unified patches into display lines and tracked hunk positions.

use bstr::ByteSlice;

use crate::domain::{DiffDocument, DiffLine, DiffLineKind, LineNumber};
use crate::git::OutputCompleteness;

pub(crate) fn parse_patch(input: &[u8], completeness: OutputCompleteness) -> DiffDocument {
    if input.is_empty() && completeness == OutputCompleteness::Complete {
        return DiffDocument::Empty {
            message: "No change for this target.".to_owned(),
        };
    }
    // A capture limit can split both a patch line and a UTF-8 character. Only
    // complete lines are authoritative enough to assign source positions.
    let retained = if completeness.is_truncated() {
        let end = input
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        &input[..end]
    } else {
        input
    };
    let text = retained.to_str_lossy();
    if let Some(summary) = text.lines().find(|line| {
        line.starts_with("Binary files ")
            || *line == "GIT binary patch"
            || line.starts_with("Binary file ")
    }) {
        return DiffDocument::Binary {
            summary: summary.to_owned(),
        };
    }

    let mut state = PatchState::Metadata;
    let lines = text.lines().map(|line| state.parse_line(line)).collect();
    if completeness.is_truncated() {
        DiffDocument::Truncated {
            lines,
            bytes: retained.len(),
        }
    } else {
        DiffDocument::Text {
            lines,
            bytes: input.len(),
        }
    }
}

/// Only an ordinary, validated two-sided hunk can assign source line numbers.
enum PatchState {
    Metadata,
    Hunk(HunkCursor),
}

impl PatchState {
    fn parse_line(&mut self, line: &str) -> DiffLine {
        if line.starts_with("@@") {
            *self = HunkCursor::parse(line).map_or(Self::Metadata, Self::Hunk);
            return DiffLine::new(DiffLineKind::Hunk, None, None, line.to_owned());
        }
        if line.starts_with('\\') {
            return DiffLine::new(DiffLineKind::Meta, None, None, line.to_owned());
        }
        if let Self::Hunk(hunk) = self
            && let Some(content) = hunk.parse_line(line)
        {
            return content;
        }
        // The hunk has ended or is malformed. Do not carry its counters into
        // another file, a combined diff, or an extended metadata header.
        *self = Self::Metadata;
        let kind = if [
            "diff --",
            "index ",
            "--- ",
            "+++ ",
            "old mode ",
            "new mode ",
            "new file mode ",
            "deleted file mode ",
            "similarity index ",
            "dissimilarity index ",
            "rename from ",
            "rename to ",
            "copy from ",
            "copy to ",
        ]
        .iter()
        .any(|prefix| line.starts_with(prefix))
        {
            DiffLineKind::Header
        } else {
            DiffLineKind::Meta
        };
        DiffLine::new(kind, None, None, line.to_owned())
    }
}

struct HunkCursor {
    old: HunkSide,
    new: HunkSide,
}

impl HunkCursor {
    fn parse(header: &str) -> Option<Self> {
        let mut fields = header.split_whitespace();
        if fields.next()? != "@@" {
            return None;
        }
        let old = HunkSide::parse(fields.next()?.strip_prefix('-')?)?;
        let new = HunkSide::parse(fields.next()?.strip_prefix('+')?)?;
        if fields.next()? != "@@" {
            return None;
        }
        Some(Self { old, new })
    }

    fn parse_line(&mut self, line: &str) -> Option<DiffLine> {
        let (kind, old, new) = match line.as_bytes().first()? {
            b'+' => (DiffLineKind::Added, None, Some(self.new.take()?)),
            b'-' => (DiffLineKind::Removed, Some(self.old.take()?), None),
            b' ' if self.old.remaining > 0 && self.new.remaining > 0 => (
                DiffLineKind::Context,
                Some(self.old.take()?),
                Some(self.new.take()?),
            ),
            _ => return None,
        };
        Some(DiffLine::new(kind, old, new, line.to_owned()))
    }
}

/// An empty range may start at zero; a non-empty range must fit in one-based
/// source coordinates. Keeping the count prevents metadata from consuming lines.
struct HunkSide {
    next: u32,
    remaining: u32,
}

impl HunkSide {
    fn parse(range: &str) -> Option<Self> {
        let (start, count) = range.split_once(',').unwrap_or((range, "1"));
        let next = start.parse::<u32>().ok()?;
        let remaining = count.parse::<u32>().ok()?;
        if remaining > 0 {
            LineNumber::new(next)?;
            next.checked_add(remaining - 1)?;
        }
        Some(Self { next, remaining })
    }

    fn take(&mut self) -> Option<LineNumber> {
        if self.remaining == 0 {
            return None;
        }
        let line = LineNumber::new(self.next)?;
        self.remaining -= 1;
        self.next = self.next.saturating_add(1);
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_patch;
    use crate::domain::{DiffDocument, DiffLineKind};
    use crate::git::OutputCompleteness;

    #[test]
    fn header_like_content_keeps_its_hunk_positions() {
        let patch = parse_patch(
            b"--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n--- removed\n+++ added\n context\n",
            OutputCompleteness::Complete,
        );
        let lines = patch.lines();
        assert_eq!(lines[3].kind(), DiffLineKind::Removed);
        assert_eq!(lines[3].old_line().map(|line| line.value()), Some(1));
        assert_eq!(lines[4].kind(), DiffLineKind::Added);
        assert_eq!(lines[4].new_line().map(|line| line.value()), Some(1));
        assert_eq!(lines[5].old_line().map(|line| line.value()), Some(2));
        assert_eq!(lines[5].new_line().map(|line| line.value()), Some(2));
    }

    #[test]
    fn metadata_and_invalid_hunks_do_not_inherit_source_positions() {
        for suffix in [
            "diff --git a/next b/next\nold mode 100644\nnew mode 100755\n",
            "@@ -broken +2 @@\n+unknown\n",
            "@@@ -1,1 -1,1 +1,1 @@@\n++combined\n",
        ] {
            let input = format!("@@ -1 +1 @@\n-old\n+new\n{suffix}");
            let patch = parse_patch(input.as_bytes(), OutputCompleteness::Complete);
            assert!(
                patch.lines()[3..]
                    .iter()
                    .all(|line| { line.old_line().is_none() && line.new_line().is_none() }),
                "{suffix}"
            );
        }
    }

    #[test]
    fn truncated_patches_retain_only_complete_lines() {
        let patch = parse_patch(
            b"@@ -0,0 +1,2 @@\n+complete\n+part",
            OutputCompleteness::Truncated,
        );
        assert!(patch.is_truncated());
        assert_eq!(patch.lines().len(), 2);
        let empty = parse_patch(b"", OutputCompleteness::Truncated);
        assert!(empty.is_truncated());
    }

    #[test]
    fn empty_and_boundary_hunk_ranges_keep_only_valid_positions() {
        let patch = parse_patch(
            b"@@ -0,0 +1 @@\n+first\n@@ -4294967295 +1,0 @@\n-last\n@@ -1 +4294967295,2 @@\n+overflow\n@@ -1 +0 @@\n+zero\n",
            OutputCompleteness::Complete,
        );
        let lines = patch.lines();
        assert_eq!(lines[1].new_line().map(|line| line.value()), Some(1));
        assert!(lines[1].old_line().is_none());
        assert_eq!(lines[3].old_line().map(|line| line.value()), Some(u32::MAX));
        assert!(lines[3].new_line().is_none());
        assert!(
            lines[4..]
                .iter()
                .all(|line| line.old_line().is_none() && line.new_line().is_none())
        );
    }

    #[test]
    fn tracks_old_and_new_line_numbers() {
        let patch = parse_patch(
            b"@@ -2,2 +2,2 @@\n old\n-removed\n+added\n",
            OutputCompleteness::Complete,
        );
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
            OutputCompleteness::Complete,
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
