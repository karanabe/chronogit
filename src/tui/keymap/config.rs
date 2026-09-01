use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyModifiers};

use super::{Binding, KeyStroke, action_for_name};
use crate::app::{Action, SearchDirection};

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
        bindings.retain(|binding| binding.action != action);
        bindings.extend(replacements);
    }
    validate_bindings(path, &bindings)?;
    Ok(bindings)
}

fn parse_stroke(value: &str) -> Result<KeyStroke, String> {
    let folded = value.to_ascii_lowercase();
    let (modifiers, key, raw_key) = if let Some(key) = folded.strip_prefix("ctrl-") {
        (KeyModifiers::CONTROL, key, &value[5..])
    } else if let Some(key) = folded.strip_prefix("alt-") {
        (KeyModifiers::ALT, key, &value[4..])
    } else {
        (KeyModifiers::NONE, folded.as_str(), value)
    };
    let code = match key {
        "enter" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        "backspace" => KeyCode::Backspace,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
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
            let character = characters
                .next()
                .ok_or_else(|| "key must not be empty".to_owned())?;
            if characters.next().is_some() {
                return Err(format!("unknown key {value:?}"));
            }
            KeyCode::Char(character)
        }
    };
    Ok(KeyStroke::new(code, modifiers))
}

fn validate_bindings(path: &Path, bindings: &[Binding]) -> Result<(), KeyMapError> {
    for (index, binding) in bindings.iter().enumerate() {
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
    vec![
        single(character('q'), Action::Quit),
        single(control('c'), Action::Quit),
        single(character('1'), Action::ShowChanges),
        single(character('2'), Action::ShowHistory),
        single(character('3'), Action::ShowGraph),
        single(character('h'), Action::FocusLeft),
        single(control('k'), Action::FocusLeft),
        single(character('l'), Action::FocusRight),
        single(control('j'), Action::FocusRight),
        single(character('k'), Action::MoveUp),
        single(
            KeyStroke::new(KeyCode::Up, KeyModifiers::NONE),
            Action::MoveUp,
        ),
        single(character('j'), Action::MoveDown),
        single(
            KeyStroke::new(KeyCode::Down, KeyModifiers::NONE),
            Action::MoveDown,
        ),
        single(character('g'), Action::MoveTop),
        single(
            KeyStroke::new(KeyCode::Home, KeyModifiers::NONE),
            Action::MoveTop,
        ),
        single(character('G'), Action::MoveBottom),
        single(
            KeyStroke::new(KeyCode::End, KeyModifiers::NONE),
            Action::MoveBottom,
        ),
        single(control('u'), Action::HalfPageUp),
        single(control('d'), Action::HalfPageDown),
        Binding::new(vec![character('z'), character('h')], Action::ScrollLeft),
        Binding::new(vec![character('z'), character('l')], Action::ScrollRight),
        single(character('r'), Action::Refresh),
        single(character('m'), Action::ToggleMessage),
        single(character('b'), Action::ToggleDetails),
        single(character('t'), Action::ToggleTree),
        single(
            KeyStroke::new(KeyCode::Enter, KeyModifiers::NONE),
            Action::Activate,
        ),
        Binding::new(vec![character(' '), character('f')], Action::OpenFileSearch),
        Binding::new(
            vec![character(' '), character('g')],
            Action::OpenContentSearch,
        ),
        single(
            character('/'),
            Action::StartSearch(SearchDirection::Forward),
        ),
        single(
            character('?'),
            Action::StartSearch(SearchDirection::Backward),
        ),
        single(character('n'), Action::NextMatch),
        single(character('N'), Action::PreviousMatch),
        single(
            KeyStroke::new(KeyCode::F(1), KeyModifiers::NONE),
            Action::ToggleHelp,
        ),
        single(
            KeyStroke::new(KeyCode::Esc, KeyModifiers::NONE),
            Action::CloseOverlay,
        ),
    ]
}
