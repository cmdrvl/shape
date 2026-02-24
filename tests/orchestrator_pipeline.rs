use std::path::{Path, PathBuf};

use shape::csv::dialect::Dialect;
use shape::csv::input::ParsedInput;
use shape::orchestrator::{enforce_post_scan_empty_guards, enforce_pre_scan_empty_guards};
use shape::scan::{ColumnClassification, ScanResult};

const OLD_PATH: &str = "old.csv";
const NEW_PATH: &str = "new.csv";
const KEY_HEADER_BYTES: &[u8] = b"loan_id\n";
const KEY_HEADER_OFFSET: usize = KEY_HEADER_BYTES.len();
const KEY_COLUMN: &[u8] = b"loan_id";

struct PreScanRefusalCase {
    old_raw_bytes: &'static [u8],
    new_raw_bytes: &'static [u8],
    expected_file: &'static str,
    context: &'static str,
}

struct PreScanAcceptCase {
    old_raw_bytes: &'static [u8],
    new_raw_bytes: &'static [u8],
    context: &'static str,
}

struct PostScanRefusalCase {
    old_rows: u64,
    new_rows: u64,
    expected_file: &'static str,
    context: &'static str,
}

struct PostScanAcceptCase {
    old_rows: u64,
    new_rows: u64,
    context: &'static str,
}

fn parsed_input(path: &str, raw_bytes: &[u8], data_offset: usize) -> ParsedInput {
    ParsedInput::new(
        PathBuf::from(path),
        raw_bytes.to_vec(),
        Dialect::default(),
        vec![KEY_COLUMN.to_vec()],
        data_offset,
    )
}

fn scan_result(row_count: u64) -> ScanResult {
    ScanResult {
        row_count,
        key_scan: None,
        column_types: vec![ColumnClassification::AllMissing],
    }
}

fn assert_empty_refusal(code: &str, file: Option<&str>, expected_file: &str) {
    assert_eq!(code, "E_EMPTY");
    assert_eq!(file, Some(expected_file));
}

fn assert_pre_scan_empty_refusal(
    old_raw_bytes: &[u8],
    new_raw_bytes: &[u8],
    expected_file: &str,
    context: &str,
) {
    let old = parsed_input(OLD_PATH, old_raw_bytes, KEY_HEADER_OFFSET);
    let new = parsed_input(NEW_PATH, new_raw_bytes, KEY_HEADER_OFFSET);

    let refusal = enforce_pre_scan_empty_guards(&old, &new).expect_err(context);
    assert_empty_refusal(
        refusal.code.as_str(),
        refusal.detail["file"].as_str(),
        expected_file,
    );
}

fn assert_post_scan_empty_refusal(
    old_rows: u64,
    new_rows: u64,
    expected_file: &str,
    context: &str,
) {
    let old = scan_result(old_rows);
    let new = scan_result(new_rows);

    let refusal =
        enforce_post_scan_empty_guards(Path::new(OLD_PATH), Path::new(NEW_PATH), &old, &new)
            .expect_err(context);
    assert_empty_refusal(
        refusal.code.as_str(),
        refusal.detail["file"].as_str(),
        expected_file,
    );
}

fn assert_pre_scan_empty_accept(old_raw_bytes: &[u8], new_raw_bytes: &[u8], context: &str) {
    let old = parsed_input(OLD_PATH, old_raw_bytes, KEY_HEADER_OFFSET);
    let new = parsed_input(NEW_PATH, new_raw_bytes, KEY_HEADER_OFFSET);

    enforce_pre_scan_empty_guards(&old, &new).expect(context);
}

fn assert_post_scan_empty_accept(old_rows: u64, new_rows: u64, context: &str) {
    let old = scan_result(old_rows);
    let new = scan_result(new_rows);

    enforce_post_scan_empty_guards(Path::new(OLD_PATH), Path::new(NEW_PATH), &old, &new)
        .expect(context);
}

#[test]
fn pre_scan_empty_guards_matrix() {
    let refusal_cases = [
        PreScanRefusalCase {
            old_raw_bytes: KEY_HEADER_BYTES,
            new_raw_bytes: KEY_HEADER_BYTES,
            expected_file: OLD_PATH,
            context: "old should fail first when both files are header-only",
        },
        PreScanRefusalCase {
            old_raw_bytes: b"loan_id\nA1\n",
            new_raw_bytes: KEY_HEADER_BYTES,
            expected_file: NEW_PATH,
            context: "new should fail when old has data and new is header-only",
        },
    ];
    for case in refusal_cases {
        assert_pre_scan_empty_refusal(
            case.old_raw_bytes,
            case.new_raw_bytes,
            case.expected_file,
            case.context,
        );
    }

    let accept_cases = [PreScanAcceptCase {
        old_raw_bytes: b"loan_id\nA1\n",
        new_raw_bytes: b"loan_id\nA2\n",
        context: "both files have bytes after header and should pass step-11 guards",
    }];
    for case in accept_cases {
        assert_pre_scan_empty_accept(case.old_raw_bytes, case.new_raw_bytes, case.context);
    }
}

#[test]
fn post_scan_empty_guards_matrix() {
    let refusal_cases = [
        PostScanRefusalCase {
            old_rows: 0,
            new_rows: 0,
            expected_file: OLD_PATH,
            context: "old should fail first when both scans are all-blank",
        },
        PostScanRefusalCase {
            old_rows: 2,
            new_rows: 0,
            expected_file: NEW_PATH,
            context: "new should fail after old passes when new has no non-blank rows",
        },
    ];
    for case in refusal_cases {
        assert_post_scan_empty_refusal(
            case.old_rows,
            case.new_rows,
            case.expected_file,
            case.context,
        );
    }

    let accept_cases = [PostScanAcceptCase {
        old_rows: 3,
        new_rows: 4,
        context: "both scans have data rows and should pass step-16 guards",
    }];
    for case in accept_cases {
        assert_post_scan_empty_accept(case.old_rows, case.new_rows, case.context);
    }
}
