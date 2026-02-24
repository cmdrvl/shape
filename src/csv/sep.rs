use crate::cli::delimiter::is_supported_delimiter_byte;

/// Parsed `sep=` directive metadata from the first line of a CSV file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SepDirective {
    pub delimiter: u8,
    pub consumed_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelimiterSource {
    Forced,
    SepDirective,
    AutoDetect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelimiterResolution {
    pub delimiter: u8,
    pub consumed_bytes: usize,
    pub source: DelimiterSource,
}

impl DelimiterResolution {
    pub fn from_forced(delimiter: u8, consumed_bytes: usize) -> Self {
        Self {
            delimiter,
            consumed_bytes,
            source: DelimiterSource::Forced,
        }
    }

    pub fn from_sep(sep: SepDirective) -> Self {
        Self {
            delimiter: sep.delimiter,
            consumed_bytes: sep.consumed_bytes,
            source: DelimiterSource::SepDirective,
        }
    }

    pub fn from_autodetect(delimiter: u8) -> Self {
        Self {
            delimiter,
            consumed_bytes: 0,
            source: DelimiterSource::AutoDetect,
        }
    }
}

/// Detect and parse `sep=<byte>` from the first line.
pub fn detect_sep_directive(bytes: &[u8]) -> Option<SepDirective> {
    let line_end = bytes
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(bytes.len());
    let mut line = &bytes[..line_end];

    let consumed_bytes = if line_end < bytes.len() {
        line_end + 1
    } else {
        line_end
    };

    if line.last().copied() == Some(b'\r') {
        line = &line[..line.len() - 1];
    }
    if line.len() != 5 || !line.starts_with(b"sep=") {
        return None;
    }

    let delimiter = line[4];
    if !is_supported_delimiter_byte(delimiter) {
        return None;
    }

    Some(SepDirective {
        delimiter,
        consumed_bytes,
    })
}

/// Resolve delimiter precedence for one file.
///
/// Priority: forced `--delimiter` -> `sep=` directive -> auto-detected delimiter.
pub fn resolve_delimiter(
    forced: Option<u8>,
    bytes: &[u8],
    autodetected: u8,
) -> DelimiterResolution {
    let sep = detect_sep_directive(bytes);

    if let Some(delimiter) = forced {
        return DelimiterResolution::from_forced(
            delimiter,
            sep.map_or(0, |directive| directive.consumed_bytes),
        );
    }
    if let Some(sep) = sep {
        return DelimiterResolution::from_sep(sep);
    }
    DelimiterResolution::from_autodetect(autodetected)
}

#[cfg(test)]
mod tests {
    use super::{
        DelimiterResolution, DelimiterSource, SepDirective, detect_sep_directive, resolve_delimiter,
    };

    #[test]
    fn consumes_exact_sep_line_with_lf_and_crlf() {
        assert_eq!(
            detect_sep_directive(b"sep=,\nloan_id,amount\n1,10\n"),
            Some(SepDirective {
                delimiter: b',',
                consumed_bytes: 6,
            })
        );
        assert_eq!(
            detect_sep_directive(b"sep=;\r\nloan_id;amount\r\n1;10\r\n"),
            Some(SepDirective {
                delimiter: b';',
                consumed_bytes: 7,
            })
        );
    }

    #[test]
    fn supports_exact_match_without_trailing_newline() {
        assert_eq!(
            detect_sep_directive(b"sep=|"),
            Some(SepDirective {
                delimiter: b'|',
                consumed_bytes: 5,
            })
        );
        assert_eq!(
            detect_sep_directive(b"sep==\nloan_id=amount\n"),
            Some(SepDirective {
                delimiter: b'=',
                consumed_bytes: 6,
            })
        );
    }

    #[test]
    fn ignores_non_exact_or_disallowed_first_line_forms() {
        for input in [
            b"SEP=,\nloan_id,amount\n"[..].as_ref(),
            b"sep=,\x20\nloan_id,amount\n".as_ref(),
            b" sep=,\nloan_id,amount\n".as_ref(),
            b"sep=\nloan_id,amount\n".as_ref(),
            b"sep=\"\nloan_id,amount\n".as_ref(),
            b"sep=\x7f\nloan_id,amount\n".as_ref(),
        ] {
            assert!(
                detect_sep_directive(input).is_none(),
                "unexpected match for {:?}",
                input
            );
        }
    }

    #[test]
    fn resolve_delimiter_honors_precedence() {
        let forced = resolve_delimiter(Some(b'|'), b"sep=,\na,b\n", b';');
        assert_eq!(
            forced,
            DelimiterResolution {
                delimiter: b'|',
                consumed_bytes: 6,
                source: DelimiterSource::Forced,
            }
        );

        let forced_no_sep = resolve_delimiter(Some(b'|'), b"a,b\n", b';');
        assert_eq!(
            forced_no_sep,
            DelimiterResolution {
                delimiter: b'|',
                consumed_bytes: 0,
                source: DelimiterSource::Forced,
            }
        );

        let sep = resolve_delimiter(None, b"sep=,\na,b\n", b';');
        assert_eq!(
            sep,
            DelimiterResolution {
                delimiter: b',',
                consumed_bytes: 6,
                source: DelimiterSource::SepDirective,
            }
        );

        let auto = resolve_delimiter(None, b"a,b\n1,2\n", b';');
        assert_eq!(
            auto,
            DelimiterResolution {
                delimiter: b';',
                consumed_bytes: 0,
                source: DelimiterSource::AutoDetect,
            }
        );
    }
}
