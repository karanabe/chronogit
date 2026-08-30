use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{Action, SearchDirection};

#[derive(Debug, Default)]
pub struct KeyMapper {
    pending_z_since: Option<Instant>,
}

impl KeyMapper {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn map(&mut self, key: KeyEvent, search_input_active: bool) -> Option<Action> {
        if matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(Action::Quit);
        }
        if search_input_active {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => Some(Action::ConfirmSearch),
                (KeyCode::Esc, _) => Some(Action::CancelSearch),
                (KeyCode::Backspace, _) => Some(Action::DeleteSearch),
                (KeyCode::Char(character), modifiers)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    Some(Action::InsertSearch(character))
                }
                _ => None,
            };
        }
        if let Some(started) = self.pending_z_since.take()
            && started.elapsed() <= Duration::from_millis(750)
        {
            return match key.code {
                KeyCode::Char('h') => Some(Action::ScrollLeft),
                KeyCode::Char('l') => Some(Action::ScrollRight),
                _ => self.map(key, false),
            };
        }
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), KeyModifiers::NONE) => Some(Action::Quit),
            (KeyCode::Char('c'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Quit)
            }
            (KeyCode::Char('1'), KeyModifiers::NONE) => Some(Action::ShowChanges),
            (KeyCode::Char('2'), KeyModifiers::NONE) => Some(Action::ShowHistory),
            (KeyCode::Char('j'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::FocusRight)
            }
            (KeyCode::Char('k'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::FocusLeft)
            }
            (KeyCode::Char('h'), KeyModifiers::NONE) => Some(Action::FocusLeft),
            (KeyCode::Char('l'), KeyModifiers::NONE) => Some(Action::FocusRight),
            (KeyCode::Char('j'), KeyModifiers::NONE) | (KeyCode::Down, _) => Some(Action::MoveDown),
            (KeyCode::Char('k'), KeyModifiers::NONE) | (KeyCode::Up, _) => Some(Action::MoveUp),
            (KeyCode::Char('g'), KeyModifiers::NONE) | (KeyCode::Home, _) => Some(Action::MoveTop),
            (KeyCode::Char('G'), _) | (KeyCode::End, _) => Some(Action::MoveBottom),
            (KeyCode::Char('d'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::HalfPageDown)
            }
            (KeyCode::Char('u'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::HalfPageUp)
            }
            (KeyCode::Char('r'), KeyModifiers::NONE) => Some(Action::Refresh),
            (KeyCode::Char('m'), KeyModifiers::NONE) => Some(Action::ToggleMessage),
            (KeyCode::Char('b'), KeyModifiers::NONE) => Some(Action::ToggleDetails),
            (KeyCode::Char('t'), KeyModifiers::NONE) => Some(Action::ToggleTree),
            (KeyCode::Char('/'), KeyModifiers::NONE) => {
                Some(Action::StartSearch(SearchDirection::Forward))
            }
            (KeyCode::Char('?'), _) => Some(Action::StartSearch(SearchDirection::Backward)),
            (KeyCode::Char('n'), KeyModifiers::NONE) => Some(Action::NextMatch),
            (KeyCode::Char('N'), _) => Some(Action::PreviousMatch),
            (KeyCode::F(1), _) => Some(Action::ToggleHelp),
            (KeyCode::Enter, _) | (KeyCode::Char(' '), KeyModifiers::NONE) => {
                Some(Action::Activate)
            }
            (KeyCode::Esc, _) => Some(Action::CloseOverlay),
            (KeyCode::Char('z'), KeyModifiers::NONE) => {
                self.pending_z_since = Some(Instant::now());
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::KeyMapper;
    use crate::app::Action;

    #[test]
    fn maps_vim_navigation_and_z_sequence() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE), false,),
            Some(Action::MoveDown)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE), false,),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE), false,),
            Some(Action::ScrollRight)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                false,
            ),
            Some(Action::Quit)
        );
    }

    #[test]
    fn search_input_captures_commands_as_query_text() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), true),
            Some(Action::InsertSearch('q'))
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
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                true,
            ),
            Some(Action::Quit)
        );
    }

    #[test]
    fn maps_search_directions_and_moves_help_to_f1() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE), false),
            Some(Action::StartSearch(crate::app::SearchDirection::Forward))
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT),
                false
            ),
            Some(Action::StartSearch(crate::app::SearchDirection::Backward))
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE), false),
            Some(Action::ToggleHelp)
        );
    }

    #[test]
    fn maps_control_j_and_k_to_vertical_pane_navigation() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
                false,
            ),
            Some(Action::FocusRight)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
                false,
            ),
            Some(Action::FocusLeft)
        );
    }

    #[test]
    fn maps_message_and_history_body_view_keys() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE), false),
            Some(Action::ToggleMessage)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE), false),
            Some(Action::ToggleDetails)
        );
    }
}
