//! Conversion between source byte columns, display cells, and LSP encodings.

use unicode_width::UnicodeWidthChar;

use crate::lsp::LspError;

/// A position encoding negotiated with a language server.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PositionEncoding {
    /// UTF-8 code units (bytes).
    Utf8,
    /// UTF-16 code units, the LSP 3.17 default.
    #[default]
    Utf16,
    /// UTF-32 code units (Unicode scalar values).
    Utf32,
}

impl PositionEncoding {
    pub(crate) fn from_server(value: Option<&str>) -> Self {
        match value {
            Some("utf-8") => Self::Utf8,
            Some("utf-32") => Self::Utf32,
            Some("utf-16") | Some(_) | None => Self::Utf16,
        }
    }
}

pub(crate) fn to_lsp_character(
    line: &str,
    byte_column: usize,
    encoding: PositionEncoding,
) -> Result<u32, LspError> {
    if byte_column > line.len() || !line.is_char_boundary(byte_column) {
        return Err(LspError::InvalidDocument(
            "source cursor is not on a UTF-8 character boundary".to_owned(),
        ));
    }
    let prefix = &line[..byte_column];
    let units = match encoding {
        PositionEncoding::Utf8 => prefix.len(),
        PositionEncoding::Utf16 => prefix.encode_utf16().count(),
        PositionEncoding::Utf32 => prefix.chars().count(),
    };
    u32::try_from(units).map_err(|_| {
        LspError::InvalidDocument("source line is too long for an LSP position".to_owned())
    })
}

pub(crate) fn from_lsp_character(
    line: &str,
    character: u32,
    encoding: PositionEncoding,
) -> Result<usize, LspError> {
    let requested = usize::try_from(character).map_err(|_| {
        LspError::Protocol("language server returned an invalid source position".to_owned())
    })?;
    if encoding == PositionEncoding::Utf8 {
        if requested <= line.len() && line.is_char_boundary(requested) {
            return Ok(requested);
        }
        return Err(LspError::Protocol(
            "language server returned a non-boundary UTF-8 position".to_owned(),
        ));
    }
    let mut units = 0usize;
    for (byte, character) in line.char_indices() {
        if units == requested {
            return Ok(byte);
        }
        units = units.saturating_add(match encoding {
            PositionEncoding::Utf16 => character.len_utf16(),
            PositionEncoding::Utf32 => 1,
            PositionEncoding::Utf8 => unreachable!("UTF-8 handled above"),
        });
        if units > requested {
            return Err(LspError::Protocol(
                "language server returned a position inside a code point".to_owned(),
            ));
        }
    }
    if units == requested {
        Ok(line.len())
    } else {
        Err(LspError::Protocol(
            "language server returned a position past the end of a line".to_owned(),
        ))
    }
}

/// Returns the next UTF-8 character boundary, clamped to the line end.
#[must_use]
pub fn next_byte_column(line: &str, byte_column: usize) -> usize {
    let start = clamp_boundary(line, byte_column);
    line[start..]
        .chars()
        .next()
        .map_or(start, |character| start + character.len_utf8())
}

/// Returns the previous UTF-8 character boundary, clamped to zero.
#[must_use]
pub fn previous_byte_column(line: &str, byte_column: usize) -> usize {
    let end = clamp_boundary(line, byte_column);
    line[..end]
        .char_indices()
        .next_back()
        .map_or(0, |(byte, _)| byte)
}

/// Converts a UTF-8 byte column to terminal display cells using four-cell tabs.
#[must_use]
pub fn display_column(line: &str, byte_column: usize) -> usize {
    let end = clamp_boundary(line, byte_column);
    line[..end].chars().fold(0usize, |column, character| {
        if character == '\t' {
            column.saturating_add(4)
        } else {
            column.saturating_add(UnicodeWidthChar::width(character).unwrap_or(0))
        }
    })
}

fn clamp_boundary(line: &str, byte_column: usize) -> usize {
    let mut column = byte_column.min(line.len());
    while !line.is_char_boundary(column) {
        column = column.saturating_sub(1);
    }
    column
}

#[cfg(test)]
mod tests {
    use super::{
        PositionEncoding, display_column, from_lsp_character, next_byte_column,
        previous_byte_column, to_lsp_character,
    };

    #[test]
    fn converts_multibyte_positions_for_all_encodings() {
        let line = "a😀界e\u{301}";
        let byte = "a😀界".len();
        assert_eq!(to_lsp_character(line, byte, PositionEncoding::Utf8), Ok(8));
        assert_eq!(to_lsp_character(line, byte, PositionEncoding::Utf16), Ok(4));
        assert_eq!(to_lsp_character(line, byte, PositionEncoding::Utf32), Ok(3));
        assert_eq!(
            from_lsp_character(line, 4, PositionEncoding::Utf16),
            Ok(byte)
        );
        assert!(from_lsp_character(line, 2, PositionEncoding::Utf16).is_err());
    }

    #[test]
    fn cursor_steps_only_between_code_points() {
        let line = "a界😀";
        assert_eq!(next_byte_column(line, 1), 4);
        assert_eq!(next_byte_column(line, 4), 8);
        assert_eq!(previous_byte_column(line, 8), 4);
        assert_eq!(previous_byte_column(line, 3), 0);
    }

    #[test]
    fn display_width_accounts_for_tabs_wide_and_combining_characters() {
        let line = "\t界e\u{301}";
        assert_eq!(display_column(line, 1), 4);
        assert_eq!(display_column(line, "\t界".len()), 6);
        assert_eq!(display_column(line, line.len()), 7);
        assert_eq!(display_column("a\tb", 2), 5);
    }
}
