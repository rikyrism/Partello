//! Input normalisation shared by REST and WebSocket handlers.
//! The frontend validates too, but these functions are the source of truth.

use crate::error::AppError;

pub const DISPLAY_NAME_MAX: usize = 32;
pub const ROOM_NAME_MAX: usize = 60;

/// Trims, collapses inner whitespace, and rejects control/format characters.
pub fn clean_line(input: &str, field: &str, max_chars: usize) -> Result<String, AppError> {
    if input.chars().any(|c| c.is_control() || is_invisible_format(c)) {
        return Err(AppError::validation(format!("{field} contains invalid characters")));
    }
    let cleaned = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let len = cleaned.chars().count();
    if len == 0 {
        return Err(AppError::validation(format!("{field} is required")));
    }
    if len > max_chars {
        return Err(AppError::validation(format!("{field} must be at most {max_chars} characters")));
    }
    Ok(cleaned)
}

pub fn display_name(input: &str) -> Result<String, AppError> {
    clean_line(input, "Display name", DISPLAY_NAME_MAX)
}

/// Rooms can be locked with a 4-digit PIN.
pub fn room_pin(input: &str) -> Result<String, AppError> {
    if input.len() == 4 && input.bytes().all(|b| b.is_ascii_digit()) {
        Ok(input.to_string())
    } else {
        Err(AppError::validation("PIN must be exactly 4 digits"))
    }
}

pub fn room_name(input: &str) -> Result<String, AppError> {
    clean_line(input, "Room name", ROOM_NAME_MAX)
}

/// Chat bodies may contain newlines (Shift+Enter) but no other control characters.
pub fn chat_body(input: &str, max_chars: usize) -> Result<String, AppError> {
    if input.chars().any(|c| (c.is_control() && c != '\n') || is_invisible_format(c)) {
        return Err(AppError::validation("Message contains invalid characters"));
    }
    let trimmed = input.trim();
    // At most two consecutive blank lines.
    let mut out = String::with_capacity(trimmed.len());
    let mut newlines = 0;
    for c in trimmed.chars() {
        if c == '\n' {
            newlines += 1;
            if newlines > 2 {
                continue;
            }
        } else {
            newlines = 0;
        }
        out.push(c);
    }
    let len = out.chars().count();
    if len == 0 {
        return Err(AppError::validation("Message is empty"));
    }
    if len > max_chars {
        return Err(AppError::validation(format!("Message must be at most {max_chars} characters")));
    }
    Ok(out)
}

/// Bidi overrides and zero-width characters can be used to spoof names.
fn is_invisible_format(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names() {
        assert_eq!(display_name("  Ana   Maria ").unwrap(), "Ana Maria");
        assert!(display_name("   ").is_err());
        assert!(display_name(&"x".repeat(33)).is_err());
        assert!(display_name(&"é".repeat(32)).is_ok(), "limit counts characters, not bytes");
        assert!(display_name("bad\u{0007}name").is_err());
        assert!(display_name("evil\u{202E}name").is_err());
    }

    #[test]
    fn room_pins() {
        assert_eq!(room_pin("0420").unwrap(), "0420");
        for bad in ["", "123", "12345", "12a4", " 123", "١٢٣٤"] {
            assert!(room_pin(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn chat_bodies() {
        assert_eq!(chat_body("  hi there  ", 500).unwrap(), "hi there");
        assert_eq!(chat_body("a\n\n\n\n\nb", 500).unwrap(), "a\n\nb");
        assert!(chat_body("\n\n", 500).is_err());
        assert!(chat_body(&"a".repeat(501), 500).is_err());
        assert!(chat_body("tab\tchar", 500).is_err());
    }
}
