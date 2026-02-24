use std::collections::HashSet;
use std::path::Path;

use csv::ByteRecord;

use crate::csv::dialect::Dialect;
use crate::csv::parser::{
    CsvReaderConfig, line_offset_for_prefix, stream_byte_records_with_line_offset,
};
use crate::normalize::headers::ascii_trim;
use crate::refusal::payload::RefusalPayload;

/// Result of scanning one file's data rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanResult {
    pub row_count: u64,
    pub key_scan: Option<KeyScan>,
    pub column_types: Vec<ColumnClassification>,
}

/// Key-tracking details captured during a single-pass scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyScan {
    pub values: HashSet<Vec<u8>>,
    pub duplicate_count: u64,
    pub empty_count: u64,
}

impl KeyScan {
    pub fn new() -> Self {
        Self {
            values: HashSet::new(),
            duplicate_count: 0,
            empty_count: 0,
        }
    }
}

impl Default for KeyScan {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnClassification {
    Numeric,
    NonNumeric,
    AllMissing,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NumericClassifier {
    seen_non_missing: bool,
    seen_non_numeric: bool,
}

impl NumericClassifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, value: &[u8]) {
        if is_missing(value) {
            return;
        }
        self.seen_non_missing = true;
        if self.seen_non_numeric {
            return;
        }
        if !parses_as_numeric(value) {
            self.seen_non_numeric = true;
        }
    }

    pub fn classify(self) -> ColumnClassification {
        if !self.seen_non_missing {
            ColumnClassification::AllMissing
        } else if self.seen_non_numeric {
            ColumnClassification::NonNumeric
        } else {
            ColumnClassification::Numeric
        }
    }
}

/// Scan one file's rows exactly once, collecting row/key/type metrics.
pub fn scan_file(
    file: &Path,
    raw_bytes: &[u8],
    data_offset: usize,
    dialect: &Dialect,
    common_column_indices: &[usize],
    key_column_index: Option<usize>,
) -> Result<ScanResult, RefusalPayload> {
    pre_scan_empty_guard(file, raw_bytes, data_offset)?;

    let data = raw_bytes
        .get(data_offset..)
        .ok_or_else(|| RefusalPayload::empty(file.to_string_lossy().to_string(), 0))?;
    let config = CsvReaderConfig {
        delimiter: dialect.delimiter,
        has_headers: false,
        escape: dialect.escape,
    };
    let file_label = file.to_string_lossy().to_string();
    let line_offset = line_offset_for_prefix(raw_bytes, data_offset);

    let mut row_count = 0u64;
    let mut key_scan = key_column_index.map(|_| KeyScan::new());
    let mut classifiers = vec![NumericClassifier::new(); common_column_indices.len()];

    stream_byte_records_with_line_offset(data, &config, &file_label, line_offset, |record| {
        if is_blank_record(record) {
            return Ok(());
        }

        row_count += 1;

        if let (Some(key_idx), Some(ks)) = (key_column_index, &mut key_scan) {
            let key = ascii_trim(record.get(key_idx).unwrap_or(&[]));
            if is_missing(key) {
                ks.empty_count += 1;
            } else if !ks.values.insert(key.to_vec()) {
                ks.duplicate_count += 1;
            }
        }

        for (classifier, &column_index) in classifiers.iter_mut().zip(common_column_indices) {
            let value = ascii_trim(record.get(column_index).unwrap_or(&[]));
            classifier.observe(value);
        }

        Ok(())
    })?;

    post_scan_empty_guard(file, row_count)?;

    Ok(ScanResult {
        row_count,
        key_scan,
        column_types: classifiers
            .into_iter()
            .map(NumericClassifier::classify)
            .collect(),
    })
}

/// Returns true when every field in the record is blank after ASCII-trim.
pub fn is_blank_record(record: &ByteRecord) -> bool {
    record.iter().all(|field| ascii_trim(field).is_empty())
}

/// Step 11 quick emptiness probe: true if bytes exist after the parsed header row.
pub fn has_data_bytes_after_header(raw_bytes: &[u8], data_offset: usize) -> bool {
    raw_bytes
        .get(data_offset..)
        .is_some_and(|remaining| !remaining.is_empty())
}

/// Apply the step-11 quick `E_EMPTY` check for one parsed input.
pub fn pre_scan_empty_guard(
    file: &Path,
    raw_bytes: &[u8],
    data_offset: usize,
) -> Result<(), RefusalPayload> {
    if has_data_bytes_after_header(raw_bytes, data_offset) {
        Ok(())
    } else {
        Err(RefusalPayload::empty(file.to_string_lossy().to_string(), 0))
    }
}

/// Apply the step-16 post-scan `E_EMPTY` check (all-blank datasets).
pub fn post_scan_empty_guard(file: &Path, row_count: u64) -> Result<(), RefusalPayload> {
    if row_count == 0 {
        Err(RefusalPayload::empty(file.to_string_lossy().to_string(), 0))
    } else {
        Ok(())
    }
}

/// Returns true if the value is one of shape's missing-value tokens.
pub fn is_missing(value: &[u8]) -> bool {
    let trimmed = ascii_trim(value);
    if trimmed.is_empty() || trimmed == b"-" {
        return true;
    }

    eq_ascii_case_insensitive(trimmed, b"NA")
        || eq_ascii_case_insensitive(trimmed, b"N/A")
        || eq_ascii_case_insensitive(trimmed, b"NULL")
        || eq_ascii_case_insensitive(trimmed, b"NAN")
        || eq_ascii_case_insensitive(trimmed, b"NONE")
}

/// Returns true when a value matches shape's numeric grammar.
pub fn parses_as_numeric(value: &[u8]) -> bool {
    let trimmed = ascii_trim(value);
    if trimmed.is_empty() || is_missing(trimmed) {
        return false;
    }

    let mut s = match std::str::from_utf8(trimmed) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let mut accounting = false;
    if let Some(inner) = s.strip_prefix('(').and_then(|v| v.strip_suffix(')')) {
        accounting = true;
        s = inner.trim_matches([' ', '\t']);
        if s.is_empty() || s.contains('(') || s.contains(')') {
            return false;
        }
    } else if s.contains('(') || s.contains(')') {
        return false;
    }

    let (after_sign, sign_seen) = strip_leading_sign(s);
    s = after_sign;

    let mut currency_seen = false;
    if let Some(rest) = s.strip_prefix('$') {
        currency_seen = true;
        s = rest;
    }

    let (after_second_sign, second_sign_seen) = strip_leading_sign(s);
    s = after_second_sign;

    if sign_seen && second_sign_seen {
        return false;
    }
    if accounting && (sign_seen || second_sign_seen) {
        return false;
    }
    if s.is_empty() || s.contains('$') {
        return false;
    }

    let (mantissa, exponent) = match split_exponent(s) {
        Some(parts) => parts,
        None => return false,
    };
    if !valid_mantissa(mantissa) {
        return false;
    }
    if let Some(exp) = exponent
        && !valid_signed_integer(exp)
    {
        return false;
    }

    let mut normalized = mantissa.replace(',', "");
    if let Some(exp) = exponent {
        normalized.push('e');
        normalized.push_str(exp);
    }

    if currency_seen && normalized.starts_with('.') {
        // "$.50" is accepted by f64 parsing but we keep currency forms strict.
        return false;
    }

    normalized
        .parse::<f64>()
        .map(|v| v.is_finite())
        .unwrap_or(false)
}

fn eq_ascii_case_insensitive(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(l, r)| l.eq_ignore_ascii_case(r))
}

fn strip_leading_sign(s: &str) -> (&str, bool) {
    if let Some(rest) = s.strip_prefix('+') {
        (rest, true)
    } else if let Some(rest) = s.strip_prefix('-') {
        (rest, true)
    } else {
        (s, false)
    }
}

fn split_exponent(s: &str) -> Option<(&str, Option<&str>)> {
    let mut exp_index = None;
    for (idx, ch) in s.char_indices() {
        if ch == 'e' || ch == 'E' {
            if exp_index.is_some() {
                return None;
            }
            exp_index = Some(idx);
        }
    }

    match exp_index {
        Some(idx) => {
            let mantissa = &s[..idx];
            let exponent = &s[idx + 1..];
            Some((mantissa, Some(exponent)))
        }
        None => Some((s, None)),
    }
}

fn valid_mantissa(mantissa: &str) -> bool {
    if mantissa.is_empty() {
        return false;
    }

    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((left, right)) => {
            if right.contains('.') {
                return false;
            }
            (left, Some(right))
        }
        None => (mantissa, None),
    };

    if int_part.is_empty() && frac_part.is_none_or(str::is_empty) {
        return false;
    }

    if int_part.contains(',') {
        if int_part.starts_with(',') || int_part.ends_with(',') {
            return false;
        }
        let mut groups = int_part.split(',');
        let first = match groups.next() {
            Some(group) => group,
            None => return false,
        };
        if first.is_empty() || first.len() > 3 || !first.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        if !groups.all(|group| group.len() == 3 && group.chars().all(|c| c.is_ascii_digit())) {
            return false;
        }
    } else if !int_part.is_empty() && !int_part.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    if let Some(frac) = frac_part
        && !frac.is_empty()
        && !frac.chars().all(|c| c.is_ascii_digit())
    {
        return false;
    }

    let integer_digits = int_part.chars().filter(|c| c.is_ascii_digit()).count();
    let fractional_digits = frac_part.unwrap_or_default().len();
    integer_digits + fractional_digits > 0
}

fn valid_signed_integer(value: &str) -> bool {
    let (digits, _) = strip_leading_sign(value);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use csv::ByteRecord;

    use super::{
        ColumnClassification, Dialect, has_data_bytes_after_header, is_blank_record, is_missing,
        parses_as_numeric, post_scan_empty_guard, pre_scan_empty_guard, scan_file,
    };

    #[test]
    fn missing_tokens_are_case_insensitive() {
        for token in ["", " ", "-", "NA", "n/a", "null", "NAN", "none"] {
            assert!(
                is_missing(token.as_bytes()),
                "expected missing token: {token:?}"
            );
        }
        assert!(!is_missing(b"value"));
    }

    #[test]
    fn parses_supported_numeric_forms() {
        for value in [
            "123",
            "-123.45",
            "1e6",
            "$1,234.56",
            "-$1,234.56",
            "(123.45)",
            "1,234",
            "1,234,567.89",
            ".5",
            "123.",
        ] {
            assert!(
                parses_as_numeric(value.as_bytes()),
                "expected numeric: {value}"
            );
        }
    }

    #[test]
    fn rejects_malformed_numeric_forms() {
        for value in [
            "12-3", "1,23", "1,23,456", "1,234,56", "abc", "1e", "1e-", "1..2", "$.50", "($123)-",
        ] {
            assert!(
                !parses_as_numeric(value.as_bytes()),
                "expected non-numeric: {value}"
            );
        }
    }

    #[test]
    fn missing_values_are_not_numeric() {
        for value in ["", "-", "NA", "null", "NONE"] {
            assert!(
                !parses_as_numeric(value.as_bytes()),
                "missing value should not classify as numeric: {value}"
            );
        }
    }

    #[test]
    fn blank_record_detection_uses_ascii_trimmed_fields() {
        let blank = ByteRecord::from(vec!["", "   ", "\t\t", " \t "]);
        let non_blank = ByteRecord::from(vec!["", "0", "\t"]);

        assert!(is_blank_record(&blank));
        assert!(!is_blank_record(&non_blank));
    }

    #[test]
    fn pre_scan_empty_guard_fires_for_header_only_file() {
        let file = Path::new("header-only.csv");
        let raw = b"loan_id,balance\n";
        let data_offset = raw.len();

        let refusal =
            pre_scan_empty_guard(file, raw, data_offset).expect_err("header-only should refuse");
        assert_eq!(refusal.code.as_str(), "E_EMPTY");
        assert_eq!(refusal.detail["file"].as_str(), Some("header-only.csv"));
        assert_eq!(refusal.detail["rows"].as_u64(), Some(0));
    }

    #[test]
    fn pre_scan_empty_guard_allows_non_empty_tail_for_post_scan_check() {
        let file = Path::new("all-blank.csv");
        let raw = b"loan_id,balance\n,\n,\n";
        let data_offset = "loan_id,balance\n".len();

        assert!(
            has_data_bytes_after_header(raw, data_offset),
            "all-blank rows should pass quick pre-scan byte probe"
        );
        assert!(pre_scan_empty_guard(file, raw, data_offset).is_ok());
    }

    #[test]
    fn post_scan_empty_guard_refuses_zero_rows_and_accepts_non_zero() {
        let file = Path::new("scan.csv");

        let refusal = post_scan_empty_guard(file, 0).expect_err("zero rows should refuse");
        assert_eq!(refusal.code.as_str(), "E_EMPTY");
        assert_eq!(refusal.detail["file"].as_str(), Some("scan.csv"));
        assert_eq!(refusal.detail["rows"].as_u64(), Some(0));

        assert!(post_scan_empty_guard(file, 1).is_ok());
    }

    #[test]
    fn scan_file_tracks_rows_keys_and_column_types_in_one_pass() {
        let bytes = b"loan_id,amount,flag\nA1,10,yes\nA2,20,no\nA2,30,no\n,40,no\n";
        let file = Path::new("rows.csv");
        let result = scan_file(
            file,
            bytes,
            "loan_id,amount,flag\n".len(),
            &Dialect::default(),
            &[1, 2],
            Some(0),
        )
        .expect("scan should succeed");

        assert_eq!(result.row_count, 4);
        assert_eq!(
            result.column_types,
            vec![
                ColumnClassification::Numeric,
                ColumnClassification::NonNumeric
            ]
        );
        let key_scan = result.key_scan.expect("key scan should be present");
        assert_eq!(key_scan.values.len(), 2);
        assert_eq!(key_scan.duplicate_count, 1);
        assert_eq!(key_scan.empty_count, 1);
    }

    #[test]
    fn scan_file_classifies_all_missing_columns() {
        let bytes = b"loan_id,optional\nA1,\nA2, \n";
        let file = Path::new("all-missing.csv");
        let result = scan_file(
            file,
            bytes,
            "loan_id,optional\n".len(),
            &Dialect::default(),
            &[1],
            Some(0),
        )
        .expect("scan should succeed");

        assert_eq!(result.row_count, 2);
        assert_eq!(result.column_types, vec![ColumnClassification::AllMissing]);
        let key_scan = result.key_scan.expect("key scan should be present");
        assert_eq!(key_scan.values.len(), 2);
        assert_eq!(key_scan.duplicate_count, 0);
        assert_eq!(key_scan.empty_count, 0);
    }

    #[test]
    fn scan_file_refuses_header_only_and_all_blank_data() {
        let header_only = b"loan_id,amount\n";
        let file = Path::new("header-only.csv");
        let refusal = scan_file(
            file,
            header_only,
            header_only.len(),
            &Dialect::default(),
            &[1],
            Some(0),
        )
        .expect_err("header-only should refuse");
        assert_eq!(refusal.code.as_str(), "E_EMPTY");

        let all_blank = b"loan_id,amount\n,\n \t,\t \n";
        let refusal = scan_file(
            Path::new("all-blank.csv"),
            all_blank,
            "loan_id,amount\n".len(),
            &Dialect::default(),
            &[1],
            Some(0),
        )
        .expect_err("all-blank should refuse");
        assert_eq!(refusal.code.as_str(), "E_EMPTY");
    }
}
