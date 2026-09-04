//! Stateful forward and backward search within a loaded document.

use crate::domain::SourcePosition;

/// Direction used when finding and repeating in-document matches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchDirection {
    /// Search toward increasing line numbers, wrapping at the end.
    Forward,
    /// Search toward decreasing line numbers, wrapping at the beginning.
    Backward,
}

impl SearchDirection {
    /// Returns the Vim-style prompt character for this direction.
    #[must_use]
    pub fn prompt(self) -> char {
        match self {
            Self::Forward => '/',
            Self::Backward => '?',
        }
    }

    /// Returns the opposite search direction.
    #[must_use]
    pub fn reversed(self) -> Self {
        match self {
            Self::Forward => Self::Backward,
            Self::Backward => Self::Forward,
        }
    }
}

#[derive(Debug)]
struct SearchPrompt {
    direction: SearchDirection,
    input: String,
}

#[derive(Debug)]
pub(crate) struct SearchState {
    prompt: Option<SearchPrompt>,
    query: String,
    direction: SearchDirection,
    whole_word: bool,
    matches: Vec<SearchMatch>,
    current: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SearchMatch {
    line: usize,
    byte_column: usize,
}

impl SearchMatch {
    fn position(self) -> SourcePosition {
        SourcePosition::new(
            u32::try_from(self.line).unwrap_or(u32::MAX),
            self.byte_column,
        )
    }
}

impl SearchState {
    pub(crate) fn new() -> Self {
        Self {
            prompt: None,
            query: String::new(),
            direction: SearchDirection::Forward,
            whole_word: false,
            matches: Vec::new(),
            current: None,
        }
    }

    pub(crate) fn begin(&mut self, direction: SearchDirection) {
        self.prompt = Some(SearchPrompt {
            direction,
            input: String::new(),
        });
    }

    pub(crate) fn push(&mut self, character: char) {
        if let Some(prompt) = &mut self.prompt {
            prompt.input.push(character);
        }
    }

    pub(crate) fn pop(&mut self) {
        if let Some(prompt) = &mut self.prompt {
            prompt.input.pop();
        }
    }

    pub(crate) fn cancel_input(&mut self) {
        self.prompt = None;
    }

    pub(crate) fn clear(&mut self) {
        self.prompt = None;
        self.query.clear();
        self.whole_word = false;
        self.matches.clear();
        self.current = None;
    }

    pub(crate) fn is_input_active(&self) -> bool {
        self.prompt.is_some()
    }

    pub(crate) fn prompt_text(&self) -> Option<(SearchDirection, &str)> {
        self.prompt
            .as_ref()
            .map(|prompt| (prompt.direction, prompt.input.as_str()))
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn direction(&self) -> SearchDirection {
        self.direction
    }

    pub(crate) fn match_count(&self) -> usize {
        self.matches.len()
    }

    pub(crate) fn current_ordinal(&self) -> Option<usize> {
        self.current.map(|index| index.saturating_add(1))
    }

    pub(crate) fn current_line(&self) -> Option<usize> {
        self.current
            .and_then(|index| self.matches.get(index).map(|found| found.line))
    }

    pub(crate) fn current_position(&self) -> Option<SourcePosition> {
        self.current
            .and_then(|index| self.matches.get(index).copied())
            .map(SearchMatch::position)
    }

    pub(crate) fn is_match(&self, line: usize) -> bool {
        self.matches
            .binary_search_by_key(&line, |found| found.line)
            .is_ok()
    }

    pub(crate) fn confirm<'a>(
        &mut self,
        values: impl IntoIterator<Item = &'a str>,
        anchor: usize,
    ) -> Option<usize> {
        let prompt = self.prompt.take()?;
        self.direction = prompt.direction;
        if !prompt.input.is_empty() {
            self.query = prompt.input;
            self.whole_word = false;
        }
        self.matches = matching_positions(values, &self.query, self.whole_word);
        self.current = select_match(
            &self.matches,
            SearchMatch {
                line: anchor,
                byte_column: if self.direction == SearchDirection::Backward {
                    usize::MAX
                } else {
                    0
                },
            },
            self.direction,
            true,
        );
        self.current_line()
    }

    pub(crate) fn confirm_position<'a>(
        &mut self,
        values: impl IntoIterator<Item = &'a str>,
        anchor: SourcePosition,
    ) -> Option<SourcePosition> {
        let prompt = self.prompt.take()?;
        self.direction = prompt.direction;
        if !prompt.input.is_empty() {
            self.query = prompt.input;
            self.whole_word = false;
        }
        self.matches = matching_positions(values, &self.query, self.whole_word);
        self.current = select_match(
            &self.matches,
            SearchMatch {
                line: usize::try_from(anchor.line()).unwrap_or(usize::MAX),
                byte_column: anchor.byte_column(),
            },
            self.direction,
            false,
        );
        self.current_position()
    }

    pub(crate) fn select_next(&mut self, direction: SearchDirection) -> Option<usize> {
        self.select_next_position(direction, 1)
            .map(|position| usize::try_from(position.line()).unwrap_or(usize::MAX))
    }

    pub(crate) fn select_next_position(
        &mut self,
        direction: SearchDirection,
        count: usize,
    ) -> Option<SourcePosition> {
        let anchor = self.current_position().unwrap_or(SourcePosition::new(0, 0));
        self.select_from(anchor, direction, count)
    }

    pub(crate) fn repeat_position<'a>(
        &mut self,
        values: impl IntoIterator<Item = &'a str>,
        anchor: SourcePosition,
        direction: SearchDirection,
        count: usize,
    ) -> Option<SourcePosition> {
        self.matches = matching_positions(values, &self.query, self.whole_word);
        self.select_from(anchor, direction, count)
    }

    fn select_from(
        &mut self,
        anchor: SourcePosition,
        direction: SearchDirection,
        count: usize,
    ) -> Option<SourcePosition> {
        let anchor = SearchMatch {
            line: usize::try_from(anchor.line()).unwrap_or(usize::MAX),
            byte_column: anchor.byte_column(),
        };
        self.current = select_match(&self.matches, anchor, direction, false);
        if let Some(first) = self.current {
            let len = self.matches.len();
            let offset = count.max(1).saturating_sub(1) % len;
            self.current = Some(match direction {
                SearchDirection::Forward => (first + offset) % len,
                SearchDirection::Backward => (first + len - offset) % len,
            });
        }
        self.current_position()
    }

    pub(crate) fn search_word<'a>(
        &mut self,
        values: impl IntoIterator<Item = &'a str>,
        query: &str,
        whole_word: bool,
        anchor: SourcePosition,
        direction: SearchDirection,
        count: usize,
    ) -> Option<SourcePosition> {
        self.prompt = None;
        self.query = query.to_owned();
        self.direction = direction;
        self.whole_word = whole_word;
        self.matches = matching_positions(values, query, whole_word);
        self.select_from(anchor, direction, count)
    }
}

#[cfg(test)]
fn matching_indices<'a>(
    values: impl IntoIterator<Item = &'a str>,
    query: &str,
) -> Vec<SearchMatch> {
    matching_positions(values, query, false)
}

fn matching_positions<'a>(
    values: impl IntoIterator<Item = &'a str>,
    query: &str,
    whole_word: bool,
) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let case_sensitive = query.chars().any(char::is_uppercase);
    values
        .into_iter()
        .enumerate()
        .flat_map(|(line, value)| {
            match_starts(value, query, case_sensitive)
                .into_iter()
                .filter(move |(start, end)| !whole_word || is_whole_word(value, *start, *end))
                .map(move |(byte_column, _)| SearchMatch { line, byte_column })
        })
        .collect()
}

fn match_starts(value: &str, query: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    if case_sensitive {
        return overlapping_matches(value, query);
    }
    let folded_query = query.to_lowercase();
    if value.is_ascii() {
        return overlapping_matches(&value.to_ascii_lowercase(), &folded_query);
    }
    let mut folded = String::with_capacity(value.len());
    let mut boundaries = Vec::new();
    for (byte, character) in value.char_indices() {
        boundaries.push((folded.len(), byte));
        folded.extend(character.to_lowercase());
    }
    boundaries.push((folded.len(), value.len()));
    overlapping_matches(&folded, &folded_query)
        .into_iter()
        .filter_map(|(start, end)| {
            let start = boundaries
                .binary_search_by_key(&start, |(folded, _)| *folded)
                .ok()?;
            let end = boundaries
                .binary_search_by_key(&end, |(folded, _)| *folded)
                .ok()?;
            Some((boundaries[start].1, boundaries[end].1))
        })
        .collect()
}

fn overlapping_matches(value: &str, query: &str) -> Vec<(usize, usize)> {
    let mut matches = Vec::new();
    let mut offset = 0;
    while let Some(found) = value[offset..].find(query) {
        let start = offset + found;
        matches.push((start, start + query.len()));
        offset = start + value[start..].chars().next().map_or(1, char::len_utf8);
        if offset >= value.len() {
            break;
        }
    }
    matches
}

fn is_whole_word(value: &str, start: usize, end: usize) -> bool {
    let before = value[..start].chars().next_back();
    let after = value[end..].chars().next();
    before.is_none_or(|character| !is_keyword(character))
        && after.is_none_or(|character| !is_keyword(character))
}

fn is_keyword(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn select_match(
    matches: &[SearchMatch],
    anchor: SearchMatch,
    direction: SearchDirection,
    include_anchor: bool,
) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    match direction {
        SearchDirection::Forward => matches
            .iter()
            .position(|found| {
                if include_anchor {
                    *found >= anchor
                } else {
                    *found > anchor
                }
            })
            .or(Some(0)),
        SearchDirection::Backward => matches
            .iter()
            .rposition(|found| {
                if include_anchor {
                    *found <= anchor
                } else {
                    *found < anchor
                }
            })
            .or_else(|| matches.len().checked_sub(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::{SearchDirection, SearchState, matching_indices};

    #[test]
    fn matching_is_case_insensitive_unless_the_query_contains_uppercase() {
        let values = ["alpha", "ALPHA", "Alpha"];
        assert_eq!(
            matching_indices(values, "alpha")
                .into_iter()
                .map(|found| found.line)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(
            matching_indices(values, "Alpha")
                .into_iter()
                .map(|found| found.line)
                .collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn forward_and_backward_search_wrap_and_repeat() {
        let values = ["zero", "match one", "two", "match three"];
        let mut search = SearchState::new();
        search.begin(SearchDirection::Forward);
        for character in "match".chars() {
            search.push(character);
        }
        assert_eq!(search.confirm(values, 2), Some(3));
        assert_eq!(search.select_next(SearchDirection::Forward), Some(1));
        assert_eq!(search.select_next(SearchDirection::Backward), Some(3));
    }

    #[test]
    fn an_empty_prompt_repeats_the_previous_query() {
        let values = ["first hit", "second hit"];
        let mut search = SearchState::new();
        search.begin(SearchDirection::Forward);
        for character in "hit".chars() {
            search.push(character);
        }
        assert_eq!(search.confirm(values, 0), Some(0));
        search.begin(SearchDirection::Backward);
        assert_eq!(search.confirm(values, 1), Some(1));
        assert_eq!(search.query(), "hit");
    }

    #[test]
    fn positional_search_starts_after_or_before_the_cursor() {
        let values = ["cat cat"];
        let mut search = SearchState::new();
        search.begin(SearchDirection::Forward);
        for character in "cat".chars() {
            search.push(character);
        }
        assert_eq!(
            search.confirm_position(values, crate::domain::SourcePosition::new(0, 0)),
            Some(crate::domain::SourcePosition::new(0, 4))
        );

        search.begin(SearchDirection::Backward);
        assert_eq!(
            search.confirm_position(values, crate::domain::SourcePosition::new(0, 4)),
            Some(crate::domain::SourcePosition::new(0, 0))
        );
    }

    #[test]
    fn oversized_search_counts_wrap_without_iterating_the_count() {
        let mut search = SearchState::new();
        assert_eq!(
            search.search_word(
                ["cat cat cat"],
                "cat",
                true,
                crate::domain::SourcePosition::new(0, 0),
                SearchDirection::Forward,
                usize::MAX
            ),
            Some(crate::domain::SourcePosition::new(0, (usize::MAX % 3) * 4))
        );
        assert_eq!(
            search.repeat_position(
                ["cat cat cat"],
                crate::domain::SourcePosition::new(0, 0),
                SearchDirection::Backward,
                usize::MAX
            ),
            Some(crate::domain::SourcePosition::new(
                0,
                ((3 - usize::MAX % 3) % 3) * 4
            ))
        );
    }

    #[test]
    fn smart_case_matches_keep_unicode_offsets_and_overlapping_hits() {
        assert_eq!(
            super::match_starts("界İ cat CAT", "cat", false),
            vec![(6, 9), (10, 13)]
        );
        assert_eq!(super::match_starts("İi", "i\u{307}", false), vec![(0, 2)]);
        assert_eq!(
            super::match_starts("aaa", "aa", false),
            vec![(0, 2), (1, 3)]
        );
        assert_eq!(super::match_starts("AAA", "AA", true), vec![(0, 2), (1, 3)]);
        let line = "a".repeat(100_000);
        let query = format!("{}b", "a".repeat(1_000));
        assert!(super::match_starts(&line, &query, false).is_empty());
    }

    #[test]
    fn word_search_distinguishes_same_line_matches_and_word_boundaries() {
        let values = ["cat concatenate cat", "CAT"];
        let mut search = SearchState::new();
        assert_eq!(
            search.search_word(
                values,
                "cat",
                true,
                crate::domain::SourcePosition::new(0, 0),
                SearchDirection::Forward,
                1,
            ),
            Some(crate::domain::SourcePosition::new(0, 16))
        );
        assert_eq!(
            search.select_next_position(SearchDirection::Forward, 1),
            Some(crate::domain::SourcePosition::new(1, 0))
        );

        assert_eq!(
            search.search_word(
                values,
                "cat",
                false,
                crate::domain::SourcePosition::new(0, 0),
                SearchDirection::Forward,
                1,
            ),
            Some(crate::domain::SourcePosition::new(0, 7))
        );
    }
}
