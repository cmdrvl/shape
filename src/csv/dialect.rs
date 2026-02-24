use std::collections::BTreeMap;

use crate::refusal::payload::RefusalPayload;

const CANDIDATE_DELIMITERS: [u8; 5] = [b',', b'\t', b';', b'|', b'^'];
const MAX_SAMPLE_RECORDS: usize = 200;
const MAX_SAMPLE_BYTES: usize = 64 * 1024;

/// CSV escape handling mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeMode {
    /// RFC4180 doubled-quote escaping.
    None,
    /// Backslash escaping before quote.
    Backslash,
}

/// Effective dialect used when parsing one CSV file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dialect {
    pub delimiter: u8,
    pub quote: u8,
    pub escape: EscapeMode,
}

impl Default for Dialect {
    fn default() -> Self {
        Self {
            delimiter: b',',
            quote: b'"',
            escape: EscapeMode::None,
        }
    }
}

impl Dialect {
    pub fn delimiter_display(self) -> String {
        match self.delimiter {
            b'\t' => "\\t".to_owned(),
            byte if byte.is_ascii_graphic() || byte == b' ' => (byte as char).to_string(),
            byte => format!("0x{byte:02x}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CandidateScore {
    records_parsed: u64,
    mode_count: u64,
    mode_fields: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CandidateProbe {
    delimiter: u8,
    escape: EscapeMode,
    score: CandidateScore,
    sample_signature: Vec<Vec<u8>>,
}

/// Auto-detect CSV dialect with PLAN scoring and refusal behavior.
pub fn detect_dialect(
    bytes: &[u8],
    detail_file: &str,
    old_file: &str,
    new_file: &str,
) -> Result<Dialect, RefusalPayload> {
    if bytes.is_empty() {
        return Err(RefusalPayload::headers_missing(detail_file.to_string()));
    }

    let sample_len = bytes.len().min(MAX_SAMPLE_BYTES);
    let sample = &bytes[..sample_len];
    match detect_dialect_from_sample(sample, detail_file, old_file, new_file) {
        Ok(dialect) => Ok(dialect),
        Err(_) if sample_len < bytes.len() => {
            detect_dialect_from_sample(bytes, detail_file, old_file, new_file)
        }
        Err(refusal) => Err(refusal),
    }
}

fn detect_dialect_from_sample(
    sample: &[u8],
    detail_file: &str,
    old_file: &str,
    new_file: &str,
) -> Result<Dialect, RefusalPayload> {
    let probes: Vec<CandidateProbe> = CANDIDATE_DELIMITERS
        .iter()
        .copied()
        .filter_map(|delimiter| probe_candidate(sample, delimiter))
        .collect();

    if probes.is_empty() {
        return Err(dialect_refusal(
            detail_file,
            old_file,
            new_file,
            CANDIDATE_DELIMITERS.iter().copied().map(hex_byte).collect(),
            Some(DelimiterHint::CommaPriority),
        ));
    }

    let best_score = match probes.iter().map(|candidate| candidate.score).max() {
        Some(score) => score,
        None => {
            return Err(dialect_refusal(
                detail_file,
                old_file,
                new_file,
                CANDIDATE_DELIMITERS.iter().copied().map(hex_byte).collect(),
                Some(DelimiterHint::CommaPriority),
            ));
        }
    };

    let tied_best: Vec<&CandidateProbe> = probes
        .iter()
        .filter(|candidate| candidate.score == best_score)
        .collect();

    let Some(winner) = tied_best.first().copied() else {
        return Err(dialect_refusal(
            detail_file,
            old_file,
            new_file,
            CANDIDATE_DELIMITERS.iter().copied().map(hex_byte).collect(),
            Some(DelimiterHint::CommaPriority),
        ));
    };

    if tied_best.len() > 1 {
        let first_signature = &winner.sample_signature;
        let same_sample = tied_best
            .iter()
            .all(|candidate| candidate.sample_signature == *first_signature);
        if !same_sample {
            let candidates = tied_best
                .iter()
                .map(|candidate| hex_byte(candidate.delimiter));
            return Err(dialect_refusal(
                detail_file,
                old_file,
                new_file,
                candidates.collect(),
                Some(DelimiterHint::Byte(winner.delimiter)),
            ));
        }
    }

    if winner.score.mode_fields <= 1 {
        return Err(dialect_refusal(
            detail_file,
            old_file,
            new_file,
            vec![hex_byte(winner.delimiter)],
            Some(DelimiterHint::Byte(winner.delimiter)),
        ));
    }

    Ok(Dialect {
        delimiter: winner.delimiter,
        quote: b'"',
        escape: winner.escape,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DelimiterHint {
    Byte(u8),
    CommaPriority,
}

fn dialect_refusal(
    detail_file: &str,
    old_file: &str,
    new_file: &str,
    candidates: Vec<String>,
    hint: Option<DelimiterHint>,
) -> RefusalPayload {
    let next_command = hint.map(|hint| {
        let delimiter = match hint {
            DelimiterHint::Byte(byte) => delimiter_flag_value(byte),
            DelimiterHint::CommaPriority => "comma".to_string(),
        };
        RefusalPayload::build_next_command_for_dialect(old_file, new_file, &delimiter)
    });
    RefusalPayload::dialect(detail_file.to_string(), candidates, next_command)
}

fn probe_candidate(sample: &[u8], delimiter: u8) -> Option<CandidateProbe> {
    probe_with_escape(sample, delimiter, EscapeMode::None)
        .or_else(|| probe_with_escape(sample, delimiter, EscapeMode::Backslash))
}

fn probe_with_escape(sample: &[u8], delimiter: u8, escape: EscapeMode) -> Option<CandidateProbe> {
    let mut builder = csv::ReaderBuilder::new();
    builder
        .has_headers(false)
        .delimiter(delimiter)
        .escape(match escape {
            EscapeMode::None => None,
            EscapeMode::Backslash => Some(b'\\'),
        });

    let mut reader = builder.from_reader(sample);
    let mut histogram: BTreeMap<u64, u64> = BTreeMap::new();
    let mut parsed_records = 0u64;
    let mut sample_signature: Vec<Vec<u8>> = Vec::new();

    for record in reader.byte_records().take(MAX_SAMPLE_RECORDS) {
        let record = match record {
            Ok(record) => record,
            Err(_) => return None,
        };
        parsed_records += 1;
        let field_count = record.len() as u64;
        *histogram.entry(field_count).or_insert(0) += 1;
        sample_signature.push(record_signature(&record));
    }

    if parsed_records == 0 {
        return None;
    }

    let (mode_fields, mode_count) = histogram
        .into_iter()
        .max_by_key(|(fields, count)| (*count, *fields))?;

    Some(CandidateProbe {
        delimiter,
        escape,
        score: CandidateScore {
            records_parsed: parsed_records,
            mode_count,
            mode_fields,
        },
        sample_signature,
    })
}

fn record_signature(record: &csv::ByteRecord) -> Vec<u8> {
    let mut signature = Vec::new();
    for (index, field) in record.iter().enumerate() {
        if index > 0 {
            signature.push(0x1f);
        }
        signature.extend_from_slice(field);
    }
    signature
}

fn hex_byte(byte: u8) -> String {
    format!("0x{byte:02x}")
}

fn delimiter_flag_value(byte: u8) -> String {
    match byte {
        b',' => "comma".to_string(),
        b'\t' => "tab".to_string(),
        b';' => "semicolon".to_string(),
        b'|' => "pipe".to_string(),
        b'^' => "caret".to_string(),
        _ => hex_byte(byte),
    }
}

#[cfg(test)]
mod tests {
    use super::{Dialect, EscapeMode, detect_dialect};
    use crate::refusal::codes::RefusalCode;

    #[test]
    fn empty_input_maps_to_e_headers_instead_of_dialect_refusal() {
        let refusal = detect_dialect(b"", "old.csv", "old.csv", "new.csv")
            .expect_err("empty input should refuse as missing headers");

        assert_eq!(refusal.code, RefusalCode::EHeaders);
        assert_eq!(refusal.detail["file"].as_str(), Some("old.csv"));
        assert_eq!(refusal.detail["issue"].as_str(), Some("missing"));
        assert!(refusal.next_command.is_none());
    }

    #[test]
    fn detects_each_supported_candidate_delimiter() {
        let fixtures = [
            (b"loan_id,amount\n1,10\n2,20\n".as_slice(), b','),
            (b"loan_id\tamount\n1\t10\n2\t20\n".as_slice(), b'\t'),
            (b"loan_id;amount\n1;10\n2;20\n".as_slice(), b';'),
            (b"loan_id|amount\n1|10\n2|20\n".as_slice(), b'|'),
            (b"loan_id^amount\n1^10\n2^20\n".as_slice(), b'^'),
        ];

        for (bytes, expected_delimiter) in fixtures {
            let detected = detect_dialect(bytes, "old.csv", "old.csv", "new.csv")
                .expect("dialect should be detected");
            assert_eq!(
                detected,
                Dialect {
                    delimiter: expected_delimiter,
                    quote: b'"',
                    escape: EscapeMode::None,
                }
            );
        }
    }

    #[test]
    fn delimiter_display_handles_named_and_arbitrary_supported_bytes() {
        assert_eq!(
            Dialect {
                delimiter: b',',
                quote: b'"',
                escape: EscapeMode::None,
            }
            .delimiter_display(),
            ","
        );
        assert_eq!(
            Dialect {
                delimiter: b'\t',
                quote: b'"',
                escape: EscapeMode::None,
            }
            .delimiter_display(),
            "\\t"
        );
        assert_eq!(
            Dialect {
                delimiter: b'=',
                quote: b'"',
                escape: EscapeMode::None,
            }
            .delimiter_display(),
            "="
        );
        assert_eq!(
            Dialect {
                delimiter: 0x01,
                quote: b'"',
                escape: EscapeMode::None,
            }
            .delimiter_display(),
            "0x01"
        );
    }

    #[test]
    fn refuses_when_best_scores_tie_with_different_samples() {
        let bytes = b"a,b;c\n1,2;3\n";
        let refusal =
            detect_dialect(bytes, "old.csv", "old.csv", "new.csv").expect_err("should refuse");
        assert_eq!(refusal.code, RefusalCode::EDialect);
        assert_eq!(refusal.detail["file"].as_str(), Some("old.csv"));
        let candidates = refusal.detail["candidates"]
            .as_array()
            .expect("candidates should be an array");
        let observed: Vec<&str> = candidates
            .iter()
            .map(|entry| entry.as_str().expect("candidate must be string"))
            .collect();
        assert_eq!(observed, vec!["0x2c", "0x3b"]);
        assert!(
            refusal
                .next_command
                .as_deref()
                .is_some_and(|cmd| cmd.contains("--delimiter comma"))
        );
    }

    #[test]
    fn refuses_when_winner_has_one_column_only() {
        let bytes = b"header_only\nvalue\n";
        let refusal =
            detect_dialect(bytes, "old.csv", "old.csv", "new.csv").expect_err("should refuse");
        assert_eq!(refusal.code, RefusalCode::EDialect);
        let candidates = refusal.detail["candidates"]
            .as_array()
            .expect("candidates should be an array");
        let observed: Vec<&str> = candidates
            .iter()
            .map(|entry| entry.as_str().expect("candidate must be string"))
            .collect();
        assert_eq!(observed, vec!["0x2c"]);
        assert!(
            refusal
                .next_command
                .as_deref()
                .is_some_and(|cmd| cmd.contains("--delimiter comma"))
        );
    }

    #[test]
    fn retries_with_full_bytes_when_truncated_sample_refuses_valid_large_csv() {
        let long = "x".repeat(super::MAX_SAMPLE_BYTES + 1_024);
        let bytes = format!("loan_id,balance\n\"{long}\",123\nA2,456\n");

        let detected = detect_dialect(bytes.as_bytes(), "old.csv", "old.csv", "new.csv")
            .expect("full-byte fallback should detect comma delimiter");

        assert_eq!(detected.delimiter, b',');
        assert_eq!(detected.escape, EscapeMode::None);
    }
}
