/// Named delimiter keywords accepted by the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelimiterKind {
    Comma,
    Tab,
    Semicolon,
    Pipe,
    Caret,
}

impl DelimiterKind {
    pub fn as_byte(self) -> u8 {
        match self {
            Self::Comma => b',',
            Self::Tab => b'\t',
            Self::Semicolon => b';',
            Self::Pipe => b'|',
            Self::Caret => b'^',
        }
    }
}

/// Parse a delimiter argument into a byte.
pub fn parse_delimiter(input: &str) -> Result<u8, String> {
    if input.is_empty() {
        return Err("delimiter cannot be empty".to_owned());
    }

    // Preserve literal single-byte delimiters (including whitespace like
    // a single space or tab) before keyword/hex trimming logic.
    if input.len() == 1 {
        let byte = input.as_bytes()[0];
        return if is_supported_delimiter_byte(byte) {
            Ok(byte)
        } else {
            Err(format!("unsupported delimiter byte: {input}"))
        };
    }

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("delimiter cannot be empty".to_owned());
    }

    let lower = trimmed.to_ascii_lowercase();
    let parsed = match lower.as_str() {
        "comma" => Some(DelimiterKind::Comma.as_byte()),
        "tab" => Some(DelimiterKind::Tab.as_byte()),
        "semicolon" => Some(DelimiterKind::Semicolon.as_byte()),
        "pipe" => Some(DelimiterKind::Pipe.as_byte()),
        "caret" => Some(DelimiterKind::Caret.as_byte()),
        _ => {
            if let Some(hex) = lower.strip_prefix("0x") {
                Some(parse_hex_byte(hex)?)
            } else if trimmed.len() == 1 {
                Some(trimmed.as_bytes()[0])
            } else {
                None
            }
        }
    };

    match parsed {
        Some(byte) if is_supported_delimiter_byte(byte) => Ok(byte),
        Some(_) => Err(format!("unsupported delimiter byte: {trimmed}")),
        None => Err(format!("unsupported delimiter value: {trimmed}")),
    }
}

fn parse_hex_byte(hex: &str) -> Result<u8, String> {
    if hex.len() != 2 {
        return Err("hex delimiter must be exactly 2 hex digits (e.g. 0x2c)".to_owned());
    }
    u8::from_str_radix(hex, 16)
        .map_err(|_| "invalid hex delimiter value; expected two hex digits".to_owned())
}

pub fn is_supported_delimiter_byte(byte: u8) -> bool {
    byte.is_ascii() && byte != b'"' && byte != b'\n' && byte != b'\r' && byte != 0 && byte != 0x7f
}

#[cfg(test)]
mod tests {
    use super::{is_supported_delimiter_byte, parse_delimiter};

    #[test]
    fn accepts_named_delimiters_case_insensitively() {
        let cases = [
            ("comma", b','),
            ("TAB", b'\t'),
            ("Semicolon", b';'),
            ("pipe", b'|'),
            ("CaReT", b'^'),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_delimiter(input).unwrap(), expected, "input={input}");
        }
    }

    #[test]
    fn accepts_hex_and_literal_delimiters() {
        assert_eq!(parse_delimiter("0x2c").unwrap(), b',');
        assert_eq!(parse_delimiter("0X09").unwrap(), b'\t');
        assert_eq!(parse_delimiter("|").unwrap(), b'|');
        assert_eq!(parse_delimiter(" ").unwrap(), b' ');
        assert_eq!(parse_delimiter("\t").unwrap(), b'\t');
    }

    #[test]
    fn rejects_multi_whitespace_as_empty() {
        assert!(parse_delimiter("  ").is_err());
    }

    #[test]
    fn rejects_invalid_hex_and_disallowed_bytes() {
        for input in ["0x2", "0xZZ", "0x2cc"] {
            assert!(parse_delimiter(input).is_err(), "input={input}");
        }
        for input in ["0x00", "0x22", "0x0a", "0x0d", "0x7f", "\""] {
            assert!(parse_delimiter(input).is_err(), "input={input}");
        }
        assert!(!is_supported_delimiter_byte(0));
        assert!(!is_supported_delimiter_byte(b'"'));
        assert!(!is_supported_delimiter_byte(b'\n'));
        assert!(!is_supported_delimiter_byte(b'\r'));
        assert!(!is_supported_delimiter_byte(0x7f));
        assert!(!is_supported_delimiter_byte(0x80));
    }
}
