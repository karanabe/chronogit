//! Translation from terminal key events to semantic application actions.
//!
//! Built-in Vim-oriented bindings can be selectively replaced by an optional
//! keymap file. Multi-key sequences are resolved without ambiguous prefixes and
//! expire after 750 milliseconds.

mod config;

use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vim_navigation::{MotionResolution, MotionState};

use crate::app::{
    Action, JumpHistory, MarkJumpTarget, SearchDirection, SemanticNavigationKind, VimMotion,
    VimMotionKind,
};

pub use config::KeyMapError;

const SEQUENCE_TIMEOUT: Duration = Duration::from_millis(750);

/// Input context used to interpret printable terminal keys.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum KeyInputContext {
    /// Interpret input as normal-mode commands and configured bindings.
    #[default]
    Normal,
    /// Interpret printable input as repository or document search text.
    SearchInput,
}

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
    command: BindingCommand,
}

impl Binding {
    pub(super) fn new(sequence: Vec<KeyStroke>, command: impl Into<BindingCommand>) -> Self {
        Self {
            sequence,
            command: command.into(),
        }
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
    motion_state: MotionState,
    awaiting_mark: Option<MarkCommand>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MarkCommand {
    Set,
    Jump {
        target: MarkJumpTarget,
        history: JumpHistory,
    },
}

impl KeyMapper {
    /// Creates a mapper using all built-in bindings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: config::default_bindings(),
            pending: Vec::new(),
            pending_since: None,
            motion_state: MotionState::new(),
            awaiting_mark: None,
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
            motion_state: MotionState::new(),
            awaiting_mark: None,
        })
    }

    /// Consumes one key event and returns a completed semantic action.
    ///
    /// `None` means the key is either unbound or is a valid prefix awaiting the
    /// next stroke. [`KeyInputContext::SearchInput`] switches printable keys to
    /// query-edit actions while retaining the reserved focus, confirmation,
    /// cancellation, and `Ctrl-C` controls.
    pub fn map(&mut self, key: KeyEvent, context: KeyInputContext) -> Option<Action> {
        if matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.clear_command();
            return Some(Action::Quit);
        }
        if context == KeyInputContext::SearchInput {
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
            let _ = self.motion_state.take_count();
            return match (key.code, key.modifiers) {
                (KeyCode::Esc, _) => None,
                (KeyCode::Char(mark), modifiers)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    Some(match command {
                        MarkCommand::Set => Action::SetVimMark(mark),
                        MarkCommand::Jump { target, history } => Action::JumpToVimMark {
                            mark,
                            target,
                            history,
                        },
                    })
                }
                _ => None,
            };
        }

        if self.motion_state.is_awaiting_target() {
            self.pending_since = None;
            return match (key.code, key.modifiers) {
                (KeyCode::Esc, _) => {
                    self.motion_state.reset_pending();
                    None
                }
                (KeyCode::Char(target), modifiers)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.motion_state
                        .accept_target(target)
                        .map(Action::VimMotion)
                }
                _ => {
                    self.motion_state.reset_pending();
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
            && (digit != '0' || self.motion_state.has_count())
        {
            let _ = self.motion_state.push_count_digit(digit);
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

    fn resolve_pending(&mut self) -> Option<BindingCommand> {
        let action = self
            .bindings
            .iter()
            .find(|binding| binding.sequence == self.pending)
            .map(|binding| binding.command)?;
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

    fn finish_action(&mut self, command: BindingCommand) -> Option<Action> {
        let action = match command {
            BindingCommand::AwaitMark(command) => {
                let _ = self.motion_state.take_count();
                self.awaiting_mark = Some(command);
                return None;
            }
            BindingCommand::Action(action) => action,
        };
        if matches!(action, Action::JumpListBack(_) | Action::JumpListForward(_)) {
            let count = self.motion_state.take_count();
            return Some(match action {
                Action::JumpListBack(_) => Action::JumpListBack(count.unwrap_or(1).max(1)),
                Action::JumpListForward(_) => Action::JumpListForward(count.unwrap_or(1).max(1)),
                _ => unreachable!(),
            });
        }
        let Action::VimMotion(motion) = action else {
            let _ = self.motion_state.take_count();
            return Some(action);
        };
        match self.motion_state.finish(motion) {
            MotionResolution::Ready(motion) => Some(Action::VimMotion(motion)),
            MotionResolution::AwaitingTarget => {
                self.pending_since = Some(Instant::now());
                None
            }
            MotionResolution::Unavailable => None,
        }
    }

    fn clear_command(&mut self) {
        self.pending.clear();
        self.pending_since = None;
        self.motion_state.reset_pending();
        self.awaiting_mark = None;
    }
}

impl Default for KeyMapper {
    fn default() -> Self {
        Self::new()
    }
}

/// A binding may still need input; only completed commands become app actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BindingCommand {
    Action(Action),
    AwaitMark(MarkCommand),
}

impl From<Action> for BindingCommand {
    fn from(action: Action) -> Self {
        Self::Action(action)
    }
}

impl From<MarkCommand> for BindingCommand {
    fn from(command: MarkCommand) -> Self {
        Self::AwaitMark(command)
    }
}

pub(super) fn command_for_name(name: &str) -> Option<BindingCommand> {
    let mark = match name {
        "set_mark" => Some(MarkCommand::Set),
        "jump_mark_line" => Some(MarkCommand::Jump {
            target: MarkJumpTarget::Line,
            history: JumpHistory::Record,
        }),
        "jump_mark_exact" => Some(MarkCommand::Jump {
            target: MarkJumpTarget::Exact,
            history: JumpHistory::Record,
        }),
        "jump_mark_line_without_history" => Some(MarkCommand::Jump {
            target: MarkJumpTarget::Line,
            history: JumpHistory::Preserve,
        }),
        "jump_mark_exact_without_history" => Some(MarkCommand::Jump {
            target: MarkJumpTarget::Exact,
            history: JumpHistory::Preserve,
        }),
        _ => None,
    };
    mark.map(BindingCommand::AwaitMark)
        .or_else(|| action_for_name(name).map(BindingCommand::Action))
}

fn action_for_name(name: &str) -> Option<Action> {
    let motion = |kind| Action::VimMotion(VimMotion::new(kind));
    match name {
        "quit" => Some(Action::Quit),
        "show_changes" => Some(Action::ShowChanges),
        "show_history" => Some(Action::ShowHistory),
        "show_graph" => Some(Action::ShowGraph),
        "show_code" => Some(Action::ShowCode),
        "switch_branch" => Some(Action::OpenBranches),
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
        "lsp_hover" => Some(Action::ToggleLspHover),
        "symbol_context" => Some(Action::OpenSymbolContext),
        "open_full_file" => Some(Action::OpenFullFile),
        "toggle_full_file_mode" => Some(Action::ToggleFullFileMode),
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

    use super::{KeyInputContext, KeyMapper};
    use crate::app::{
        Action, JumpHistory, MarkJumpTarget, SemanticNavigationKind, VimMotion, VimMotionKind,
    };

    use super::KeyStroke;
    use super::config::parse_stroke;

    fn motion(kind: VimMotionKind) -> Option<Action> {
        Some(Action::VimMotion(VimMotion::new(kind)))
    }

    #[test]
    fn maps_navigation_sequences_graph_and_global_search() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            Some(Action::CloseOverlay)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            Some(Action::DismissSearchOrClose)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT),
                KeyInputContext::Normal
            ),
            Some(Action::Quit)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
                KeyInputContext::Normal
            ),
            Some(Action::FocusRight)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            Some(Action::FocusLeft)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::Down)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::Left)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::Right)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::ScrollColumnRight)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::LeftWrap)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            Some(Action::ShowGraph)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            Some(Action::ShowCode)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            Some(Action::OpenFileSearch)
        );
        for (suffix, action) in [
            ('s', Action::OpenSymbolContext),
            ('v', Action::OpenFullFile),
            ('d', Action::ToggleFullFileMode),
        ] {
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
                    KeyInputContext::Normal
                ),
                None
            );
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char(suffix), KeyModifiers::NONE),
                    KeyInputContext::Normal
                ),
                Some(action)
            );
        }
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::BufferTop)
        );
        for (suffix, kind) in [
            ('d', SemanticNavigationKind::Definition),
            ('i', SemanticNavigationKind::Implementation),
            ('y', SemanticNavigationKind::TypeDefinition),
            ('D', SemanticNavigationKind::Declaration),
        ] {
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
                    KeyInputContext::Normal
                ),
                None
            );
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char(suffix), KeyModifiers::NONE),
                    KeyInputContext::Normal
                ),
                Some(Action::GoToSemanticTarget(kind))
            );
        }
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::Right)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::WordForward)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::BigWordForward)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::BigWordBackward)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::ScreenLastNonBlank)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::BufferBottomEnd)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SHIFT),
                KeyInputContext::Normal
            ),
            Some(Action::ToggleLspHover)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
                KeyInputContext::Normal
            ),
            Some(Action::JumpListBack(1))
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL),
                KeyInputContext::Normal
            ),
            Some(Action::JumpListForward(1))
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            Some(Action::JumpListForward(1))
        );
    }

    #[test]
    fn standalone_control_keys_and_ctrl_w_sequences_focus_panes() {
        for (key, expected) in [
            ('h', Action::FocusLeft),
            ('k', Action::FocusLeft),
            ('j', Action::FocusRight),
            ('l', Action::FocusRight),
        ] {
            let mut mapper = KeyMapper::new();
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char(key), KeyModifiers::CONTROL),
                    KeyInputContext::Normal
                ),
                Some(expected),
                "standalone Ctrl-{key}"
            );
        }

        for (code, modifiers, expected) in [
            (KeyCode::Char('h'), KeyModifiers::NONE, Action::FocusLeft),
            (KeyCode::Char('k'), KeyModifiers::NONE, Action::FocusLeft),
            (KeyCode::Char('j'), KeyModifiers::NONE, Action::FocusRight),
            (KeyCode::Char('l'), KeyModifiers::NONE, Action::FocusRight),
            (KeyCode::Char('h'), KeyModifiers::CONTROL, Action::FocusLeft),
            (KeyCode::Char('k'), KeyModifiers::CONTROL, Action::FocusLeft),
            (
                KeyCode::Char('j'),
                KeyModifiers::CONTROL,
                Action::FocusRight,
            ),
            (
                KeyCode::Char('l'),
                KeyModifiers::CONTROL,
                Action::FocusRight,
            ),
            (KeyCode::Backspace, KeyModifiers::NONE, Action::FocusLeft),
            (KeyCode::Char('W'), KeyModifiers::SHIFT, Action::FocusLeft),
            (KeyCode::Char('w'), KeyModifiers::NONE, Action::FocusRight),
            (
                KeyCode::Char('w'),
                KeyModifiers::CONTROL,
                Action::FocusRight,
            ),
            (KeyCode::Left, KeyModifiers::NONE, Action::FocusLeft),
            (KeyCode::Up, KeyModifiers::NONE, Action::FocusLeft),
            (KeyCode::Right, KeyModifiers::NONE, Action::FocusRight),
            (KeyCode::Down, KeyModifiers::NONE, Action::FocusRight),
        ] {
            let mut mapper = KeyMapper::new();
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
                    KeyInputContext::Normal
                ),
                None
            );
            assert_eq!(
                mapper.map(KeyEvent::new(code, modifiers), KeyInputContext::Normal),
                Some(expected),
                "Ctrl-w {modifiers:?}-{code:?}"
            );
        }

        for (code, expected) in [
            (KeyCode::Char('h'), VimMotionKind::Left),
            (KeyCode::Char('j'), VimMotionKind::Down),
            (KeyCode::Char('k'), VimMotionKind::Up),
            (KeyCode::Char('l'), VimMotionKind::Right),
            (KeyCode::Left, VimMotionKind::Left),
            (KeyCode::Down, VimMotionKind::Down),
            (KeyCode::Up, VimMotionKind::Up),
            (KeyCode::Right, VimMotionKind::Right),
        ] {
            let mut mapper = KeyMapper::new();
            assert_eq!(
                mapper.map(
                    KeyEvent::new(code, KeyModifiers::NONE),
                    KeyInputContext::Normal
                ),
                motion(expected),
                "unmodified {code:?}"
            );
        }
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::LeftWrap)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::Down)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::Up)
        );
    }

    #[test]
    fn default_space_leader_resolves_every_application_action_without_a_space_motion() {
        for (suffix, expected) in [
            ('1', Action::ShowChanges),
            ('2', Action::ShowHistory),
            ('3', Action::ShowGraph),
            ('4', Action::ShowCode),
            ('f', Action::OpenFileSearch),
            ('g', Action::OpenContentSearch),
            ('m', Action::ToggleMessage),
            ('b', Action::OpenBranches),
            ('B', Action::ToggleDetails),
            ('t', Action::ToggleTree),
        ] {
            let mut mapper = KeyMapper::new();
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
                    KeyInputContext::Normal
                ),
                None,
                "Space must remain a prefix before {suffix:?}"
            );
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char(suffix), KeyModifiers::NONE),
                    KeyInputContext::Normal
                ),
                Some(expected)
            );
        }

        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::Right),
            "an unrelated suffix is retried as a normal key, so l remains the right-motion alternative"
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::Right)
        );
    }

    #[test]
    fn search_input_accepts_q_and_reserves_control_keys() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
                KeyInputContext::SearchInput
            ),
            Some(Action::InsertSearch('q'))
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT),
                KeyInputContext::SearchInput
            ),
            Some(Action::InsertSearch('Q'))
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
                KeyInputContext::SearchInput
            ),
            Some(Action::InsertSearch(' ')),
            "the application leader is disabled inside a search prompt"
        );
        for character in ['j', 'j'] {
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
                    KeyInputContext::SearchInput
                ),
                Some(Action::InsertSearch(character)),
                "EditableBuffer's Insert escape sequence must not leak into search input"
            );
        }
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
                KeyInputContext::SearchInput
            ),
            Some(Action::DeleteSearch)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                KeyInputContext::SearchInput
            ),
            Some(Action::ConfirmSearch)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
                KeyInputContext::SearchInput
            ),
            Some(Action::FocusRight)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
                KeyInputContext::SearchInput
            ),
            Some(Action::FocusLeft)
        );
        for character in ['h', 'l'] {
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL),
                    KeyInputContext::SearchInput
                ),
                None,
                "normal-mode Ctrl-{character} must not leak into search input"
            );
        }
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                KeyInputContext::SearchInput
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
                    KeyInputContext::Normal
                ),
                None
            );
        }
        let Some(Action::VimMotion(word)) = mapper.map(
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE),
            KeyInputContext::Normal,
        ) else {
            panic!("expected counted word motion");
        };
        assert_eq!(word.kind(), VimMotionKind::WordForward);
        assert_eq!(word.count(), 12);
        assert!(word.has_explicit_count());

        let Some(Action::VimMotion(line_start)) = mapper.map(
            KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE),
            KeyInputContext::Normal,
        ) else {
            panic!("expected zero to remain a line-start motion without a count");
        };
        assert_eq!(line_start.kind(), VimMotionKind::LineStart);

        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('5'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        let Some(Action::VimMotion(percentage)) = mapper.map(
            KeyEvent::new(KeyCode::Char('%'), KeyModifiers::NONE),
            KeyInputContext::Normal,
        ) else {
            panic!("expected percentage motion");
        };
        assert_eq!(percentage.kind(), VimMotionKind::BufferPercentage);
        assert_eq!(percentage.count(), 5);

        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('%'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::MatchingPairBackward)
        );

        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        let Some(Action::VimMotion(find)) = mapper.map(
            KeyEvent::new(KeyCode::Char('界'), KeyModifiers::NONE),
            KeyInputContext::Normal,
        ) else {
            panic!("expected completed character search");
        };
        assert_eq!(find.kind(), VimMotionKind::FindForward);
        assert_eq!(find.target(), Some('界'));

        let Some(Action::VimMotion(repeated)) = mapper.map(
            KeyEvent::new(KeyCode::Char(';'), KeyModifiers::NONE),
            KeyInputContext::Normal,
        ) else {
            panic!("expected repeated character search");
        };
        assert_eq!(repeated.kind(), VimMotionKind::FindForward);
        assert_eq!(repeated.target(), Some('界'));

        let Some(Action::VimMotion(reversed)) = mapper.map(
            KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE),
            KeyInputContext::Normal,
        ) else {
            panic!("expected reversed character search");
        };
        assert_eq!(reversed.kind(), VimMotionKind::FindBackward);
        assert_eq!(reversed.target(), Some('界'));
    }

    #[test]
    fn character_argument_waits_consume_control_focus_keys_without_focusing() {
        for prefix in ['f', 't', 'm', '\'', '`'] {
            let mut mapper = KeyMapper::new();
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char(prefix), KeyModifiers::NONE),
                    KeyInputContext::Normal
                ),
                None
            );
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL),
                    KeyInputContext::Normal
                ),
                None,
                "Ctrl-h must only cancel the {prefix:?} character wait"
            );
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL),
                    KeyInputContext::Normal
                ),
                Some(Action::FocusLeft),
                "the next Ctrl-h must be a new normal-mode command"
            );
        }
    }

    #[test]
    fn vim_marks_jump_counts_and_till_repeats_keep_command_state() {
        let mut mapper = KeyMapper::new();
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            Some(Action::SetVimMark('a'))
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('\''), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            Some(Action::JumpToVimMark {
                mark: 'a',
                target: MarkJumpTarget::Line,
                history: JumpHistory::Record,
            })
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('`'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            Some(Action::JumpToVimMark {
                mark: 'a',
                target: MarkJumpTarget::Exact,
                history: JumpHistory::Preserve,
            })
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('\''), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::NextMarkLine)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
                KeyInputContext::Normal
            ),
            Some(Action::JumpListBack(3))
        );

        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert!(matches!(
            mapper.map(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE), KeyInputContext::Normal),
            Some(Action::VimMotion(motion))
                if motion.kind() == VimMotionKind::TillForward && motion.count() == 1
        ));
        assert!(matches!(
            mapper.map(KeyEvent::new(KeyCode::Char(';'), KeyModifiers::NONE), KeyInputContext::Normal),
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
            "[bindings]\nshow_graph = x\nfile_search = alt-p\ntoggle_tree = alt-t\nsemantic_forward = tab\ncursor_right_wrap = \\\n",
        )
        .unwrap_or_else(|error| panic!("could not write keymap: {error}"));
        let mut mapper = KeyMapper::load(Some(&path)).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            Some(Action::ShowGraph)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::RightWrap)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            Some(Action::ShowHistory),
            "replacing one leader action leaves the other default Space sequences intact"
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            None
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT),
                KeyInputContext::Normal
            ),
            Some(Action::OpenFileSearch)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('t'), KeyModifiers::ALT),
                KeyInputContext::Normal
            ),
            Some(Action::ToggleTree)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            Some(Action::JumpListForward(1))
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL),
                KeyInputContext::Normal
            ),
            None,
            "an explicit semantic_forward binding replaces its defaults"
        );
    }

    #[test]
    fn configured_mark_commands_wait_for_their_argument() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
        let path = directory.path().join("keymap.conf");
        for (name, expected) in [
            ("set_mark", Action::SetVimMark('a')),
            (
                "jump_mark_line",
                Action::JumpToVimMark {
                    mark: 'a',
                    target: MarkJumpTarget::Line,
                    history: JumpHistory::Record,
                },
            ),
            (
                "jump_mark_exact",
                Action::JumpToVimMark {
                    mark: 'a',
                    target: MarkJumpTarget::Exact,
                    history: JumpHistory::Record,
                },
            ),
            (
                "jump_mark_line_without_history",
                Action::JumpToVimMark {
                    mark: 'a',
                    target: MarkJumpTarget::Line,
                    history: JumpHistory::Preserve,
                },
            ),
            (
                "jump_mark_exact_without_history",
                Action::JumpToVimMark {
                    mark: 'a',
                    target: MarkJumpTarget::Exact,
                    history: JumpHistory::Preserve,
                },
            ),
        ] {
            std::fs::write(&path, format!("{name} = alt-m\n"))
                .unwrap_or_else(|error| panic!("write keymap: {error}"));
            let mut mapper =
                KeyMapper::load(Some(&path)).unwrap_or_else(|error| panic!("load keymap: {error}"));
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char('m'), KeyModifiers::ALT),
                    KeyInputContext::Normal
                ),
                None,
                "{name}"
            );
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
                    KeyInputContext::Normal
                ),
                Some(expected),
                "{name}"
            );
        }
    }

    #[test]
    fn pane_focus_actions_are_independently_replaceable() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create temp directory: {error}"));
        let path = directory.path().join("keymap.conf");
        for (source, expected) in [
            (
                "focus_previous = ctrl-k, alt-h\n",
                [
                    ('h', KeyModifiers::CONTROL, None),
                    ('k', KeyModifiers::CONTROL, Some(Action::FocusLeft)),
                    ('h', KeyModifiers::ALT, Some(Action::FocusLeft)),
                    ('j', KeyModifiers::CONTROL, Some(Action::FocusRight)),
                    ('l', KeyModifiers::CONTROL, Some(Action::FocusRight)),
                ],
            ),
            (
                "focus_next = ctrl-l, alt-j\n",
                [
                    ('h', KeyModifiers::CONTROL, Some(Action::FocusLeft)),
                    ('k', KeyModifiers::CONTROL, Some(Action::FocusLeft)),
                    ('j', KeyModifiers::CONTROL, None),
                    ('l', KeyModifiers::CONTROL, Some(Action::FocusRight)),
                    ('j', KeyModifiers::ALT, Some(Action::FocusRight)),
                ],
            ),
            (
                "focus_previous = alt-h\nfocus_next = alt-l\n",
                [
                    ('h', KeyModifiers::CONTROL, None),
                    ('k', KeyModifiers::CONTROL, None),
                    ('j', KeyModifiers::CONTROL, None),
                    ('l', KeyModifiers::CONTROL, None),
                    ('h', KeyModifiers::ALT, Some(Action::FocusLeft)),
                ],
            ),
        ] {
            fs::write(&path, source).unwrap_or_else(|error| panic!("{error}"));
            for (key, modifiers, action) in expected {
                let mut mapper =
                    KeyMapper::load(Some(&path)).unwrap_or_else(|error| panic!("{error}"));
                assert_eq!(
                    mapper.map(
                        KeyEvent::new(KeyCode::Char(key), modifiers),
                        KeyInputContext::Normal
                    ),
                    action,
                    "{source:?}: {modifiers:?}-{key}"
                );
            }
            if source.contains("focus_next = alt-l") {
                let mut mapper =
                    KeyMapper::load(Some(&path)).unwrap_or_else(|error| panic!("{error}"));
                assert_eq!(
                    mapper.map(
                        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::ALT),
                        KeyInputContext::Normal
                    ),
                    Some(Action::FocusRight)
                );
            }
        }

        fs::write(
            &path,
            "focus_previous = ctrl-k, ctrl-w h, ctrl-w k\n\
             focus_next = ctrl-l, ctrl-w j, ctrl-w l\n\
             cursor_left_wrap = backspace, ctrl-h\n\
             move_down = j, down, ctrl-j, ctrl-n\n",
        )
        .unwrap_or_else(|error| panic!("{error}"));
        for (key, expected) in [
            ('h', motion(VimMotionKind::LeftWrap)),
            ('j', motion(VimMotionKind::Down)),
            ('k', Some(Action::FocusLeft)),
            ('l', Some(Action::FocusRight)),
        ] {
            let mut mapper = KeyMapper::load(Some(&path)).unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(
                mapper.map(
                    KeyEvent::new(KeyCode::Char(key), KeyModifiers::CONTROL),
                    KeyInputContext::Normal
                ),
                expected,
                "restored Ctrl-{key} mapping"
            );
        }

        for source in ["focus_previous = ctrl-j\n", "focus_previous = ctrl-w\n"] {
            fs::write(&path, source).unwrap_or_else(|error| panic!("{error}"));
            assert!(
                KeyMapper::load(Some(&path)).is_err(),
                "conflicting focus binding must be rejected: {source:?}"
            );
        }
    }

    #[test]
    fn replacing_every_space_action_can_restore_the_library_space_motion() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("keymap.conf");
        fs::write(
            &path,
            "[bindings]\n\
             show_changes = alt-1\n\
             show_history = alt-2\n\
             show_graph = alt-3\n\
             show_code = alt-4\n\
             switch_branch = alt-B\n\
             file_search = alt-f\n\
             content_search = alt-g\n\
             toggle_message = alt-m\n\
             toggle_details = alt-b\n\
             toggle_tree = alt-t\n\
             symbol_context = alt-s\n\
             open_full_file = alt-v\n\
             toggle_full_file_mode = alt-d\n\
             cursor_right_wrap = space\n",
        )
        .unwrap_or_else(|error| panic!("{error}"));

        let mut mapper = KeyMapper::load(Some(&path)).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
                KeyInputContext::Normal
            ),
            motion(VimMotionKind::RightWrap)
        );
        assert_eq!(
            mapper.map(
                KeyEvent::new(KeyCode::Char('1'), KeyModifiers::ALT),
                KeyInputContext::Normal
            ),
            Some(Action::ShowChanges)
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
            assert_eq!(
                mapper.map(esc, KeyInputContext::Normal),
                expected,
                "{source}"
            );
            assert_eq!(
                mapper.map(esc, KeyInputContext::SearchInput),
                Some(Action::CancelSearch)
            );
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
        fs::write(&path, "cursor_right_wrap = space\n")
            .unwrap_or_else(|error| panic!("could not write keymap: {error}"));
        assert!(
            KeyMapper::load(Some(&path)).is_err(),
            "a standalone Space action conflicts with the remaining default leader sequences"
        );
        fs::write(&path, "show_graph = f0\n")
            .unwrap_or_else(|error| panic!("could not write keymap: {error}"));
        assert!(KeyMapper::load(Some(&path)).is_err());
        fs::write(&path, "show_graph = 3\n")
            .unwrap_or_else(|error| panic!("could not write keymap: {error}"));
        assert!(KeyMapper::load(Some(&path)).is_err());
    }

    #[test]
    fn built_in_space_leader_is_prefix_unambiguous() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("keymap.conf");
        fs::write(&path, "[bindings]\n").unwrap_or_else(|error| panic!("{error}"));
        KeyMapper::load(Some(&path))
            .unwrap_or_else(|error| panic!("default bindings must validate: {error}"));
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
