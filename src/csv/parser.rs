use std::io::Cursor;

use crate::csv::dialect::EscapeMode;
use crate::refusal::payload::RefusalPayload;

pub type CsvByteReader<'a> = csv::Reader<Cursor<&'a [u8]>>;

/// Reader configuration used by header parsing and row scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvReaderConfig {
    pub delimiter: u8,
    pub has_headers: bool,
    pub escape: EscapeMode,
}

impl Default for CsvReaderConfig {
    fn default() -> Self {
        Self {
            delimiter: b',',
            has_headers: false,
            escape: EscapeMode::None,
        }
    }
}

/// Build a CSV reader over an in-memory byte slice.
///
/// Fallback between RFC4180/backslash modes is handled by dialect detection.
/// This parser consumes the selected mode as-is.
pub fn reader_from_bytes<'a>(bytes: &'a [u8], config: &CsvReaderConfig) -> CsvByteReader<'a> {
    let mut builder = csv::ReaderBuilder::new();
    builder
        .has_headers(config.has_headers)
        .delimiter(config.delimiter)
        .escape(match config.escape {
            EscapeMode::None => None,
            EscapeMode::Backslash => Some(b'\\'),
        });

    builder.from_reader(Cursor::new(bytes))
}

/// Read one byte record and map parser failures to `E_CSV_PARSE`.
pub fn read_byte_record(
    reader: &mut CsvByteReader<'_>,
    record: &mut csv::ByteRecord,
    file: &str,
) -> Result<bool, RefusalPayload> {
    read_byte_record_with_line_offset(reader, record, file, 0)
}

/// Read one byte record and map parser failures to `E_CSV_PARSE`, offsetting
/// reported line numbers by the number of logical lines consumed before this
/// reader's byte slice.
pub fn read_byte_record_with_line_offset(
    reader: &mut CsvByteReader<'_>,
    record: &mut csv::ByteRecord,
    file: &str,
    line_offset: u64,
) -> Result<bool, RefusalPayload> {
    reader
        .read_byte_record(record)
        .map_err(|error| map_csv_parse_error(file, error, line_offset))
}

/// Stream all byte records without materializing the full record set.
pub fn stream_byte_records<F>(
    bytes: &[u8],
    config: &CsvReaderConfig,
    file: &str,
    mut on_record: F,
) -> Result<u64, RefusalPayload>
where
    F: FnMut(&csv::ByteRecord) -> Result<(), RefusalPayload>,
{
    stream_byte_records_with_line_offset(bytes, config, file, 0, |record| on_record(record))
}

/// Stream all byte records without materializing the full record set.
///
/// `line_offset` is the number of logical lines consumed before `bytes`.
pub fn stream_byte_records_with_line_offset<F>(
    bytes: &[u8],
    config: &CsvReaderConfig,
    file: &str,
    line_offset: u64,
    mut on_record: F,
) -> Result<u64, RefusalPayload>
where
    F: FnMut(&csv::ByteRecord) -> Result<(), RefusalPayload>,
{
    let mut reader = reader_from_bytes(bytes, config);
    let mut record = csv::ByteRecord::new();
    let mut count = 0u64;

    while read_byte_record_with_line_offset(&mut reader, &mut record, file, line_offset)? {
        on_record(&record)?;
        count += 1;
    }

    Ok(count)
}

/// Count logical lines in a byte prefix and return a line-number offset that
/// can be added to parser-relative line numbers.
pub fn line_offset_for_prefix(bytes: &[u8], prefix_len: usize) -> u64 {
    let end = prefix_len.min(bytes.len());
    count_logical_lines(&bytes[..end])
}

fn map_csv_parse_error(file: &str, error: csv::Error, line_offset: u64) -> RefusalPayload {
    let line = error
        .position()
        .map(|pos| pos.line())
        .unwrap_or(1)
        .saturating_add(line_offset);
    RefusalPayload::csv_parse(file.to_string(), line, error.to_string())
}

fn count_logical_lines(bytes: &[u8]) -> u64 {
    let mut lines = 0u64;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                lines += 1;
                index += 1;
            }
            b'\r' => {
                lines += 1;
                index += 1;
                if bytes.get(index) == Some(&b'\n') {
                    index += 1;
                }
            }
            _ => index += 1,
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::{CsvReaderConfig, line_offset_for_prefix, stream_byte_records};
    use crate::csv::dialect::EscapeMode;
    use crate::refusal::codes::RefusalCode;

    #[test]
    fn streams_records_for_comma_and_tab_inputs() {
        let comma = b"loan_id,amount\nL1,10\nL2,20\n";
        let comma_config = CsvReaderConfig {
            delimiter: b',',
            has_headers: true,
            escape: EscapeMode::None,
        };

        let mut comma_records = Vec::new();
        let parsed = stream_byte_records(comma, &comma_config, "comma.csv", |record| {
            comma_records.push(record.iter().map(|f| f.to_vec()).collect::<Vec<_>>());
            Ok(())
        })
        .expect("comma CSV should parse");

        assert_eq!(parsed, 2);
        assert_eq!(comma_records[0], vec![b"L1".to_vec(), b"10".to_vec()]);
        assert_eq!(comma_records[1], vec![b"L2".to_vec(), b"20".to_vec()]);

        let tab = b"loan_id\tamount\nT1\t11\nT2\t22\n";
        let tab_config = CsvReaderConfig {
            delimiter: b'\t',
            has_headers: true,
            escape: EscapeMode::None,
        };

        let mut tab_count = 0u64;
        let parsed = stream_byte_records(tab, &tab_config, "tab.csv", |_| {
            tab_count += 1;
            Ok(())
        })
        .expect("tab CSV should parse");

        assert_eq!(parsed, 2);
        assert_eq!(tab_count, 2);
    }

    #[test]
    fn malformed_csv_maps_to_e_csv_parse_with_line_detail() {
        let malformed = b"loan_id,amount\n1,10,extra\n2,20\n";
        let config = CsvReaderConfig {
            delimiter: b',',
            has_headers: false,
            escape: EscapeMode::None,
        };

        let refusal = stream_byte_records(malformed, &config, "bad.csv", |_| Ok(()))
            .expect_err("malformed CSV should refuse");

        assert_eq!(refusal.code, RefusalCode::ECsvParse);
        assert_eq!(refusal.detail["file"].as_str(), Some("bad.csv"));
        assert!(
            refusal.detail["line"]
                .as_u64()
                .is_some_and(|line| line == 2),
            "line metadata should reflect source line"
        );
        assert!(
            refusal.detail["error"]
                .as_str()
                .is_some_and(|message| !message.is_empty()),
            "error detail should be non-empty"
        );
    }

    #[test]
    fn large_input_smoke_test_streams_without_collecting_all_rows() {
        let mut bytes = String::from("loan_id,amount\n");
        for i in 0..10_000u64 {
            bytes.push_str(&format!("L{i},{}\n", i * 10));
        }

        let config = CsvReaderConfig {
            delimiter: b',',
            has_headers: true,
            escape: EscapeMode::None,
        };

        let mut seen = 0u64;
        let parsed = stream_byte_records(bytes.as_bytes(), &config, "large.csv", |_| {
            seen += 1;
            Ok(())
        })
        .expect("large CSV should stream parse");

        assert_eq!(parsed, 10_000);
        assert_eq!(seen, 10_000);
    }

    #[test]
    fn line_offset_for_prefix_counts_lf_and_crlf_lines() {
        let bytes = b"sep=;\r\nloan_id,balance\nA1,100\n";
        assert_eq!(line_offset_for_prefix(bytes, 0), 0);
        assert_eq!(line_offset_for_prefix(bytes, "sep=;\r\n".len()), 1);
        assert_eq!(
            line_offset_for_prefix(bytes, "sep=;\r\nloan_id,balance\n".len()),
            2
        );
    }
}
