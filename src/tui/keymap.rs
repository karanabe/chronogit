//! Translation from terminal key events to semantic application actions.
//!
//! Built-in Vim-oriented bindings can be selectively replaced by an optional
//! keymap file. Multi-key sequences are resolved without ambiguous prefixes and
//! expire after 750 milliseconds.

mod config;

use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{Action, SearchDirection};

pub use config::KeyMapError;

const SEQUENCE_TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct KeyStroke {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl KeyStroke {
    fn from_event(key: KeyEvent) -> Self {
        let mut modifiers = key.modifiers;
        if matches!(key.code, KeyCode::Char(_)) {
            modifiers.remove(KeyModifiers::SHIFT);
        }
        modifiers &= KeyModifiers::CONTROL | KeyModifiers::ALT;
        Self {
            code: key.code,
            modifiers,
        }
    }

    pub(super) fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Binding {
    sequence: Vec<KeyStroke>,
    action: Action,
}

impl Binding {
    pub(super) fn new(sequence: Vec<KeyStroke>, action: Action) -> Self {
        Self { sequence, action }
    }
}

/// Stateful key-to-action translator with support for multi-key sequences.
///
/// `Ctrl-C` is reserved for [`Action::Quit`] regardless of configuration. While
/// a search prompt is active, printable characters edit the query and pending
/// normal-mode sequences are cleared.
#[derive(Debug)]
pub struct KeyMapper {
    bindings: Vec<Binding>,
    pending: Vec<KeyStroke>,
    pending_since: Option<Instant>,
}

impl KeyMapper {
    /// Creates a mapper using all built-in bindings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: config::default_bindings(),
            pending: Vec::new(),
            pending_since: None,
        }
    }

    /// Loads built-in bindings with optional validated overrides.
    ///
    /// An explicit `path` must exist. With `None`, the mapper tries
    /// `$XDG_CONFIG_HOME/chronogit/keymap.conf` and then
    /// `~/.config/chronogit/keymap.conf`; a missing implicit file falls back to
    /// defaults.
    ///
    /// # Errors
    ///
    /// Returns [`KeyMapError`] when a selected file cannot be read, contains an
    /// unknown action or key, or introduces duplicate or prefix-ambiguous keys.
    pub fn load(path: Option<&Path>) -> Result<Self, KeyMapError> {
        Ok(Self {
            bindings: config::load_bindings(path)?,
            pending: Vec::new(),
            pending_since: None,
        })
    }

    /// Consumes one key event and returns a completed semantic action.
    ///
    /// `None` means the key is either unbound or is a valid prefix awaiting the
    /// next stroke. `search_input_active` switches printable keys to query-edit
    /// actions while retaining the reserved focus, confirmation, cancellation,
    /// and `Ctrl-C` controls.
    pub fn map(&mut self, key: KeyEvent, search_input_active: bool) -> Option<Action> {
        if matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.clear_pending();
            return Some(Action::Quit);
        }
        if search_input_active {
            self.clear_pending();
            return match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => Some(Action::ConfirmSearch),
                (KeyCode::Esc, _) => Some(Action::CancelSearch),
                (KeyCode::Char('j'), modifiers)
                    if modifiers.contains(KeyModifiers::CONTROL)
                        && !modifiers.contains(KeyModifiers::ALT) =>
                {
                    Some(Action::FocusRight)
                }
                (KeyCode::Char('k'), modifiers)
                    if modifiers.contains(KeyModifiers::CONTROL)
                        && !modifiers.contains(KeyModifiers::ALT) =>
                {
                    Some(Action::FocusLeft)
                }
                (KeyCode::Backspace, _) => Some(Action::DeleteSearch),
                (KeyCode::Char(character), modifiers)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    Some(Action::InsertSearch(character))
                }
                _ => None,
            };
        }

        if self
            .pending_since
            .is_some_and(|started| started.elapsed() > SEQUENCE_TIMEOUT)
        {
            self.clear_pending();
        }
        let stroke = KeyStroke::from_event(key);
        self.pending.push(stroke.clone());
        if let Some(action) = self.resolve_pending() {
            return Some(action);
        }
        if self
            .bindings
            .iter()
            .any(|binding| binding.sequence.starts_with(&self.pending))
        {
            self.pending_since.get_or_insert_with(Instant::now);
            return None;
        }

        self.clear_pending();
        self.pending.push(stroke);
        if let Some(action) = self.resolve_pending() {
            return Some(action);
        }
        if self
            .bindings
            .iter()
            .any(|binding| binding.sequence.starts_with(&self.pending))
        {
            self.pending_since = Some(Instant::now());
        } else {
            self.clear_pending();
        }
        None
    }

    fn resolve_pending(&mut self) -> Option<Action> {
        let action = self
            .bindings
            .iter()
            .find(|binding| binding.sequence == self.pending)
            .map(|binding| binding.action)?;
        let has_longer = self.bindings.iter().any(|binding| {
            binding.sequence.len() > self.pending.len()
                && binding.sequence.starts_with(&self.pending)
        });
        if has_longer {
            None
        } else {
            self.clear_pending();
            Some(action)
        }
    }

    fn clear_pending(&mut self) {
        self.pending.clear();
        self.pending_since = None;
    }
}

impl Default for KeyMapper {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) fn action_for_name(name: &str) -> Option<Action> {
    match name {
        "quit" => Some(Action::Quit),
        "show_changes" => Some(Action::ShowChanges),
        "show_history" => Some(Action::ShowHistory),
        "show_graph" => Some(Action::ShowGraph),
        "show_code" => Some(Action::ShowCode),
        "focus_previous" => Some(Action::FocusLeft),
        "focus_next" => Some(Action::FocusRight),
        "move_up" => Some(Action::MoveUp),
        "move_down" => Some(Action::MoveDown),
        "move_top" => Some(Action::MoveTop),
        "move_bottom" => Some(Action::MoveBottom),
        "half_page_up" => Some(Action::HalfPageUp),
        "half_page_down" => Some(Action::HalfPageDown),
        "scroll_left" => Some(Action::ScrollLeft),
        "scroll_right" => Some(Action::ScrollRight),
        "refresh" => Some(Action::Refresh),
        "toggle_message" => Some(Action::ToggleMessage),
        "toggle_details" => Some(Action::ToggleDetails),
        "toggle_tree" => Some(Action::ToggleTree),
        "activate" => Some(Action::Activate),
        "file_search" => Some(Action::OpenFileSearch),
        "content_search" => Some(Action::OpenContentSearch),
        "search_forward" => Some(Action::StartSearch(SearchDirection::Forward)),
        "search_backward" => Some(Action::StartSearch(SearchDirection::Backward)),
        "next_match" => Some(Action::NextMatch),
        "previous_match" => Some(Action::PreviousMatch),
        "toggle_help" => Some(Action::ToggleHelp),
        "close" => Some(Action::CloseOverlay),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::KeyMapper;
    use crate::app::Action;

    #[test]
    fn maps_navigation_sequences_graph_and_global_search() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), false),
            Some(Action::CloseOverlay)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), false),
            Some(Action::CloseOverlay)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT),
                false
            ),
            Some(Action::Quit)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
                false
            ),
            Some(Action::FocusRight)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
                false
            ),
            Some(Action::FocusLeft)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE), false),
            Some(Action::MoveDown)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE), false),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE), false),
            Some(Action::ScrollRight)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE), false),
            Some(Action::ShowGraph)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE), false),
            Some(Action::ShowCode)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE), false),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE), false),
            Some(Action::OpenFileSearch)
        );
    }

    #[test]
    fn search_input_accepts_q_and_reserves_control_keys() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), true),
            Some(Action::InsertSearch('q'))
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT), true),
            Some(Action::InsertSearch('Q'))
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE), true),
            Some(Action::DeleteSearch)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), true),
            Some(Action::ConfirmSearch)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
                true
            ),
            Some(Action::FocusRight)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
                true
            ),
            Some(Action::FocusLeft)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                true
            ),
            Some(Action::Quit)
        );
    }

    #[test]
    fn explicit_config_replaces_selected_defaults() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create temp directory: {error}"));
        let path = directory.path().join("keymap.conf");
        fs::write(
            &path,
            "[bindings]\nshow_graph = x\nfile_search = ctrl-p\ntoggle_tree = f\n",
        )
        .unwrap_or_else(|error| panic!("could not write keymap: {error}"));
        let mut mapper = KeyMapper::load(Some(&path)).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE), false),
            Some(Action::ShowGraph)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE), false),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                false
            ),
            Some(Action::OpenFileSearch)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE), false),
            Some(Action::ToggleTree)
        );
    }

    #[test]
    fn invalid_or_ambiguous_config_is_rejected() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create temp directory: {error}"));
        let path = directory.path().join("keymap.conf");
        fs::write(&path, "unknown = x\n")
            .unwrap_or_else(|error| panic!("could not write keymap: {error}"));
        assert!(KeyMapper::load(Some(&path)).is_err());
        fs::write(&path, "show_graph = space\nfile_search = space f\n")
            .unwrap_or_else(|error| panic!("could not write keymap: {error}"));
        assert!(KeyMapper::load(Some(&path)).is_err());
        fs::write(&path, "show_graph = f0\n")
            .unwrap_or_else(|error| panic!("could not write keymap: {error}"));
        assert!(KeyMapper::load(Some(&path)).is_err());
    }
}
