use std::path::{Path, PathBuf};

use crate::csv::dialect::{Dialect, EscapeMode, detect_dialect};
use crate::csv::parser::{
    CsvReaderConfig, line_offset_for_prefix, read_byte_record_with_line_offset, reader_from_bytes,
};
use crate::csv::sep::detect_sep_directive;
use crate::normalize::headers::normalize_headers_or_refusal;
use crate::refusal::codes::RefusalCode;
use crate::refusal::payload::RefusalPayload;

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
const UTF16_BE_BOM: [u8; 2] = [0xFE, 0xFF];
const UTF16_LE_BOM: [u8; 2] = [0xFF, 0xFE];
const UTF32_BE_BOM: [u8; 4] = [0x00, 0x00, 0xFE, 0xFF];
const UTF32_LE_BOM: [u8; 4] = [0xFF, 0xFE, 0x00, 0x00];
const NUL_SCAN_LIMIT_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingIssue {
    Utf32BeBom,
    Utf32LeBom,
    Utf16BeBom,
    Utf16LeBom,
    NulByte,
}

impl EncodingIssue {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Utf32BeBom => "utf32_be_bom",
            Self::Utf32LeBom => "utf32_le_bom",
            Self::Utf16BeBom => "utf16_be_bom",
            Self::Utf16LeBom => "utf16_le_bom",
            Self::NulByte => "nul_byte",
        }
    }
}

/// Read one input file and apply encoding guards.
pub fn read_input_bytes(path: &Path) -> Result<Vec<u8>, RefusalPayload> {
    let raw = std::fs::read(path).map_err(|error| io_refusal(path, &error))?;
    let guarded = guard_input_bytes(path, &raw)?;
    Ok(guarded.to_vec())
}

/// Validate input encoding and return a UTF-8-compatible byte slice.
///
/// Guard order must match PLAN:
/// 1) UTF-32 BOM checks
/// 2) UTF-16 BOM checks
/// 3) UTF-8 BOM strip
/// 4) NUL-byte scan (first 8 KiB)
pub fn guard_input_bytes<'a>(path: &Path, raw: &'a [u8]) -> Result<&'a [u8], RefusalPayload> {
    if raw.starts_with(&UTF32_BE_BOM) {
        return Err(encoding_refusal(path, EncodingIssue::Utf32BeBom));
    }
    if raw.starts_with(&UTF32_LE_BOM) {
        return Err(encoding_refusal(path, EncodingIssue::Utf32LeBom));
    }
    if raw.starts_with(&UTF16_BE_BOM) {
        return Err(encoding_refusal(path, EncodingIssue::Utf16BeBom));
    }
    if raw.starts_with(&UTF16_LE_BOM) {
        return Err(encoding_refusal(path, EncodingIssue::Utf16LeBom));
    }

    let stripped = raw.strip_prefix(&UTF8_BOM).unwrap_or(raw);
    if stripped
        .iter()
        .take(NUL_SCAN_LIMIT_BYTES)
        .any(|&byte| byte == 0)
    {
        return Err(encoding_refusal(path, EncodingIssue::NulByte));
    }

    Ok(stripped)
}

fn io_refusal(path: &Path, error: &std::io::Error) -> RefusalPayload {
    RefusalPayload::io(path.to_string_lossy().to_string(), error.to_string())
}

fn encoding_refusal(path: &Path, issue: EncodingIssue) -> RefusalPayload {
    RefusalPayload::encoding(path.to_string_lossy().to_string(), issue.as_str())
}

/// Parsed state for one CSV input prior to full scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedInput {
    pub path: PathBuf,
    pub raw_bytes: Vec<u8>,
    pub dialect: Dialect,
    pub headers: Vec<Vec<u8>>,
    pub data_offset: usize,
}

impl ParsedInput {
    pub fn new(
        path: PathBuf,
        raw_bytes: Vec<u8>,
        dialect: Dialect,
        headers: Vec<Vec<u8>>,
        data_offset: usize,
    ) -> Self {
        Self {
            path,
            raw_bytes,
            dialect,
            headers,
            data_offset,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseInputError {
    pub refusal: RefusalPayload,
    pub dialect: Option<Dialect>,
}

impl ParseInputError {
    fn without_dialect(refusal: RefusalPayload) -> Self {
        Self {
            refusal,
            dialect: None,
        }
    }

    pub fn into_refusal(self) -> RefusalPayload {
        self.refusal
    }
}

/// Read and parse one CSV input through header normalization (PLAN steps 3-6/7-10).
pub fn parse_input_file(
    path: &Path,
    forced_delimiter: Option<u8>,
    old_file_for_next_command: &str,
    new_file_for_next_command: &str,
) -> Result<ParsedInput, RefusalPayload> {
    parse_input_file_with_context(
        path,
        forced_delimiter,
        old_file_for_next_command,
        new_file_for_next_command,
    )
    .map_err(ParseInputError::into_refusal)
}

/// Read and parse one CSV input through header normalization while preserving
/// best-effort dialect context on parse failures.
pub fn parse_input_file_with_context(
    path: &Path,
    forced_delimiter: Option<u8>,
    old_file_for_next_command: &str,
    new_file_for_next_command: &str,
) -> Result<ParsedInput, ParseInputError> {
    let raw_bytes = read_input_bytes(path).map_err(ParseInputError::without_dialect)?;
    let file_label = path.to_string_lossy().to_string();

    let sep = detect_sep_directive(&raw_bytes);
    let (delimiter, consumed_bytes, primary_escape, allow_escape_fallback) =
        if let Some(delimiter) = forced_delimiter {
            (
                delimiter,
                sep.map_or(0, |directive| directive.consumed_bytes),
                EscapeMode::None,
                true,
            )
        } else if let Some(sep) = sep {
            (sep.delimiter, sep.consumed_bytes, EscapeMode::None, true)
        } else {
            let autodetected = detect_dialect(
                &raw_bytes,
                &file_label,
                old_file_for_next_command,
                new_file_for_next_command,
            )
            .map_err(ParseInputError::without_dialect)?;
            (autodetected.delimiter, 0usize, autodetected.escape, false)
        };

    let (headers, data_offset, escape) = parse_headers_with_escape_modes(
        path,
        &raw_bytes,
        consumed_bytes,
        delimiter,
        primary_escape,
        allow_escape_fallback,
    )?;
    let dialect = Dialect {
        delimiter,
        quote: b'"',
        escape,
    };

    Ok(ParsedInput::new(
        path.to_path_buf(),
        raw_bytes,
        dialect,
        headers,
        data_offset,
    ))
}

fn parse_headers_with_escape_modes(
    path: &Path,
    raw_bytes: &[u8],
    consumed_bytes: usize,
    delimiter: u8,
    primary_escape: EscapeMode,
    allow_escape_fallback: bool,
) -> Result<(Vec<Vec<u8>>, usize, EscapeMode), ParseInputError> {
    let mut escape_modes = vec![primary_escape];
    if allow_escape_fallback && primary_escape == EscapeMode::None {
        escape_modes.push(EscapeMode::Backslash);
    }

    for (index, escape) in escape_modes.iter().copied().enumerate() {
        let dialect = Dialect {
            delimiter,
            quote: b'"',
            escape,
        };
        match parse_headers_once(path, raw_bytes, consumed_bytes, dialect) {
            Ok((headers, data_offset)) => return Ok((headers, data_offset, escape)),
            Err(refusal)
                if index + 1 < escape_modes.len() && refusal.code == RefusalCode::ECsvParse =>
            {
                continue;
            }
            Err(refusal) => {
                return Err(ParseInputError {
                    refusal,
                    dialect: Some(dialect),
                });
            }
        }
    }

    Err(ParseInputError::without_dialect(RefusalPayload::csv_parse(
        path.to_string_lossy().into_owned(),
        1,
        "internal invariant violated: no escape mode attempted",
    )))
}

fn parse_headers_once(
    path: &Path,
    raw_bytes: &[u8],
    consumed_bytes: usize,
    dialect: Dialect,
) -> Result<(Vec<Vec<u8>>, usize), RefusalPayload> {
    let file_label = path.to_string_lossy().into_owned();
    let line_offset = line_offset_for_prefix(raw_bytes, consumed_bytes);
    let parse_bytes = raw_bytes
        .get(consumed_bytes..)
        .ok_or_else(|| RefusalPayload::headers_missing(file_label.clone()))?;

    let config = CsvReaderConfig {
        delimiter: dialect.delimiter,
        has_headers: false,
        escape: dialect.escape,
    };
    let mut reader = reader_from_bytes(parse_bytes, &config);
    let mut header_record = csv::ByteRecord::new();

    if !read_byte_record_with_line_offset(
        &mut reader,
        &mut header_record,
        &file_label,
        line_offset,
    )? {
        return Err(RefusalPayload::headers_missing(file_label));
    }

    let raw_headers = header_record
        .iter()
        .map(|field| field.to_vec())
        .collect::<Vec<_>>();
    let headers = normalize_headers_or_refusal(&file_label, &raw_headers)?;
    let data_offset = consumed_bytes + reader.position().byte() as usize;

    // Probe one record so obvious row-level parse failures are surfaced with
    // the same dialect-context path used for header parsing.
    let mut probe_record = csv::ByteRecord::new();
    let _ = read_byte_record_with_line_offset(
        &mut reader,
        &mut probe_record,
        &file_label,
        line_offset,
    )?;

    Ok((headers, data_offset))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{EncodingIssue, guard_input_bytes};
    use super::{RefusalCode, parse_input_file, parse_input_file_with_context, read_input_bytes};
    use crate::csv::dialect::Dialect;

    static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn write_temp_file(bytes: &[u8]) -> PathBuf {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        path.push(format!(
            "shape-input-{}-{nanos}-{counter}.csv",
            std::process::id()
        ));
        fs::write(&path, bytes).expect("write temp input");
        path
    }

    #[test]
    fn write_temp_file_generates_unique_paths() {
        let first = write_temp_file(b"loan_id,balance\nA1,10\n");
        let second = write_temp_file(b"loan_id,balance\nA2,20\n");

        assert_ne!(first, second, "temp helper should produce unique paths");

        let _ = fs::remove_file(&first);
        let _ = fs::remove_file(&second);
    }

    #[test]
    fn strips_utf8_bom_before_returning_bytes() {
        let raw = b"\xEF\xBB\xBFa,b\n1,2\n";
        let stripped = guard_input_bytes(Path::new("fixture.csv"), raw).expect("guard should pass");
        assert_eq!(stripped, b"a,b\n1,2\n");
    }

    #[test]
    fn detects_utf32_before_utf16() {
        let raw = [0xFF, 0xFE, 0x00, 0x00, b'a'];
        let refusal =
            guard_input_bytes(Path::new("fixture.csv"), &raw).expect_err("utf32 should refuse");
        assert_eq!(refusal.code, RefusalCode::EEncoding);
        assert_eq!(refusal.detail["issue"].as_str(), Some("utf32_le_bom"));
    }

    #[test]
    fn detects_utf16_bom() {
        let raw = [0xFE, 0xFF, b'a', b',', b'b'];
        let refusal =
            guard_input_bytes(Path::new("fixture.csv"), &raw).expect_err("utf16 should refuse");
        assert_eq!(refusal.code, RefusalCode::EEncoding);
        assert_eq!(refusal.detail["issue"].as_str(), Some("utf16_be_bom"));
        assert_eq!(refusal.message, RefusalCode::EEncoding.reason());
    }

    #[test]
    fn detects_nul_bytes_within_scan_window() {
        let mut raw = vec![b'a'; 100];
        raw[12] = 0;

        let refusal =
            guard_input_bytes(Path::new("fixture.csv"), &raw).expect_err("nul byte should refuse");
        assert_eq!(refusal.code, RefusalCode::EEncoding);
        assert_eq!(refusal.detail["issue"].as_str(), Some("nul_byte"));
        assert_eq!(
            refusal.detail["issue"].as_str(),
            Some(EncodingIssue::NulByte.as_str())
        );
    }

    #[test]
    fn ignores_nul_bytes_past_scan_window() {
        let mut raw = vec![b'a'; 8 * 1024 + 32];
        raw[8 * 1024 + 1] = 0;
        let accepted = guard_input_bytes(Path::new("fixture.csv"), &raw);
        assert!(
            accepted.is_ok(),
            "nul bytes after scan window should be ignored"
        );
    }

    #[test]
    fn read_missing_path_maps_to_e_io() {
        let missing = PathBuf::from(format!(
            "tests/fixtures/definitely-missing-{}-{}.csv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos()
        ));

        let refusal = read_input_bytes(&missing).expect_err("missing path should refuse");
        assert_eq!(refusal.code, RefusalCode::EIo);
        assert_eq!(refusal.message, RefusalCode::EIo.reason());
        assert_eq!(
            refusal.detail["file"].as_str(),
            Some(missing.to_string_lossy().as_ref())
        );
        assert!(
            refusal.detail["error"].as_str().is_some(),
            "io refusal must include the os error"
        );
    }

    #[test]
    fn parse_input_file_wrapper_preserves_pre_dialect_io_refusal_contract() {
        let missing = PathBuf::from(format!(
            "tests/fixtures/definitely-missing-wrapper-{}-{}.csv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos()
        ));

        let refusal = parse_input_file(&missing, None, "old.csv", "new.csv")
            .expect_err("wrapper should preserve missing-file refusal");

        assert_eq!(refusal.code, RefusalCode::EIo);
        assert_eq!(
            refusal.detail["file"].as_str(),
            Some(missing.to_string_lossy().as_ref())
        );
        assert!(refusal.next_command.is_none());
    }

    #[test]
    fn parse_input_file_applies_sep_directive_and_normalizes_headers() {
        let path = write_temp_file(b"sep=;\n loan_id ; amount \nA1;10\n");
        let result = parse_input_file(&path, None, "old.csv", "new.csv")
            .expect("input with sep directive and trimmed headers should parse");

        assert_eq!(result.dialect.delimiter, b';');
        assert_eq!(
            result.headers,
            vec![b"loan_id".to_vec(), b"amount".to_vec()]
        );
        assert_eq!(result.data_offset, "sep=;\n loan_id ; amount \n".len());
        assert_eq!(result.raw_bytes, b"sep=;\n loan_id ; amount \nA1;10\n");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_input_file_maps_duplicate_headers_to_e_headers() {
        let path = write_temp_file(b"loan_id,loan_id\nA1,A2\n");
        let refusal = parse_input_file(&path, None, "old.csv", "new.csv")
            .expect_err("duplicate headers should refuse");

        assert_eq!(refusal.code, RefusalCode::EHeaders);
        assert_eq!(refusal.detail["issue"].as_str(), Some("duplicate"));
        assert_eq!(refusal.detail["name"].as_str(), Some("u8:loan_id"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_input_file_wrapper_forced_delimiter_duplicate_headers_refuses_without_next_command() {
        let path = write_temp_file(b"loan_id,loan_id\nA1,A2\n");
        let refusal = parse_input_file(&path, Some(b','), "old.csv", "new.csv")
            .expect_err("duplicate headers should refuse under forced delimiter");

        assert_eq!(refusal.code, RefusalCode::EHeaders);
        assert_eq!(refusal.detail["issue"].as_str(), Some("duplicate"));
        assert_eq!(refusal.detail["name"].as_str(), Some("u8:loan_id"));
        assert!(refusal.next_command.is_none());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_input_file_wrapper_forced_delimiter_parse_failure_refuses_without_next_command() {
        let path = write_temp_file(b"loan_id,balance\nA1,10,extra\n");
        let refusal = parse_input_file(&path, Some(b','), "old.csv", "new.csv")
            .expect_err("malformed row should refuse under forced delimiter");

        assert_eq!(refusal.code, RefusalCode::ECsvParse);
        assert_eq!(
            refusal.detail["file"].as_str(),
            Some(path.to_string_lossy().as_ref())
        );
        assert_eq!(refusal.detail["line"].as_u64(), Some(2));
        assert!(
            refusal.detail["error"]
                .as_str()
                .is_some_and(|error| !error.is_empty()),
            "parse refusal should include parser error detail"
        );
        assert!(refusal.next_command.is_none());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_input_file_forced_delimiter_parse_failure_after_sep_reports_source_line() {
        let path = write_temp_file(b"sep=;\nloan_id,balance\nA1,10,extra\n");
        let refusal = parse_input_file(&path, Some(b','), "old.csv", "new.csv")
            .expect_err("malformed row should include sep/header line offsets");

        assert_eq!(refusal.code, RefusalCode::ECsvParse);
        assert_eq!(refusal.detail["line"].as_u64(), Some(3));
        assert!(refusal.next_command.is_none());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_input_file_wrapper_preserves_autodetect_dialect_refusal_contract() {
        let path = write_temp_file(b"a,b;c\n1,2;3\n");
        let refusal = parse_input_file(&path, None, "old.csv", "new.csv")
            .expect_err("ambiguous delimiter should refuse via wrapper");

        assert_eq!(refusal.code, RefusalCode::EDialect);
        assert!(
            refusal
                .next_command
                .as_deref()
                .is_some_and(|cmd| cmd.contains("--delimiter"))
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_input_file_with_context_exposes_dialect_on_header_failures() {
        let path = write_temp_file(b"loan_id,loan_id\nA1,A2\n");
        let error = parse_input_file_with_context(&path, None, "old.csv", "new.csv")
            .expect_err("duplicate headers should refuse with context");

        assert_eq!(error.refusal.code, RefusalCode::EHeaders);
        assert_eq!(error.dialect, Some(Dialect::default()));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_input_file_with_context_omits_dialect_for_pre_dialect_failures() {
        let missing = PathBuf::from(format!(
            "tests/fixtures/definitely-missing-{}-{}.csv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos()
        ));

        let error = parse_input_file_with_context(&missing, None, "old.csv", "new.csv")
            .expect_err("missing path should fail before dialect context exists");

        assert_eq!(error.refusal.code, RefusalCode::EIo);
        assert_eq!(error.dialect, None);
    }

    #[test]
    fn parse_input_file_with_context_omits_dialect_for_autodetect_refusals() {
        let path = write_temp_file(b"a,b;c\n1,2;3\n");
        let error = parse_input_file_with_context(&path, None, "old.csv", "new.csv")
            .expect_err("ambiguous delimiter should refuse before a dialect is committed");

        assert_eq!(error.refusal.code, RefusalCode::EDialect);
        assert_eq!(error.dialect, None);
        assert!(
            error
                .refusal
                .next_command
                .as_deref()
                .is_some_and(|cmd| cmd.contains("--delimiter"))
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_input_file_with_context_forced_delimiter_bypasses_autodetect_refusal() {
        let path = write_temp_file(b"a,b;c\n1,2;3\n");
        let parsed = parse_input_file_with_context(&path, Some(b','), "old.csv", "new.csv")
            .expect("forced delimiter should bypass autodetect ambiguity refusal");

        assert_eq!(parsed.dialect.delimiter, b',');
        assert_eq!(parsed.dialect.escape, crate::csv::dialect::EscapeMode::None);
        assert_eq!(parsed.headers, vec![b"a".to_vec(), b"b;c".to_vec()]);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_input_file_with_context_forced_delimiter_header_fail_keeps_dialect_context() {
        let path = write_temp_file(b"loan_id,loan_id\nA1,A2\n");
        let error = parse_input_file_with_context(&path, Some(b','), "old.csv", "new.csv")
            .expect_err("forced delimiter header failure should preserve attempted dialect");

        assert_eq!(error.refusal.code, RefusalCode::EHeaders);
        assert_eq!(
            error.dialect,
            Some(Dialect {
                delimiter: b',',
                quote: b'"',
                escape: crate::csv::dialect::EscapeMode::None,
            })
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_input_file_with_context_forced_delimiter_parse_fail_keeps_dialect_context() {
        let path = write_temp_file(b"loan_id,balance\nA1,10,extra\n");
        let error = parse_input_file_with_context(&path, Some(b','), "old.csv", "new.csv")
            .expect_err("forced delimiter parse failure should preserve attempted dialect");

        assert_eq!(error.refusal.code, RefusalCode::ECsvParse);
        assert_eq!(
            error.dialect,
            Some(Dialect {
                delimiter: b',',
                quote: b'"',
                escape: crate::csv::dialect::EscapeMode::Backslash,
            })
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_input_file_forced_delimiter_still_consumes_sep_directive() {
        let path = write_temp_file(b"sep=;\nloan_id,balance\nA1,10\n");
        let result =
            parse_input_file(&path, Some(b','), "old.csv", "new.csv").expect("should parse");

        assert_eq!(result.dialect.delimiter, b',');
        assert_eq!(
            result.headers,
            vec![b"loan_id".to_vec(), b"balance".to_vec()]
        );
        assert_eq!(result.data_offset, "sep=;\nloan_id,balance\n".len());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_input_file_with_context_forced_delimiter_still_consumes_sep_directive() {
        let path = write_temp_file(b"sep=;\nloan_id,balance\nA1,10\n");
        let result = parse_input_file_with_context(&path, Some(b','), "old.csv", "new.csv")
            .expect("context parser should consume sep directive with forced delimiter");

        assert_eq!(result.dialect.delimiter, b',');
        assert_eq!(result.dialect.escape, crate::csv::dialect::EscapeMode::None);
        assert_eq!(
            result.headers,
            vec![b"loan_id".to_vec(), b"balance".to_vec()]
        );
        assert_eq!(result.data_offset, "sep=;\nloan_id,balance\n".len());

        let _ = fs::remove_file(path);
    }
}
