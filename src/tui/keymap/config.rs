//! Loading, parsing, and validating the optional keymap configuration file.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyModifiers};

use super::{Binding, KeyStroke, action_for_name};
use crate::app::{Action, SearchDirection, SemanticNavigationKind, VimMotion, VimMotionKind};

/// A path-qualified keymap read or validation failure.
#[derive(Debug)]
pub struct KeyMapError {
    path: PathBuf,
    detail: String,
}

impl Display for KeyMapError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "could not load keymap {}: {}",
            self.path.display(),
            self.detail
        )
    }
}

impl Error for KeyMapError {}

pub(super) fn load_bindings(path: Option<&Path>) -> Result<Vec<Binding>, KeyMapError> {
    let explicit = path.is_some();
    let path = path.map(Path::to_path_buf).or_else(default_path);
    let Some(path) = path else {
        return Ok(default_bindings());
    };
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if !explicit && error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(default_bindings());
        }
        Err(error) => return Err(keymap_error(&path, error.to_string())),
    };
    parse_bindings(&path, &source)
}

fn default_path() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(value).join("chronogit/keymap.conf"));
    }
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(".config/chronogit/keymap.conf"))
}

fn parse_bindings(path: &Path, source: &str) -> Result<Vec<Binding>, KeyMapError> {
    let mut bindings = default_bindings();
    for (index, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line == "[bindings]" {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            return Err(keymap_error(
                path,
                format!("line {} must use action = key", index + 1),
            ));
        };
        let name = name.trim();
        let Some(action) = action_for_name(name) else {
            return Err(keymap_error(
                path,
                format!("line {} has unknown action {name:?}", index + 1),
            ));
        };
        let value = value.trim().trim_matches('"');
        if value.is_empty() {
            return Err(keymap_error(path, format!("line {} has no key", index + 1)));
        }
        let mut replacements = Vec::new();
        for alternative in value.split(',') {
            let sequence = alternative
                .split_whitespace()
                .map(parse_stroke)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|detail| keymap_error(path, format!("line {}: {detail}", index + 1)))?;
            if sequence.is_empty() {
                return Err(keymap_error(
                    path,
                    format!("line {} has an empty key sequence", index + 1),
                ));
            }
            replacements.push(Binding::new(sequence, action));
        }
        // An explicit close assignment replaces both default close keys. Its
        // keys (including Esc) keep immediate close semantics.
        bindings.retain(|binding| {
            binding.action != action
                && !(action == Action::CloseOverlay
                    && binding.action == Action::DismissSearchOrClose)
        });
        bindings.extend(replacements);
    }
    validate_bindings(path, &bindings)?;
    Ok(bindings)
}

pub(super) fn parse_stroke(value: &str) -> Result<KeyStroke, String> {
    let mut modifiers = KeyModifiers::NONE;
    let mut raw_key = value;
    loop {
        let folded = raw_key.to_ascii_lowercase();
        if folded.starts_with("ctrl-") {
            modifiers.insert(KeyModifiers::CONTROL);
            raw_key = &raw_key[5..];
        } else if folded.starts_with("alt-") {
            modifiers.insert(KeyModifiers::ALT);
            raw_key = &raw_key[4..];
        } else if folded.starts_with("shift-") {
            modifiers.insert(KeyModifiers::SHIFT);
            raw_key = &raw_key[6..];
        } else {
            break;
        }
    }
    let folded = raw_key.to_ascii_lowercase();
    let key = folded.as_str();
    let code = match key {
        "enter" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        "comma" => KeyCode::Char(','),
        "backspace" => KeyCode::Backspace,
        "tab" => KeyCode::Tab,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "page-up" => KeyCode::PageUp,
        "pagedown" | "page-down" => KeyCode::PageDown,
        key if key.len() > 1
            && key.starts_with('f')
            && key[1..].bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            let number = key[1..]
                .parse::<u8>()
                .map_err(|_| format!("invalid function key {value:?}"))?;
            if number == 0 {
                return Err(format!("invalid function key {value:?}"));
            }
            KeyCode::F(number)
        }
        _ => {
            let mut characters = raw_key.chars();
            let mut character = characters
                .next()
                .ok_or_else(|| "key must not be empty".to_owned())?;
            if characters.next().is_some() {
                return Err(format!("unknown key {value:?}"));
            }
            if modifiers.contains(KeyModifiers::SHIFT) {
                let mut uppercase = character.to_uppercase();
                character = uppercase
                    .next()
                    .ok_or_else(|| format!("unknown key {value:?}"))?;
                if uppercase.next().is_some() {
                    return Err(format!(
                        "shifted key {value:?} expands to multiple characters"
                    ));
                }
                modifiers.remove(KeyModifiers::SHIFT);
            }
            KeyCode::Char(character)
        }
    };
    Ok(KeyStroke::new(code, modifiers))
}

fn validate_bindings(path: &Path, bindings: &[Binding]) -> Result<(), KeyMapError> {
    for (index, binding) in bindings.iter().enumerate() {
        if binding.sequence.first().is_some_and(|stroke| {
            matches!(stroke.code, KeyCode::Char('1'..='9'))
                && stroke.modifiers == KeyModifiers::NONE
        }) {
            return Err(keymap_error(
                path,
                "digits 1 through 9 are reserved for Vim counts; use a leader or modifier"
                    .to_owned(),
            ));
        }
        for other in &bindings[index + 1..] {
            let ambiguous = binding.sequence == other.sequence
                || binding.sequence.starts_with(&other.sequence)
                || other.sequence.starts_with(&binding.sequence);
            if ambiguous {
                return Err(keymap_error(
                    path,
                    "two actions use the same key or an ambiguous key prefix".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

fn keymap_error(path: &Path, detail: String) -> KeyMapError {
    KeyMapError {
        path: path.to_path_buf(),
        detail,
    }
}

pub(super) fn default_bindings() -> Vec<Binding> {
    let single = |key, action| Binding::new(vec![key], action);
    let character = |value| KeyStroke::new(KeyCode::Char(value), KeyModifiers::NONE);
    let control = |value| KeyStroke::new(KeyCode::Char(value), KeyModifiers::CONTROL);
    let motion = |kind| Action::VimMotion(VimMotion::new(kind));
    vec![
        single(character('Q'), Action::Quit),
        single(control('c'), Action::Quit),
        single(character('q'), Action::CloseOverlay),
        Binding::new(vec![character('\\'), character('1')], Action::ShowChanges),
        Binding::new(vec![character('\\'), character('2')], Action::ShowHistory),
        Binding::new(vec![character('\\'), character('3')], Action::ShowGraph),
        Binding::new(vec![character('\\'), character('4')], Action::ShowCode),
        Binding::new(vec![control('w'), character('h')], Action::FocusLeft),
        Binding::new(vec![control('w'), character('k')], Action::FocusLeft),
        Binding::new(vec![control('w'), character('j')], Action::FocusRight),
        Binding::new(vec![control('w'), character('l')], Action::FocusRight),
        Binding::new(vec![control('w'), control('h')], Action::FocusLeft),
        Binding::new(vec![control('w'), control('k')], Action::FocusLeft),
        Binding::new(vec![control('w'), control('j')], Action::FocusRight),
        Binding::new(vec![control('w'), control('l')], Action::FocusRight),
        Binding::new(
            vec![
                control('w'),
                KeyStroke::new(KeyCode::Backspace, KeyModifiers::NONE),
            ],
            Action::FocusLeft,
        ),
        Binding::new(vec![control('w'), character('W')], Action::FocusLeft),
        Binding::new(vec![control('w'), character('w')], Action::FocusRight),
        Binding::new(vec![control('w'), control('w')], Action::FocusRight),
        Binding::new(
            vec![
                control('w'),
                KeyStroke::new(KeyCode::Left, KeyModifiers::NONE),
            ],
            Action::FocusLeft,
        ),
        Binding::new(
            vec![
                control('w'),
                KeyStroke::new(KeyCode::Up, KeyModifiers::NONE),
            ],
            Action::FocusLeft,
        ),
        Binding::new(
            vec![
                control('w'),
                KeyStroke::new(KeyCode::Right, KeyModifiers::NONE),
            ],
            Action::FocusRight,
        ),
        Binding::new(
            vec![
                control('w'),
                KeyStroke::new(KeyCode::Down, KeyModifiers::NONE),
            ],
            Action::FocusRight,
        ),
        single(character('h'), motion(VimMotionKind::Left)),
        single(character('l'), motion(VimMotionKind::Right)),
        single(character(' '), motion(VimMotionKind::RightWrap)),
        single(character('k'), motion(VimMotionKind::Up)),
        single(
            KeyStroke::new(KeyCode::Up, KeyModifiers::NONE),
            motion(VimMotionKind::Up),
        ),
        single(control('p'), motion(VimMotionKind::Up)),
        single(character('j'), motion(VimMotionKind::Down)),
        single(
            KeyStroke::new(KeyCode::Down, KeyModifiers::NONE),
            motion(VimMotionKind::Down),
        ),
        single(control('j'), motion(VimMotionKind::Down)),
        single(control('n'), motion(VimMotionKind::Down)),
        single(
            KeyStroke::new(KeyCode::Left, KeyModifiers::NONE),
            motion(VimMotionKind::Left),
        ),
        single(
            KeyStroke::new(KeyCode::Right, KeyModifiers::NONE),
            motion(VimMotionKind::Right),
        ),
        single(
            KeyStroke::new(KeyCode::Backspace, KeyModifiers::NONE),
            motion(VimMotionKind::LeftWrap),
        ),
        single(control('h'), motion(VimMotionKind::LeftWrap)),
        single(character('0'), motion(VimMotionKind::LineStart)),
        single(character('^'), motion(VimMotionKind::FirstNonBlank)),
        single(character('$'), motion(VimMotionKind::LineEnd)),
        Binding::new(
            vec![character('g'), character('_')],
            motion(VimMotionKind::LastNonBlank),
        ),
        Binding::new(
            vec![character('g'), character('0')],
            motion(VimMotionKind::ScreenLineStart),
        ),
        Binding::new(
            vec![character('g'), character('^')],
            motion(VimMotionKind::ScreenFirstNonBlank),
        ),
        Binding::new(
            vec![character('g'), character('$')],
            motion(VimMotionKind::ScreenLineEnd),
        ),
        Binding::new(
            vec![character('g'), character('m')],
            motion(VimMotionKind::ScreenMiddle),
        ),
        Binding::new(
            vec![character('g'), character('M')],
            motion(VimMotionKind::LineMiddle),
        ),
        Binding::new(
            vec![character('g'), character('j')],
            motion(VimMotionKind::Down),
        ),
        Binding::new(
            vec![character('g'), character('k')],
            motion(VimMotionKind::Up),
        ),
        Binding::new(
            vec![character('g'), character('o')],
            motion(VimMotionKind::ByteOffset),
        ),
        Binding::new(
            vec![
                character('g'),
                KeyStroke::new(KeyCode::Up, KeyModifiers::NONE),
            ],
            motion(VimMotionKind::Up),
        ),
        Binding::new(
            vec![
                character('g'),
                KeyStroke::new(KeyCode::Down, KeyModifiers::NONE),
            ],
            motion(VimMotionKind::Down),
        ),
        Binding::new(
            vec![
                character('g'),
                KeyStroke::new(KeyCode::Home, KeyModifiers::NONE),
            ],
            motion(VimMotionKind::ScreenLineStart),
        ),
        Binding::new(
            vec![
                character('g'),
                KeyStroke::new(KeyCode::End, KeyModifiers::NONE),
            ],
            motion(VimMotionKind::ScreenLastNonBlank),
        ),
        single(character('|'), motion(VimMotionKind::Column)),
        single(character('w'), motion(VimMotionKind::WordForward)),
        single(character('W'), motion(VimMotionKind::BigWordForward)),
        single(character('e'), motion(VimMotionKind::WordEndForward)),
        single(character('E'), motion(VimMotionKind::BigWordEndForward)),
        single(character('b'), motion(VimMotionKind::WordBackward)),
        single(character('B'), motion(VimMotionKind::BigWordBackward)),
        Binding::new(
            vec![character('g'), character('e')],
            motion(VimMotionKind::WordEndBackward),
        ),
        Binding::new(
            vec![character('g'), character('E')],
            motion(VimMotionKind::BigWordEndBackward),
        ),
        single(character('f'), motion(VimMotionKind::FindForward)),
        single(character('F'), motion(VimMotionKind::FindBackward)),
        single(character('t'), motion(VimMotionKind::TillForward)),
        single(character('T'), motion(VimMotionKind::TillBackward)),
        single(character(';'), motion(VimMotionKind::RepeatCharacterSearch)),
        single(
            character(','),
            motion(VimMotionKind::ReverseCharacterSearch),
        ),
        single(
            KeyStroke::new(KeyCode::Left, KeyModifiers::CONTROL),
            motion(VimMotionKind::BigWordBackward),
        ),
        single(
            KeyStroke::new(KeyCode::Right, KeyModifiers::CONTROL),
            motion(VimMotionKind::BigWordForward),
        ),
        single(
            KeyStroke::new(KeyCode::Left, KeyModifiers::SHIFT),
            motion(VimMotionKind::WordBackward),
        ),
        single(
            KeyStroke::new(KeyCode::Right, KeyModifiers::SHIFT),
            motion(VimMotionKind::WordForward),
        ),
        single(
            character('-'),
            motion(VimMotionKind::PreviousLineFirstNonBlank),
        ),
        single(character('+'), motion(VimMotionKind::NextLineFirstNonBlank)),
        single(
            character('_'),
            motion(VimMotionKind::CountedLineFirstNonBlank),
        ),
        Binding::new(
            vec![character('g'), character('g')],
            motion(VimMotionKind::BufferTop),
        ),
        single(
            KeyStroke::new(KeyCode::Home, KeyModifiers::NONE),
            motion(VimMotionKind::LineStart),
        ),
        single(
            KeyStroke::new(KeyCode::Home, KeyModifiers::CONTROL),
            motion(VimMotionKind::BufferTop),
        ),
        single(character('G'), motion(VimMotionKind::BufferBottom)),
        single(
            KeyStroke::new(KeyCode::End, KeyModifiers::NONE),
            motion(VimMotionKind::LineEnd),
        ),
        single(
            KeyStroke::new(KeyCode::End, KeyModifiers::CONTROL),
            motion(VimMotionKind::BufferBottomEnd),
        ),
        single(character('%'), motion(VimMotionKind::MatchingPair)),
        Binding::new(
            vec![character('g'), character('%')],
            motion(VimMotionKind::MatchingPairBackward),
        ),
        single(character('m'), Action::SetVimMark('\0')),
        single(
            character('\''),
            Action::JumpToVimMark {
                mark: '\0',
                linewise: true,
                record_jump: true,
            },
        ),
        single(
            character('`'),
            Action::JumpToVimMark {
                mark: '\0',
                linewise: false,
                record_jump: true,
            },
        ),
        Binding::new(
            vec![character('g'), character('\'')],
            Action::JumpToVimMark {
                mark: '\0',
                linewise: true,
                record_jump: false,
            },
        ),
        Binding::new(
            vec![character('g'), character('`')],
            Action::JumpToVimMark {
                mark: '\0',
                linewise: false,
                record_jump: false,
            },
        ),
        Binding::new(
            vec![character('['), character('\'')],
            motion(VimMotionKind::PreviousMarkLine),
        ),
        Binding::new(
            vec![character('['), character('`')],
            motion(VimMotionKind::PreviousMarkExact),
        ),
        Binding::new(
            vec![character(']'), character('\'')],
            motion(VimMotionKind::NextMarkLine),
        ),
        Binding::new(
            vec![character(']'), character('`')],
            motion(VimMotionKind::NextMarkExact),
        ),
        single(character('H'), motion(VimMotionKind::WindowTop)),
        single(character('M'), motion(VimMotionKind::WindowMiddle)),
        single(character('L'), motion(VimMotionKind::WindowBottom)),
        single(character('('), motion(VimMotionKind::SentenceBackward)),
        single(character(')'), motion(VimMotionKind::SentenceForward)),
        single(character('{'), motion(VimMotionKind::ParagraphBackward)),
        single(character('}'), motion(VimMotionKind::ParagraphForward)),
        Binding::new(
            vec![character('['), character('[')],
            motion(VimMotionKind::SectionStartBackward),
        ),
        Binding::new(
            vec![character(']'), character(']')],
            motion(VimMotionKind::SectionStartForward),
        ),
        Binding::new(
            vec![character('['), character(']')],
            motion(VimMotionKind::SectionEndBackward),
        ),
        Binding::new(
            vec![character(']'), character('[')],
            motion(VimMotionKind::SectionEndForward),
        ),
        Binding::new(
            vec![character('['), character('(')],
            Action::VimMotion(VimMotion::new(VimMotionKind::UnmatchedOpenBackward).targeting('(')),
        ),
        Binding::new(
            vec![character('['), character('{')],
            Action::VimMotion(VimMotion::new(VimMotionKind::UnmatchedOpenBackward).targeting('{')),
        ),
        Binding::new(
            vec![character(']'), character(')')],
            Action::VimMotion(VimMotion::new(VimMotionKind::UnmatchedCloseForward).targeting(')')),
        ),
        Binding::new(
            vec![character(']'), character('}')],
            Action::VimMotion(VimMotion::new(VimMotionKind::UnmatchedCloseForward).targeting('}')),
        ),
        Binding::new(
            vec![character('['), character('m')],
            motion(VimMotionKind::MethodBackward),
        ),
        Binding::new(
            vec![character('['), character('M')],
            Action::VimMotion(VimMotion::new(VimMotionKind::MethodBackward).targeting('M')),
        ),
        Binding::new(
            vec![character(']'), character('m')],
            motion(VimMotionKind::MethodForward),
        ),
        Binding::new(
            vec![character(']'), character('M')],
            Action::VimMotion(VimMotion::new(VimMotionKind::MethodForward).targeting('M')),
        ),
        Binding::new(
            vec![character('['), character('#')],
            motion(VimMotionKind::PreprocessorBackward),
        ),
        Binding::new(
            vec![character(']'), character('#')],
            motion(VimMotionKind::PreprocessorForward),
        ),
        Binding::new(
            vec![character('['), character('*')],
            motion(VimMotionKind::CommentBackward),
        ),
        Binding::new(
            vec![character('['), character('/')],
            motion(VimMotionKind::CommentBackward),
        ),
        Binding::new(
            vec![character(']'), character('*')],
            motion(VimMotionKind::CommentForward),
        ),
        Binding::new(
            vec![character(']'), character('/')],
            motion(VimMotionKind::CommentForward),
        ),
        Binding::new(
            vec![character('['), character('c')],
            motion(VimMotionKind::DiffChangeBackward),
        ),
        Binding::new(
            vec![character(']'), character('c')],
            motion(VimMotionKind::DiffChangeForward),
        ),
        single(control('u'), motion(VimMotionKind::HalfPageUp)),
        single(control('d'), motion(VimMotionKind::HalfPageDown)),
        single(control('b'), motion(VimMotionKind::PageUp)),
        single(control('f'), motion(VimMotionKind::PageDown)),
        single(
            KeyStroke::new(KeyCode::PageUp, KeyModifiers::NONE),
            motion(VimMotionKind::PageUp),
        ),
        single(
            KeyStroke::new(KeyCode::PageDown, KeyModifiers::NONE),
            motion(VimMotionKind::PageDown),
        ),
        single(
            KeyStroke::new(KeyCode::Up, KeyModifiers::SHIFT),
            motion(VimMotionKind::PageUp),
        ),
        single(
            KeyStroke::new(KeyCode::Down, KeyModifiers::SHIFT),
            motion(VimMotionKind::PageDown),
        ),
        single(
            KeyStroke::new(KeyCode::Enter, KeyModifiers::SHIFT),
            motion(VimMotionKind::PageDown),
        ),
        single(control('e'), motion(VimMotionKind::ScrollLineDown)),
        single(control('y'), motion(VimMotionKind::ScrollLineUp)),
        Binding::new(
            vec![character('z'), character('h')],
            motion(VimMotionKind::ScrollColumnLeft),
        ),
        Binding::new(
            vec![
                character('z'),
                KeyStroke::new(KeyCode::Left, KeyModifiers::NONE),
            ],
            motion(VimMotionKind::ScrollColumnLeft),
        ),
        Binding::new(
            vec![character('z'), character('l')],
            motion(VimMotionKind::ScrollColumnRight),
        ),
        Binding::new(
            vec![
                character('z'),
                KeyStroke::new(KeyCode::Right, KeyModifiers::NONE),
            ],
            motion(VimMotionKind::ScrollColumnRight),
        ),
        Binding::new(
            vec![character('z'), character('H')],
            motion(VimMotionKind::ScrollHalfScreenLeft),
        ),
        Binding::new(
            vec![character('z'), character('L')],
            motion(VimMotionKind::ScrollHalfScreenRight),
        ),
        Binding::new(
            vec![character('z'), character('t')],
            motion(VimMotionKind::CursorToWindowTop),
        ),
        Binding::new(
            vec![
                character('z'),
                KeyStroke::new(KeyCode::Enter, KeyModifiers::NONE),
            ],
            motion(VimMotionKind::CursorToWindowTopFirstNonBlank),
        ),
        Binding::new(
            vec![character('z'), character('z')],
            motion(VimMotionKind::CursorToWindowMiddle),
        ),
        Binding::new(
            vec![character('z'), character('.')],
            motion(VimMotionKind::CursorToWindowMiddleFirstNonBlank),
        ),
        Binding::new(
            vec![character('z'), character('b')],
            motion(VimMotionKind::CursorToWindowBottom),
        ),
        Binding::new(
            vec![character('z'), character('-')],
            motion(VimMotionKind::CursorToWindowBottomFirstNonBlank),
        ),
        Binding::new(
            vec![character('z'), character('+')],
            motion(VimMotionKind::NextWindowTop),
        ),
        Binding::new(
            vec![character('z'), character('^')],
            motion(VimMotionKind::PreviousWindowBottom),
        ),
        Binding::new(
            vec![character('z'), character('s')],
            motion(VimMotionKind::CursorToWindowLeft),
        ),
        Binding::new(
            vec![character('z'), character('e')],
            motion(VimMotionKind::CursorToWindowRight),
        ),
        single(character('r'), Action::Refresh),
        single(
            KeyStroke::new(KeyCode::Enter, KeyModifiers::NONE),
            Action::Activate,
        ),
        Binding::new(
            vec![character('\\'), character('f')],
            Action::OpenFileSearch,
        ),
        Binding::new(
            vec![character('\\'), character('g')],
            Action::OpenContentSearch,
        ),
        Binding::new(vec![character('\\'), character('m')], Action::ToggleMessage),
        Binding::new(vec![character('\\'), character('b')], Action::ToggleDetails),
        Binding::new(vec![character('\\'), character('t')], Action::ToggleTree),
        Binding::new(
            vec![character('g'), character('d')],
            Action::GoToSemanticTarget(SemanticNavigationKind::Definition),
        ),
        Binding::new(
            vec![character('g'), character('i')],
            Action::GoToSemanticTarget(SemanticNavigationKind::Implementation),
        ),
        Binding::new(
            vec![character('g'), character('y')],
            Action::GoToSemanticTarget(SemanticNavigationKind::TypeDefinition),
        ),
        Binding::new(
            vec![character('g'), character('D')],
            Action::GoToSemanticTarget(SemanticNavigationKind::Declaration),
        ),
        single(control('o'), Action::JumpListBack(1)),
        single(control('i'), Action::JumpListForward(1)),
        single(
            KeyStroke::new(KeyCode::Tab, KeyModifiers::NONE),
            Action::JumpListForward(1),
        ),
        single(character('K'), Action::ToggleLspHover),
        single(
            character('/'),
            Action::StartSearch(SearchDirection::Forward),
        ),
        single(
            character('?'),
            Action::StartSearch(SearchDirection::Backward),
        ),
        single(character('n'), motion(VimMotionKind::SearchNext)),
        single(character('N'), motion(VimMotionKind::SearchPrevious)),
        single(character('*'), motion(VimMotionKind::SearchWordForward)),
        single(character('#'), motion(VimMotionKind::SearchWordBackward)),
        Binding::new(
            vec![character('g'), character('*')],
            motion(VimMotionKind::SearchPartialWordForward),
        ),
        Binding::new(
            vec![character('g'), character('#')],
            motion(VimMotionKind::SearchPartialWordBackward),
        ),
        single(
            KeyStroke::new(KeyCode::F(1), KeyModifiers::NONE),
            Action::ToggleHelp,
        ),
        single(
            KeyStroke::new(KeyCode::Esc, KeyModifiers::NONE),
            Action::DismissSearchOrClose,
        ),
    ]
}
