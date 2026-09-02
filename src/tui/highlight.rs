//! Bounded syntax highlighting for repository source displayed by the TUI.
//!
//! The embedded syntax set and theme are initialized once. Highlighting skips
//! unusually large inputs so an adversarial repository cannot make rendering
//! spend unbounded time in syntax parsing.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use two_face::re_exports::syntect::easy::HighlightLines;
use two_face::re_exports::syntect::highlighting::{FontStyle, Style as SyntectStyle, Theme};
use two_face::re_exports::syntect::parsing::{SyntaxReference, SyntaxSet};
use two_face::re_exports::syntect::util::LinesWithEndings;
use two_face::theme::EmbeddedThemeName;

use crate::domain::RepoPath;

const MAX_HIGHLIGHT_BYTES: usize = 512 * 1024;
const MAX_HIGHLIGHT_LINES: usize = 10_000;
const MAX_HIGHLIGHT_LINE_BYTES: usize = 4 * 1024;
const MAX_CACHE_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 256;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();

thread_local! {
    static CACHE: RefCell<HighlightCache> = RefCell::new(HighlightCache::default());
}

#[derive(Default)]
struct HighlightCache {
    entries: VecDeque<HighlightCacheEntry>,
    source_bytes: usize,
}

struct HighlightCacheEntry {
    path: Option<Vec<u8>>,
    source: String,
    highlighted: Vec<Vec<Span<'static>>>,
}

/// Highlights a complete source block and returns one span list per input line.
///
/// `None` means that the path is not recognized, parsing failed, or the input
/// crossed a rendering guardrail. Callers retain a plain-text fallback.
pub(super) fn highlight_code(
    code: &str,
    path: Option<&RepoPath>,
) -> Option<Vec<Vec<Span<'static>>>> {
    if code.is_empty() || exceeds_highlight_limits(code) {
        return None;
    }
    let path_bytes = path.map(|path| path.as_bytes().to_vec());
    if let Some(highlighted) =
        CACHE.with(|cache| cache.borrow_mut().get(path_bytes.as_deref(), code))
    {
        return Some(highlighted);
    }
    let syntax = find_syntax(path, code)?;
    let mut highlighter = HighlightLines::new(syntax, theme());
    let mut lines = Vec::new();
    for line in LinesWithEndings::from(code) {
        let ranges = highlighter.highlight_line(line, syntax_set()).ok()?;
        lines.push(highlighted_spans(ranges));
    }
    CACHE.with(|cache| {
        cache
            .borrow_mut()
            .insert(path_bytes, code.to_owned(), lines.clone());
    });
    Some(lines)
}

/// Checks aggregate limits before a caller constructs temporary source blocks.
pub(super) fn source_is_too_large(bytes: usize, lines: usize) -> bool {
    bytes > MAX_HIGHLIGHT_BYTES || lines > MAX_HIGHLIGHT_LINES
}

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(two_face::syntax::extra_newlines)
}

fn theme() -> &'static Theme {
    THEME.get_or_init(|| {
        two_face::theme::extra()
            .get(EmbeddedThemeName::CatppuccinMocha)
            .clone()
    })
}

fn find_syntax(path: Option<&RepoPath>, code: &str) -> Option<&'static SyntaxReference> {
    let syntaxes = syntax_set();
    let display = path.map(RepoPath::display);
    let by_path = display.as_deref().and_then(|display| {
        let path = Path::new(display);
        let file_name = path.file_name().and_then(|value| value.to_str())?;
        syntaxes.find_syntax_by_extension(file_name).or_else(|| {
            path.extension()
                .and_then(|value| value.to_str())
                .and_then(|extension| syntaxes.find_syntax_by_extension(extension))
        })
    });
    by_path.or_else(|| {
        code.lines()
            .next()
            .and_then(|line| syntaxes.find_syntax_by_first_line(line))
    })
}

fn exceeds_highlight_limits(code: &str) -> bool {
    code.len() > MAX_HIGHLIGHT_BYTES
        || code.lines().count() > MAX_HIGHLIGHT_LINES
        || code
            .lines()
            .any(|line| line.len() > MAX_HIGHLIGHT_LINE_BYTES)
}

fn highlighted_spans(ranges: Vec<(SyntectStyle, &str)>) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (style, text) in ranges {
        let text = text.trim_end_matches(['\n', '\r']);
        if !text.is_empty() {
            spans.push(Span::styled(text.to_owned(), terminal_style(style)));
        }
    }
    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}

fn terminal_style(style: SyntectStyle) -> Style {
    let mut result = Style::default().fg(Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ));
    // Scope backgrounds would obscure both the terminal and semantic diff
    // backgrounds. Italic and underline are deliberately omitted because they
    // are inconsistently rendered and visually noisy in terminals.
    if style.font_style.contains(FontStyle::BOLD) {
        result = result.add_modifier(Modifier::BOLD);
    }
    result
}

impl HighlightCache {
    fn get(&mut self, path: Option<&[u8]>, source: &str) -> Option<Vec<Vec<Span<'static>>>> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.path.as_deref() == path && entry.source.as_str() == source)?;
        let entry = self.entries.remove(index)?;
        let highlighted = entry.highlighted.clone();
        self.entries.push_back(entry);
        Some(highlighted)
    }

    fn insert(
        &mut self,
        path: Option<Vec<u8>>,
        source: String,
        highlighted: Vec<Vec<Span<'static>>>,
    ) {
        let source_len = source.len();
        if source_len > MAX_CACHE_SOURCE_BYTES {
            return;
        }
        self.source_bytes = self.source_bytes.saturating_add(source_len);
        self.entries.push_back(HighlightCacheEntry {
            path,
            source,
            highlighted,
        });
        while self.source_bytes > MAX_CACHE_SOURCE_BYTES || self.entries.len() > MAX_CACHE_ENTRIES {
            let Some(removed) = self.entries.pop_front() else {
                self.source_bytes = 0;
                break;
            };
            self.source_bytes = self.source_bytes.saturating_sub(removed.source.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{highlight_code, source_is_too_large};
    use crate::domain::RepoPath;

    #[test]
    fn highlights_rust_with_multiple_foreground_colors_and_no_backgrounds() {
        let path = RepoPath::from_bytes(b"src/example.rs".to_vec())
            .unwrap_or_else(|error| panic!("{error}"));
        let lines = highlight_code("pub fn answer() -> u32 { 42 }\n", Some(&path))
            .unwrap_or_else(|| panic!("Rust should have an embedded syntax"));
        let colors = lines[0]
            .iter()
            .filter_map(|span| span.style.fg)
            .collect::<std::collections::HashSet<_>>();

        assert!(colors.len() > 1, "Rust tokens should not be monochrome");
        assert!(lines[0].iter().all(|span| span.style.bg.is_none()));
    }

    #[test]
    fn recognizes_extensionless_file_names_and_shebangs() {
        let makefile =
            RepoPath::from_bytes(b"Makefile".to_vec()).unwrap_or_else(|error| panic!("{error}"));
        assert!(highlight_code("all:\n\t@true\n", Some(&makefile)).is_some());

        let script =
            RepoPath::from_bytes(b"tool".to_vec()).unwrap_or_else(|error| panic!("{error}"));
        assert!(highlight_code("#!/usr/bin/env python3\nprint('ok')\n", Some(&script)).is_some());
    }

    #[test]
    fn skips_oversized_or_pathless_plain_text() {
        assert!(source_is_too_large(512 * 1024 + 1, 1));
        assert!(highlight_code(&"x".repeat(4 * 1024 + 1), None).is_none());
        assert!(highlight_code("ordinary prose\n", None).is_none());
    }
}
