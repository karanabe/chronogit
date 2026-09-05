//! Translation from terminal key events to semantic application actions.
//!
//! Built-in Vim-oriented bindings can be selectively replaced by an optional
//! keymap file. Multi-key sequences are resolved without ambiguous prefixes and
//! expire after 750 milliseconds.

mod config;

use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{Action, SearchDirection, SemanticNavigationKind, VimMotion, VimMotionKind};

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
        modifiers &= KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT;
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
    count: Option<usize>,
    awaiting_target: Option<VimMotion>,
    awaiting_mark: Option<MarkCommand>,
    last_character_search: Option<VimMotion>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MarkCommand {
    Set,
    Jump { linewise: bool, record_jump: bool },
}

impl KeyMapper {
    /// Creates a mapper using all built-in bindings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: config::default_bindings(),
            pending: Vec::new(),
            pending_since: None,
            count: None,
            awaiting_target: None,
            awaiting_mark: None,
            last_character_search: None,
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
            count: None,
            awaiting_target: None,
            awaiting_mark: None,
            last_character_search: None,
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
            self.clear_command();
            return Some(Action::Quit);
        }
        if search_input_active {
            self.clear_command();
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

        if let Some(command) = self.awaiting_mark.take() {
            self.pending_since = None;
            self.count = None;
            return match (key.code, key.modifiers) {
                (KeyCode::Esc, _) => None,
                (KeyCode::Char(mark), modifiers)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    Some(match command {
                        MarkCommand::Set => Action::SetVimMark(mark),
                        MarkCommand::Jump {
                            linewise,
                            record_jump,
                        } => Action::JumpToVimMark {
                            mark,
                            linewise,
                            record_jump,
                        },
                    })
                }
                _ => None,
            };
        }

        if let Some(motion) = self.awaiting_target.take() {
            self.pending_since = None;
            return match (key.code, key.modifiers) {
                (KeyCode::Esc, _) => {
                    self.count = None;
                    None
                }
                (KeyCode::Char(target), modifiers)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    let motion = motion.targeting(target);
                    self.last_character_search = Some(motion);
                    self.count = None;
                    Some(Action::VimMotion(motion))
                }
                _ => {
                    self.count = None;
                    None
                }
            };
        }

        if self
            .pending_since
            .is_some_and(|started| started.elapsed() > SEQUENCE_TIMEOUT)
        {
            self.clear_command();
        }
        if self.pending.is_empty()
            && let (KeyCode::Char(digit), modifiers) = (key.code, key.modifiers)
            && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            && digit.is_ascii_digit()
            && (digit != '0' || self.count.is_some())
        {
            let digit = digit.to_digit(10).unwrap_or(0) as usize;
            self.count = Some(
                self.count
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(digit),
            );
            return None;
        }
        let stroke = KeyStroke::from_event(key);
        self.pending.push(stroke.clone());
        if let Some(action) = self.resolve_pending() {
            return self.finish_action(action);
        }
        if self
            .bindings
            .iter()
            .any(|binding| binding.sequence.starts_with(&self.pending))
        {
            self.pending_since.get_or_insert_with(Instant::now);
            return None;
        }

        self.clear_command();
        self.pending.push(stroke);
        if let Some(action) = self.resolve_pending() {
            return self.finish_action(action);
        }
        if self
            .bindings
            .iter()
            .any(|binding| binding.sequence.starts_with(&self.pending))
        {
            self.pending_since = Some(Instant::now());
        } else {
            self.clear_command();
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
            self.pending.clear();
            self.pending_since = None;
            Some(action)
        }
    }

    fn finish_action(&mut self, action: Action) -> Option<Action> {
        let count = self.count.take();
        match action {
            Action::SetVimMark('\0') => {
                self.awaiting_mark = Some(MarkCommand::Set);
                return None;
            }
            Action::JumpToVimMark {
                mark: '\0',
                linewise,
                record_jump,
            } => {
                self.awaiting_mark = Some(MarkCommand::Jump {
                    linewise,
                    record_jump,
                });
                return None;
            }
            _ => {}
        }
        if matches!(action, Action::JumpListBack(_) | Action::JumpListForward(_)) {
            return Some(match action {
                Action::JumpListBack(_) => Action::JumpListBack(count.unwrap_or(1).max(1)),
                Action::JumpListForward(_) => Action::JumpListForward(count.unwrap_or(1).max(1)),
                _ => unreachable!(),
            });
        }
        let Action::VimMotion(mut motion) = action else {
            return Some(action);
        };
        motion = motion.counted(count.unwrap_or(1), count.is_some());
        if motion.kind() == VimMotionKind::MatchingPair && motion.has_explicit_count() {
            motion = VimMotion::new(VimMotionKind::BufferPercentage)
                .counted(motion.count().min(100), true);
        }
        if matches!(
            motion.kind(),
            VimMotionKind::BufferTop | VimMotionKind::BufferBottom
        ) && motion.has_explicit_count()
        {
            motion = VimMotion::new(VimMotionKind::BufferTop).counted(motion.count(), true);
        }
        if matches!(
            motion.kind(),
            VimMotionKind::FindForward
                | VimMotionKind::FindBackward
                | VimMotionKind::TillForward
                | VimMotionKind::TillBackward
        ) && motion.target().is_none()
        {
            self.awaiting_target = Some(motion);
            self.pending_since = Some(Instant::now());
            return None;
        }
        if matches!(
            motion.kind(),
            VimMotionKind::RepeatCharacterSearch | VimMotionKind::ReverseCharacterSearch
        ) {
            let mut repeated = self.last_character_search?;
            if motion.kind() == VimMotionKind::ReverseCharacterSearch {
                repeated = VimMotion::new(reverse_character_search(repeated.kind()))
                    .counted(motion.count(), motion.has_explicit_count())
                    .targeting(repeated.target()?);
            } else {
                repeated = repeated.counted(motion.count(), motion.has_explicit_count());
            }
            return Some(Action::VimMotion(repeated.repeating()));
        }
        Some(Action::VimMotion(motion))
    }

    fn clear_command(&mut self) {
        self.pending.clear();
        self.pending_since = None;
        self.count = None;
        self.awaiting_target = None;
        self.awaiting_mark = None;
    }
}

fn reverse_character_search(kind: VimMotionKind) -> VimMotionKind {
    match kind {
        VimMotionKind::FindForward => VimMotionKind::FindBackward,
        VimMotionKind::FindBackward => VimMotionKind::FindForward,
        VimMotionKind::TillForward => VimMotionKind::TillBackward,
        VimMotionKind::TillBackward => VimMotionKind::TillForward,
        _ => kind,
    }
}

impl Default for KeyMapper {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) fn action_for_name(name: &str) -> Option<Action> {
    let motion = |kind| Action::VimMotion(VimMotion::new(kind));
    match name {
        "quit" => Some(Action::Quit),
        "show_changes" => Some(Action::ShowChanges),
        "show_history" => Some(Action::ShowHistory),
        "show_graph" => Some(Action::ShowGraph),
        "show_code" => Some(Action::ShowCode),
        "focus_previous" => Some(Action::FocusLeft),
        "focus_next" => Some(Action::FocusRight),
        "move_up" => Some(motion(VimMotionKind::Up)),
        "move_down" => Some(motion(VimMotionKind::Down)),
        "move_top" => Some(motion(VimMotionKind::BufferTop)),
        "move_bottom" => Some(motion(VimMotionKind::BufferBottom)),
        "move_bottom_end" => Some(motion(VimMotionKind::BufferBottomEnd)),
        "half_page_up" => Some(motion(VimMotionKind::HalfPageUp)),
        "half_page_down" => Some(motion(VimMotionKind::HalfPageDown)),
        "page_up" => Some(motion(VimMotionKind::PageUp)),
        "page_down" => Some(motion(VimMotionKind::PageDown)),
        "scroll_line_up" => Some(motion(VimMotionKind::ScrollLineUp)),
        "scroll_line_down" => Some(motion(VimMotionKind::ScrollLineDown)),
        "scroll_left" => Some(motion(VimMotionKind::ScrollColumnLeft)),
        "scroll_right" => Some(motion(VimMotionKind::ScrollColumnRight)),
        "cursor_left" => Some(motion(VimMotionKind::Left)),
        "cursor_right" => Some(motion(VimMotionKind::Right)),
        "cursor_left_wrap" => Some(motion(VimMotionKind::LeftWrap)),
        "cursor_right_wrap" => Some(motion(VimMotionKind::RightWrap)),
        "line_start" => Some(motion(VimMotionKind::LineStart)),
        "first_non_blank" => Some(motion(VimMotionKind::FirstNonBlank)),
        "line_end" => Some(motion(VimMotionKind::LineEnd)),
        "last_non_blank" => Some(motion(VimMotionKind::LastNonBlank)),
        "screen_line_start" => Some(motion(VimMotionKind::ScreenLineStart)),
        "screen_first_non_blank" => Some(motion(VimMotionKind::ScreenFirstNonBlank)),
        "screen_line_end" => Some(motion(VimMotionKind::ScreenLineEnd)),
        "screen_last_non_blank" => Some(motion(VimMotionKind::ScreenLastNonBlank)),
        "screen_middle" => Some(motion(VimMotionKind::ScreenMiddle)),
        "line_middle" => Some(motion(VimMotionKind::LineMiddle)),
        "column" => Some(motion(VimMotionKind::Column)),
        "byte_offset" => Some(motion(VimMotionKind::ByteOffset)),
        "word_forward" => Some(motion(VimMotionKind::WordForward)),
        "word_backward" => Some(motion(VimMotionKind::WordBackward)),
        "word_end_forward" => Some(motion(VimMotionKind::WordEndForward)),
        "word_end_backward" => Some(motion(VimMotionKind::WordEndBackward)),
        "big_word_forward" => Some(motion(VimMotionKind::BigWordForward)),
        "big_word_backward" => Some(motion(VimMotionKind::BigWordBackward)),
        "big_word_end_forward" => Some(motion(VimMotionKind::BigWordEndForward)),
        "big_word_end_backward" => Some(motion(VimMotionKind::BigWordEndBackward)),
        "find_forward" => Some(motion(VimMotionKind::FindForward)),
        "find_backward" => Some(motion(VimMotionKind::FindBackward)),
        "till_forward" => Some(motion(VimMotionKind::TillForward)),
        "till_backward" => Some(motion(VimMotionKind::TillBackward)),
        "repeat_character_search" => Some(motion(VimMotionKind::RepeatCharacterSearch)),
        "reverse_character_search" => Some(motion(VimMotionKind::ReverseCharacterSearch)),
        "previous_line_first_non_blank" => Some(motion(VimMotionKind::PreviousLineFirstNonBlank)),
        "next_line_first_non_blank" => Some(motion(VimMotionKind::NextLineFirstNonBlank)),
        "counted_line_first_non_blank" => Some(motion(VimMotionKind::CountedLineFirstNonBlank)),
        "buffer_percentage" => Some(motion(VimMotionKind::BufferPercentage)),
        "sentence_forward" => Some(motion(VimMotionKind::SentenceForward)),
        "sentence_backward" => Some(motion(VimMotionKind::SentenceBackward)),
        "paragraph_forward" => Some(motion(VimMotionKind::ParagraphForward)),
        "paragraph_backward" => Some(motion(VimMotionKind::ParagraphBackward)),
        "section_start_backward" => Some(motion(VimMotionKind::SectionStartBackward)),
        "section_start_forward" => Some(motion(VimMotionKind::SectionStartForward)),
        "section_end_backward" => Some(motion(VimMotionKind::SectionEndBackward)),
        "section_end_forward" => Some(motion(VimMotionKind::SectionEndForward)),
        "matching_pair" => Some(motion(VimMotionKind::MatchingPair)),
        "matching_pair_backward" => Some(motion(VimMotionKind::MatchingPairBackward)),
        "unmatched_paren_backward" => Some(Action::VimMotion(
            VimMotion::new(VimMotionKind::UnmatchedOpenBackward).targeting('('),
        )),
        "unmatched_brace_backward" => Some(Action::VimMotion(
            VimMotion::new(VimMotionKind::UnmatchedOpenBackward).targeting('{'),
        )),
        "unmatched_paren_forward" => Some(Action::VimMotion(
            VimMotion::new(VimMotionKind::UnmatchedCloseForward).targeting(')'),
        )),
        "unmatched_brace_forward" => Some(Action::VimMotion(
            VimMotion::new(VimMotionKind::UnmatchedCloseForward).targeting('}'),
        )),
        "method_start_backward" => Some(motion(VimMotionKind::MethodBackward)),
        "method_end_backward" => Some(Action::VimMotion(
            VimMotion::new(VimMotionKind::MethodBackward).targeting('M'),
        )),
        "method_start_forward" => Some(motion(VimMotionKind::MethodForward)),
        "method_end_forward" => Some(Action::VimMotion(
            VimMotion::new(VimMotionKind::MethodForward).targeting('M'),
        )),
        "preprocessor_backward" => Some(motion(VimMotionKind::PreprocessorBackward)),
        "preprocessor_forward" => Some(motion(VimMotionKind::PreprocessorForward)),
        "comment_backward" => Some(motion(VimMotionKind::CommentBackward)),
        "comment_forward" => Some(motion(VimMotionKind::CommentForward)),
        "window_top" => Some(motion(VimMotionKind::WindowTop)),
        "window_middle" => Some(motion(VimMotionKind::WindowMiddle)),
        "window_bottom" => Some(motion(VimMotionKind::WindowBottom)),
        "previous_diff_change" => Some(motion(VimMotionKind::DiffChangeBackward)),
        "next_diff_change" => Some(motion(VimMotionKind::DiffChangeForward)),
        "cursor_to_window_top" => Some(motion(VimMotionKind::CursorToWindowTop)),
        "cursor_to_window_top_first_non_blank" => {
            Some(motion(VimMotionKind::CursorToWindowTopFirstNonBlank))
        }
        "cursor_to_window_middle" => Some(motion(VimMotionKind::CursorToWindowMiddle)),
        "cursor_to_window_middle_first_non_blank" => {
            Some(motion(VimMotionKind::CursorToWindowMiddleFirstNonBlank))
        }
        "cursor_to_window_bottom" => Some(motion(VimMotionKind::CursorToWindowBottom)),
        "cursor_to_window_bottom_first_non_blank" => {
            Some(motion(VimMotionKind::CursorToWindowBottomFirstNonBlank))
        }
        "next_window_top" => Some(motion(VimMotionKind::NextWindowTop)),
        "previous_window_bottom" => Some(motion(VimMotionKind::PreviousWindowBottom)),
        "scroll_half_screen_left" => Some(motion(VimMotionKind::ScrollHalfScreenLeft)),
        "scroll_half_screen_right" => Some(motion(VimMotionKind::ScrollHalfScreenRight)),
        "cursor_to_window_left" => Some(motion(VimMotionKind::CursorToWindowLeft)),
        "cursor_to_window_right" => Some(motion(VimMotionKind::CursorToWindowRight)),
        "search_word_forward" => Some(motion(VimMotionKind::SearchWordForward)),
        "search_word_backward" => Some(motion(VimMotionKind::SearchWordBackward)),
        "search_partial_word_forward" => Some(motion(VimMotionKind::SearchPartialWordForward)),
        "search_partial_word_backward" => Some(motion(VimMotionKind::SearchPartialWordBackward)),
        "previous_mark_line" => Some(motion(VimMotionKind::PreviousMarkLine)),
        "previous_mark_exact" => Some(motion(VimMotionKind::PreviousMarkExact)),
        "next_mark_line" => Some(motion(VimMotionKind::NextMarkLine)),
        "next_mark_exact" => Some(motion(VimMotionKind::NextMarkExact)),
        "set_mark" => Some(Action::SetVimMark('\0')),
        "jump_mark_line" => Some(Action::JumpToVimMark {
            mark: '\0',
            linewise: true,
            record_jump: true,
        }),
        "jump_mark_exact" => Some(Action::JumpToVimMark {
            mark: '\0',
            linewise: false,
            record_jump: true,
        }),
        "jump_mark_line_without_history" => Some(Action::JumpToVimMark {
            mark: '\0',
            linewise: true,
            record_jump: false,
        }),
        "jump_mark_exact_without_history" => Some(Action::JumpToVimMark {
            mark: '\0',
            linewise: false,
            record_jump: false,
        }),
        "lsp_hover" => Some(Action::ToggleLspHover),
        "go_to_definition" => Some(Action::GoToSemanticTarget(
            SemanticNavigationKind::Definition,
        )),
        "go_to_implementation" => Some(Action::GoToSemanticTarget(
            SemanticNavigationKind::Implementation,
        )),
        "go_to_type_definition" => Some(Action::GoToSemanticTarget(
            SemanticNavigationKind::TypeDefinition,
        )),
        "go_to_declaration" => Some(Action::GoToSemanticTarget(
            SemanticNavigationKind::Declaration,
        )),
        "semantic_back" => Some(Action::JumpListBack(1)),
        "semantic_forward" => Some(Action::JumpListForward(1)),
        "refresh" => Some(Action::Refresh),
        "toggle_message" => Some(Action::ToggleMessage),
        "toggle_details" => Some(Action::ToggleDetails),
        "toggle_tree" => Some(Action::ToggleTree),
        "activate" => Some(Action::Activate),
        "file_search" => Some(Action::OpenFileSearch),
        "content_search" => Some(Action::OpenContentSearch),
        "search_forward" => Some(Action::StartSearch(SearchDirection::Forward)),
        "search_backward" => Some(Action::StartSearch(SearchDirection::Backward)),
        "next_match" => Some(motion(VimMotionKind::SearchNext)),
        "previous_match" => Some(motion(VimMotionKind::SearchPrevious)),
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
    use crate::app::{Action, SemanticNavigationKind, VimMotion, VimMotionKind};

    use super::KeyStroke;
    use super::config::parse_stroke;

    fn motion(kind: VimMotionKind) -> Option<Action> {
        Some(Action::VimMotion(VimMotion::new(kind)))
    }

    #[test]
    fn maps_navigation_sequences_graph_and_global_search() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), false),
            Some(Action::CloseOverlay)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), false),
            Some(Action::DismissSearchOrClose)
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
            motion(VimMotionKind::Down)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
                false
            ),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE), false),
            Some(Action::FocusLeft)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE), false),
            motion(VimMotionKind::Down)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE), false),
            motion(VimMotionKind::Left)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE), false),
            motion(VimMotionKind::Right)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE), false),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE), false),
            motion(VimMotionKind::ScrollColumnRight)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE), false),
            motion(VimMotionKind::RightWrap)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE), false),
            motion(VimMotionKind::LeftWrap)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::NONE),
                false
            ),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE), false),
            Some(Action::ShowGraph)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::NONE),
                false
            ),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE), false),
            Some(Action::ShowCode)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::NONE),
                false
            ),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE), false),
            Some(Action::OpenFileSearch)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE), false),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE), false),
            motion(VimMotionKind::BufferTop)
        );
        for (suffix, kind) in [
            ('d', SemanticNavigationKind::Definition),
            ('i', SemanticNavigationKind::Implementation),
            ('y', SemanticNavigationKind::TypeDefinition),
            ('D', SemanticNavigationKind::Declaration),
        ] {
            assert_eq!(
                mapper.map(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE), false),
                None
            );
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char(suffix), KeyModifiers::NONE),
                    false
                ),
                Some(Action::GoToSemanticTarget(kind))
            );
        }
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), false),
            motion(VimMotionKind::Right)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT), false),
            motion(VimMotionKind::WordForward)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL), false),
            motion(VimMotionKind::BigWordForward)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL), false),
            motion(VimMotionKind::BigWordBackward)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE), false),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::End, KeyModifiers::NONE), false),
            motion(VimMotionKind::ScreenLastNonBlank)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL), false),
            motion(VimMotionKind::BufferBottomEnd)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SHIFT),
                false
            ),
            Some(Action::ToggleLspHover)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
                false
            ),
            Some(Action::JumpListBack(1))
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL),
                false
            ),
            Some(Action::JumpListForward(1))
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), false),
            Some(Action::JumpListForward(1))
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
    fn vim_counts_character_arguments_and_repeats_are_preserved() {
        let mut mapper = KeyMapper::new();
        for digit in ['1', '2'] {
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char(digit), KeyModifiers::NONE),
                    false
                ),
                None
            );
        }
        let Some(Action::VimMotion(word)) =
            mapper.map(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE), false)
        else {
            panic!("expected counted word motion");
        };
        assert_eq!(word.kind(), VimMotionKind::WordForward);
        assert_eq!(word.count(), 12);
        assert!(word.has_explicit_count());

        let Some(Action::VimMotion(line_start)) =
            mapper.map(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE), false)
        else {
            panic!("expected zero to remain a line-start motion without a count");
        };
        assert_eq!(line_start.kind(), VimMotionKind::LineStart);

        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('5'), KeyModifiers::NONE), false),
            None
        );
        let Some(Action::VimMotion(percentage)) =
            mapper.map(KeyEvent::new(KeyCode::Char('%'), KeyModifiers::NONE), false)
        else {
            panic!("expected percentage motion");
        };
        assert_eq!(percentage.kind(), VimMotionKind::BufferPercentage);
        assert_eq!(percentage.count(), 5);

        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE), false),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('%'), KeyModifiers::NONE), false),
            motion(VimMotionKind::MatchingPairBackward)
        );

        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE), false),
            None
        );
        let Some(Action::VimMotion(find)) = mapper.map(
            KeyEvent::new(KeyCode::Char('界'), KeyModifiers::NONE),
            false,
        ) else {
            panic!("expected completed character search");
        };
        assert_eq!(find.kind(), VimMotionKind::FindForward);
        assert_eq!(find.target(), Some('界'));

        let Some(Action::VimMotion(repeated)) =
            mapper.map(KeyEvent::new(KeyCode::Char(';'), KeyModifiers::NONE), false)
        else {
            panic!("expected repeated character search");
        };
        assert_eq!(repeated.kind(), VimMotionKind::FindForward);
        assert_eq!(repeated.target(), Some('界'));

        let Some(Action::VimMotion(reversed)) =
            mapper.map(KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE), false)
        else {
            panic!("expected reversed character search");
        };
        assert_eq!(reversed.kind(), VimMotionKind::FindBackward);
        assert_eq!(reversed.target(), Some('界'));
    }

    #[test]
    fn vim_marks_jump_counts_and_till_repeats_keep_command_state() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE), false),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE), false),
            Some(Action::SetVimMark('a'))
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('\''), KeyModifiers::NONE),
                false
            ),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE), false),
            Some(Action::JumpToVimMark {
                mark: 'a',
                linewise: true,
                record_jump: true,
            })
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE), false),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('`'), KeyModifiers::NONE), false),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE), false),
            Some(Action::JumpToVimMark {
                mark: 'a',
                linewise: false,
                record_jump: false,
            })
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE), false),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('\''), KeyModifiers::NONE),
                false
            ),
            motion(VimMotionKind::NextMarkLine)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE), false),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
                false
            ),
            Some(Action::JumpListBack(3))
        );

        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE), false),
            None
        );
        assert!(matches!(
            mapper.map(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE), false),
            Some(Action::VimMotion(motion))
                if motion.kind() == VimMotionKind::TillForward && motion.count() == 1
        ));
        assert!(matches!(
            mapper.map(KeyEvent::new(KeyCode::Char(';'), KeyModifiers::NONE), false),
            Some(Action::VimMotion(motion))
                if motion.kind() == VimMotionKind::TillForward && motion.count() == 1 && motion.is_repeated()
        ));
    }

    #[test]
    fn explicit_config_replaces_selected_defaults() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create temp directory: {error}"));
        let path = directory.path().join("keymap.conf");
        fs::write(
            &path,
            "[bindings]\nshow_graph = x\nfile_search = alt-p\ntoggle_tree = alt-t\nsemantic_forward = tab\n",
        )
        .unwrap_or_else(|error| panic!("could not write keymap: {error}"));
        let mut mapper = KeyMapper::load(Some(&path)).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE), false),
            Some(Action::ShowGraph)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE), false),
            motion(VimMotionKind::RightWrap)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE), false),
            None
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT), false),
            Some(Action::OpenFileSearch)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::ALT), false),
            Some(Action::ToggleTree)
        );
        assert_eq!(
            mapper.map(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), false),
            Some(Action::JumpListForward(1))
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL),
                false
            ),
            None,
            "an explicit semantic_forward binding replaces its defaults"
        );
    }

    #[test]
    fn explicit_close_preserves_the_meaning_of_removed_or_reassigned_escape() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("keymap.conf");
        for (source, expected) in [
            ("close = x", None),
            ("close = q, esc", Some(Action::CloseOverlay)),
            ("close = x\nrefresh = esc", Some(Action::Refresh)),
            ("refresh = esc\nclose = x", Some(Action::Refresh)),
            ("refresh = x", Some(Action::DismissSearchOrClose)),
        ] {
            fs::write(&path, source).unwrap_or_else(|error| panic!("{error}"));
            let mut mapper = KeyMapper::load(Some(&path)).unwrap_or_else(|error| panic!("{error}"));
            let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
            assert_eq!(mapper.map(esc, false), expected, "{source}");
            assert_eq!(mapper.map(esc, true), Some(Action::CancelSearch));
        }
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
        fs::write(&path, "show_graph = 3\n")
            .unwrap_or_else(|error| panic!("could not write keymap: {error}"));
        assert!(KeyMapper::load(Some(&path)).is_err());
    }

    #[test]
    fn config_parser_accepts_page_and_combined_modifier_keys() {
        assert_eq!(
            parse_stroke("comma"),
            Ok(KeyStroke::new(KeyCode::Char(','), KeyModifiers::NONE))
        );
        assert_eq!(
            parse_stroke("pageup"),
            Ok(KeyStroke::new(KeyCode::PageUp, KeyModifiers::NONE))
        );
        assert_eq!(
            parse_stroke("page-down"),
            Ok(KeyStroke::new(KeyCode::PageDown, KeyModifiers::NONE))
        );
        assert_eq!(
            parse_stroke("shift-left"),
            Ok(KeyStroke::new(KeyCode::Left, KeyModifiers::SHIFT))
        );
        assert_eq!(
            parse_stroke("ctrl-shift-x"),
            Ok(KeyStroke::new(KeyCode::Char('X'), KeyModifiers::CONTROL))
        );
        assert_eq!(
            parse_stroke("shift-alt-x"),
            Ok(KeyStroke::new(KeyCode::Char('X'), KeyModifiers::ALT))
        );
    }
}
