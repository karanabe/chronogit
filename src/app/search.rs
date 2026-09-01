//! Stateful forward and backward search within a loaded document.

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
    matches: Vec<usize>,
    current: Option<usize>,
}

impl SearchState {
    pub(crate) fn new() -> Self {
        Self {
            prompt: None,
            query: String::new(),
            direction: SearchDirection::Forward,
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
            .and_then(|index| self.matches.get(index).copied())
    }

    pub(crate) fn is_match(&self, line: usize) -> bool {
        self.matches.binary_search(&line).is_ok()
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
        }
        self.matches = matching_indices(values, &self.query);
        self.current = select_match(&self.matches, anchor, self.direction, true);
        self.current_line()
    }

    pub(crate) fn select_next(&mut self, direction: SearchDirection) -> Option<usize> {
        if self.matches.is_empty() {
            self.current = None;
            return None;
        }
        let anchor = self.current_line().unwrap_or(0);
        self.current = select_match(&self.matches, anchor, direction, false);
        self.current_line()
    }
}

fn matching_indices<'a>(values: impl IntoIterator<Item = &'a str>, query: &str) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }
    let case_sensitive = query.chars().any(char::is_uppercase);
    let folded_query = (!case_sensitive).then(|| query.to_lowercase());
    values
        .into_iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let matches = if let Some(folded_query) = &folded_query {
                value.to_lowercase().contains(folded_query)
            } else {
                value.contains(query)
            };
            matches.then_some(index)
        })
        .collect()
}

fn select_match(
    matches: &[usize],
    anchor: usize,
    direction: SearchDirection,
    include_anchor: bool,
) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    match direction {
        SearchDirection::Forward => matches
            .iter()
            .position(|line| {
                if include_anchor {
                    *line >= anchor
                } else {
                    *line > anchor
                }
            })
            .or(Some(0)),
        SearchDirection::Backward => matches
            .iter()
            .rposition(|line| {
                if include_anchor {
                    *line <= anchor
                } else {
                    *line < anchor
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
        assert_eq!(matching_indices(values, "alpha"), vec![0, 1, 2]);
        assert_eq!(matching_indices(values, "Alpha"), vec![2]);
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
}
