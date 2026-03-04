mod helpers;

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;
use std::fs;
use std::panic::panic_any;

use helpers::{ShapeInvocation, run_shape, run_shape_with_fixtures};
use serde_json::Value;
use shape::checks::key_viability::evaluate_key_viability;
use shape::checks::row_granularity::{compute_key_overlap_metrics, evaluate_row_granularity};
use shape::checks::schema_overlap::evaluate_schema_overlap;
use shape::checks::suite::{CheckStatus, CheckSuite, Outcome, build_reasons};
use shape::checks::type_consistency::TypeConsistencyResult;
use shape::csv::dialect::{Dialect, EscapeMode};
use shape::csv::parser::{CsvReaderConfig, reader_from_bytes};
use shape::output::human::{
    RefusalRenderContext, render_compatible, render_incompatible, render_refusal_with_context,
};
use shape::refusal::codes::RefusalCode;
use shape::refusal::payload::RefusalPayload;
use shape::scan::KeyScan;

fn assert_json_mode_stderr_empty(result: &ShapeInvocation, context: &str) {
    let stderr = result
        .stderr
        .lines()
        .filter(|line| !line.starts_with("shape: note:"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(stderr.trim().is_empty(), "{context}: {}", result.stderr);
}

fn parse_json_stdout(result: &ShapeInvocation, context: &str) -> Value {
    serde_json::from_str(result.stdout_trimmed()).expect(context)
}

const BASIC_OLD: &str = "basic_old.csv";
const BASIC_NEW: &str = "basic_new.csv";
const SCHEMA_DRIFT_OLD: &str = "schema_drift_old.csv";
const SCHEMA_DRIFT_NEW: &str = "schema_drift_new.csv";
const TYPE_SHIFT_OLD: &str = "type_shift_old.csv";
const TYPE_SHIFT_NEW: &str = "type_shift_new.csv";
const DUP_KEY_OLD: &str = "dup_key_old.csv";
const DUP_KEY_NEW: &str = "dup_key_new.csv";
const EMPTY: &str = "empty.csv";
const NO_HEADER: &str = "no_header.csv";
const KEY_LOAN_ID: &str = "loan_id";

fn assert_human_stdout_routing(result: &ShapeInvocation, expected_status: i32, context: &str) {
    assert_eq!(result.status, expected_status);
    assert!(
        result.stderr.trim().is_empty(),
        "{context}: {}",
        result.stderr
    );
}

fn assert_human_stderr_routing(result: &ShapeInvocation, expected_status: i32, context: &str) {
    assert_eq!(result.status, expected_status);
    assert!(
        result.stdout.trim().is_empty(),
        "{context}: {}",
        result.stdout
    );
}

#[test]
fn fixture_files_are_present_and_readable() {
    for name in fixture_names() {
        let path = helpers::fixture_path(name);
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => panic_any(format!(
                "fixture {name} missing or unreadable ({}): {error}",
                path.display()
            )),
        };
        assert!(
            metadata.is_file(),
            "fixture {} is not a regular file",
            path.display()
        );
    }
}

#[test]
fn fixture_data_row_counts_match_expected() {
    let expected = BTreeMap::from([
        (BASIC_OLD, 3usize),
        (BASIC_NEW, 3),
        (SCHEMA_DRIFT_OLD, 2),
        (SCHEMA_DRIFT_NEW, 2),
        (TYPE_SHIFT_OLD, 2),
        (TYPE_SHIFT_NEW, 2),
        (DUP_KEY_OLD, 3),
        (DUP_KEY_NEW, 3),
        (EMPTY, 0),
        (NO_HEADER, 0),
    ]);

    for (name, expected_rows) in expected {
        let content = helpers::read_fixture(name);
        let observed = count_data_rows(&content);
        assert_eq!(observed, expected_rows, "unexpected row count for {name}");
    }
}

#[test]
fn cli_harness_captures_usage_error_for_missing_positionals() {
    let result = run_shape(std::iter::empty::<&str>());
    assert_eq!(result.status, 2);
    assert!(
        result.stderr.contains("Usage: shape"),
        "expected clap usage in stderr, got: {}",
        result.stderr
    );
    assert!(
        result.stdout.trim().is_empty(),
        "did not expect stdout for parse error, got: {}",
        result.stdout
    );
}

#[test]
fn cli_harness_rejects_invalid_delimiter_value() {
    let result = run_shape_with_fixtures(BASIC_OLD, BASIC_NEW, &["--delimiter", "bad"]);
    assert_eq!(result.status, 2);
    assert!(
        result.stdout.trim().is_empty(),
        "did not expect stdout for invalid --delimiter, got: {}",
        result.stdout
    );
    assert!(
        result.stderr.contains("invalid --delimiter value"),
        "expected delimiter validation error on stderr, got: {}",
        result.stderr
    );
    assert!(
        result.stderr.contains("unsupported delimiter value: bad"),
        "expected raw invalid delimiter token in stderr, got: {}",
        result.stderr
    );
}

#[test]
fn cli_harness_describe_mode_emits_operator_json() {
    let result = run_shape(["--describe"]);
    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "did not expect stderr output for --describe, got: {}",
        result.stderr
    );

    let payload = parse_json_stdout(
        &result,
        "--describe should emit valid operator.json payload",
    );
    assert_eq!(payload["schema_version"], "operator.v0");
    assert_eq!(payload["name"], "shape");
}

#[test]
fn cli_human_compatible_routes_to_stdout_with_exit_zero() {
    let result = run_shape_with_fixtures(BASIC_OLD, BASIC_NEW, &["--key", KEY_LOAN_ID]);

    assert_human_stdout_routing(
        &result,
        0,
        "human compatible output should not write stderr",
    );
    assert!(result.stdout.contains("COMPATIBLE"));
    assert!(
        result
            .stdout
            .contains("Key: loan_id (unique in both files)")
    );
}

#[test]
fn cli_human_incompatible_routes_to_stdout_with_exit_one() {
    let result = run_shape_with_fixtures(TYPE_SHIFT_OLD, TYPE_SHIFT_NEW, &[]);

    assert_human_stdout_routing(
        &result,
        1,
        "human incompatible output should not write stderr",
    );
    assert!(result.stdout.contains("INCOMPATIBLE"));
    assert!(
        result
            .stdout
            .contains("Type shift: balance changed from numeric to non-numeric"),
        "expected deterministic type-shift reason in human output: {}",
        result.stdout
    );
}

#[test]
fn cli_human_refusal_routes_to_stderr_with_exit_two() {
    let result = run_shape_with_fixtures(BASIC_OLD, EMPTY, &[]);

    assert_human_stderr_routing(&result, 2, "human refusal output should not write stdout");
    assert!(result.stderr.contains("SHAPE ERROR (E_EMPTY)"));
    assert!(
        result.stderr.contains("Compared:"),
        "human refusal should include comparison context: {}",
        result.stderr
    );
}

#[test]
fn cli_human_refusal_omits_dialects_when_old_parse_fails() {
    let result = run_shape_with_fixtures(NO_HEADER, BASIC_NEW, &[]);

    assert_human_stderr_routing(&result, 2, "human refusal output should not write stdout");
    assert!(
        result.stderr.contains("SHAPE ERROR (E_HEADERS)"),
        "expected refusal header in stderr, got: {}",
        result.stderr
    );
    assert!(
        result.stderr.contains("Compared:"),
        "human refusal should include comparison context: {}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("Dialect(old):"),
        "old dialect context should be omitted when old parse fails: {}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("Dialect(new):"),
        "new dialect context should be omitted when old parse fails: {}",
        result.stderr
    );
}

#[test]
fn cli_json_refusal_keeps_old_context_when_new_parse_fails() {
    let old = helpers::fixture_path(BASIC_OLD).display().to_string();
    let new = helpers::fixture_path(NO_HEADER).display().to_string();
    let result = run_shape_with_fixtures(BASIC_OLD, NO_HEADER, &["--json"]);

    assert_eq!(result.status, 2);
    assert_json_mode_stderr_empty(&result, "json output should not write stderr");
    let payload = parse_json_stdout(&result, "json refusal payload should parse");
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_HEADERS");
    assert_eq!(payload["files"]["old"], old);
    assert_eq!(payload["files"]["new"], new);
    assert!(payload["dialect"]["old"].is_object());
    assert!(payload["dialect"]["new"].is_null());
}

#[test]
fn cli_json_refusal_omits_dialects_when_old_parse_fails() {
    let old = helpers::fixture_path(NO_HEADER).display().to_string();
    let new = helpers::fixture_path(BASIC_NEW).display().to_string();
    let result = run_shape_with_fixtures(NO_HEADER, BASIC_NEW, &["--json"]);

    assert_eq!(result.status, 2);
    assert_json_mode_stderr_empty(&result, "json output should not write stderr");
    let payload = parse_json_stdout(&result, "json refusal payload should parse");
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_HEADERS");
    assert_eq!(payload["files"]["old"], old);
    assert_eq!(payload["files"]["new"], new);
    assert!(payload["dialect"]["old"].is_null());
    assert!(payload["dialect"]["new"].is_null());
}

#[test]
fn cli_json_echoes_profile_id_for_compatible_and_refusal_outcomes() {
    // Create a temp profile YAML with profile_id and profile_sha256.
    let profile_dir =
        std::env::temp_dir().join(format!("shape-profile-echo-test-{}", std::process::id()));
    fs::create_dir_all(&profile_dir).unwrap();
    let profile_path = profile_dir.join("monthly.yaml");
    fs::write(
        &profile_path,
        "profile_id: monthly-profile\nprofile_sha256: sha256:deadbeef\ninclude_columns: [loan_id, balance]\n",
    )
    .unwrap();
    let profile_str = profile_path.to_string_lossy().to_string();

    let compatible =
        run_shape_with_fixtures(BASIC_OLD, BASIC_NEW, &["--json", "--profile", &profile_str]);
    assert_eq!(compatible.status, 0);
    assert_json_mode_stderr_empty(
        &compatible,
        "json compatible output should not write stderr",
    );
    let compatible_payload = parse_json_stdout(&compatible, "compatible json should parse");
    assert_eq!(compatible_payload["outcome"], "COMPATIBLE");
    assert_eq!(compatible_payload["profile_id"], "monthly-profile");
    assert_eq!(
        compatible_payload["profile_sha256"], "sha256:deadbeef",
        "resolved sha256 should appear in JSON output"
    );

    let refusal = run_shape_with_fixtures(EMPTY, BASIC_NEW, &["--json", "--profile", &profile_str]);
    assert_eq!(refusal.status, 2);
    assert_json_mode_stderr_empty(&refusal, "json refusal output should not write stderr");
    let refusal_payload = parse_json_stdout(&refusal, "refusal json should parse");
    assert_eq!(refusal_payload["outcome"], "REFUSAL");
    assert_eq!(refusal_payload["profile_id"], "monthly-profile");
    assert_eq!(refusal_payload["profile_sha256"], "sha256:deadbeef");

    fs::remove_dir_all(profile_dir).ok();
}

#[test]
fn cli_human_compatible_with_key_matches_snapshot() {
    let result = run_shape_with_fixtures(BASIC_OLD, BASIC_NEW, &["--key", KEY_LOAN_ID]);
    assert_human_stdout_routing(
        &result,
        0,
        "human compatible output should not write stderr",
    );

    assert_human_snapshot(
        &result.stdout,
        BASIC_OLD,
        BASIC_NEW,
        include_str!("snapshots/human_compatible_with_key.txt"),
    );
}

#[test]
fn cli_human_compatible_without_key_matches_snapshot() {
    let result = run_shape_with_fixtures(BASIC_OLD, BASIC_NEW, &[]);
    assert_human_stdout_routing(
        &result,
        0,
        "human compatible output should not write stderr",
    );

    assert_human_snapshot(
        &result.stdout,
        BASIC_OLD,
        BASIC_NEW,
        include_str!("snapshots/human_compatible_without_key.txt"),
    );
}

#[test]
fn cli_human_incompatible_type_shift_matches_snapshot() {
    let result = run_shape_with_fixtures(TYPE_SHIFT_OLD, TYPE_SHIFT_NEW, &["--key", KEY_LOAN_ID]);
    assert_human_stdout_routing(
        &result,
        1,
        "human incompatible output should not write stderr",
    );

    assert_human_snapshot(
        &result.stdout,
        TYPE_SHIFT_OLD,
        TYPE_SHIFT_NEW,
        include_str!("snapshots/human_incompatible_type_shift.txt"),
    );
}

#[test]
fn cli_human_refusal_dialect_matches_snapshot() {
    let result = run_shape_with_fixtures(BASIC_OLD, NO_HEADER, &[]);
    assert_human_stderr_routing(&result, 2, "human refusal output should not write stdout");

    assert_human_snapshot(
        &result.stderr,
        BASIC_OLD,
        NO_HEADER,
        include_str!("snapshots/human_refusal_dialect.txt"),
    );
}

fn assert_human_snapshot(actual: &str, old_fixture: &str, new_fixture: &str, expected: &str) {
    let old = helpers::fixture_path(old_fixture).display().to_string();
    let new = helpers::fixture_path(new_fixture).display().to_string();
    let normalized_actual =
        normalize_newlines(&actual.replace(&old, "<OLD>").replace(&new, "<NEW>"));
    let normalized_expected = normalize_newlines(expected);

    assert_eq!(
        normalized_actual.trim_end(),
        normalized_expected.trim_end(),
        "snapshot mismatch for {old_fixture} -> {new_fixture}"
    );
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn fixture_names() -> &'static [&'static str] {
    &[
        "README.md",
        BASIC_OLD,
        BASIC_NEW,
        SCHEMA_DRIFT_OLD,
        SCHEMA_DRIFT_NEW,
        TYPE_SHIFT_OLD,
        TYPE_SHIFT_NEW,
        DUP_KEY_OLD,
        DUP_KEY_NEW,
        EMPTY,
        NO_HEADER,
    ]
}

fn count_data_rows(content: &str) -> usize {
    let mut lines = content.lines();
    if lines.next().is_none() {
        return 0;
    }

    lines.filter(|line| !line.trim().is_empty()).count()
}

#[test]
fn parser_reads_comma_delimited_records() {
    let bytes = b"loan_id,balance\nA1,100.0\nA2,200.0\n";
    let config = CsvReaderConfig {
        delimiter: b',',
        has_headers: true,
        escape: EscapeMode::None,
    };

    let mut reader = reader_from_bytes(bytes, &config);
    let mut records = reader.byte_records();

    let first = records
        .next()
        .expect("first record should exist")
        .expect("first record should parse");
    assert_eq!(first.get(0), Some(&b"A1"[..]));
    assert_eq!(first.get(1), Some(&b"100.0"[..]));

    let second = records
        .next()
        .expect("second record should exist")
        .expect("second record should parse");
    assert_eq!(second.get(0), Some(&b"A2"[..]));
    assert_eq!(second.get(1), Some(&b"200.0"[..]));

    assert!(
        records.next().is_none(),
        "expected exactly two data records"
    );
}

#[test]
fn parser_reads_tab_delimited_records() {
    let bytes = b"loan_id\tbalance\nA1\t100.0\nA2\t200.0\n";
    let config = CsvReaderConfig {
        delimiter: b'\t',
        has_headers: true,
        escape: EscapeMode::None,
    };

    let mut reader = reader_from_bytes(bytes, &config);
    let rows = reader.byte_records().fold(0usize, |count, record| {
        record.expect("record should parse");
        count + 1
    });

    assert_eq!(rows, 2);
}

#[test]
fn parser_error_can_be_mapped_to_e_csv_parse_payload() {
    let bytes = b"loan_id,balance\n\"A1,100.0\n";
    let config = CsvReaderConfig {
        delimiter: b',',
        has_headers: true,
        escape: EscapeMode::None,
    };

    let mut reader = reader_from_bytes(bytes, &config);
    let error = reader
        .byte_records()
        .next()
        .expect("expected one parse result")
        .expect_err("malformed csv should fail");

    let line = error.position().map(|pos| pos.line()).unwrap_or(0);
    let payload = RefusalPayload::csv_parse("malformed.csv", line, error.to_string());

    assert_eq!(payload.code, RefusalCode::ECsvParse);
    assert_eq!(payload.detail["file"].as_str(), Some("malformed.csv"));
    assert_eq!(payload.detail["line"].as_u64(), Some(line));
    assert!(
        payload.detail["error"]
            .as_str()
            .is_some_and(|message| !message.is_empty()),
        "expected non-empty parse error message"
    );
}

#[test]
fn parser_byte_record_iteration_scales_for_large_input() {
    let mut csv_data = String::from("loan_id,balance\n");
    for i in 0..10_000usize {
        writeln!(&mut csv_data, "L{i},{}", i * 10).expect("write record");
    }

    let config = CsvReaderConfig {
        delimiter: b',',
        has_headers: true,
        escape: EscapeMode::None,
    };
    let mut reader = reader_from_bytes(csv_data.as_bytes(), &config);

    let rows = reader.byte_records().fold(0usize, |count, record| {
        record.expect("record should parse");
        count + 1
    });
    assert_eq!(rows, 10_000);
}

#[test]
fn check_suite_assembly_with_key_sets_optional_fields() {
    let schema = evaluate_schema_overlap(
        &[b"loan_id".to_vec(), b"balance".to_vec()],
        &[b"loan_id".to_vec(), b"balance".to_vec()],
        None,
    );
    let old_scan = key_scan(&[b"K1", b"K2"], 0, 0);
    let new_scan = key_scan(&[b"K1", b"K3"], 0, 0);
    let key = evaluate_key_viability(
        b"loan_id".to_vec(),
        true,
        true,
        Some(&old_scan),
        Some(&new_scan),
    );
    let key_metrics = compute_key_overlap_metrics(&old_scan, &new_scan);
    let rows = evaluate_row_granularity(2, 2, Some(key_metrics));
    let types = TypeConsistencyResult {
        status: CheckStatus::Pass,
        numeric_columns: 1,
        type_shifts: vec![],
    };

    let suite = CheckSuite {
        schema_overlap: schema,
        key_viability: Some(key),
        row_granularity: rows,
        type_consistency: types,
    };

    assert_eq!(suite.determine_outcome(), Outcome::Compatible);
    assert_eq!(suite.row_granularity.key_overlap, Some(1));
    assert_eq!(suite.row_granularity.keys_old_only, Some(1));
    assert_eq!(suite.row_granularity.keys_new_only, Some(1));
    assert!(
        suite
            .key_viability
            .as_ref()
            .is_some_and(|k| k.unique_old == Some(true) && k.unique_new == Some(true))
    );
}

#[test]
fn check_suite_assembly_without_key_uses_nullability_contract() {
    let schema = evaluate_schema_overlap(&[b"a".to_vec()], &[b"a".to_vec()], None);
    let rows = evaluate_row_granularity(3, 4, None);
    let types = TypeConsistencyResult {
        status: CheckStatus::Pass,
        numeric_columns: 0,
        type_shifts: vec![],
    };

    let suite = CheckSuite {
        schema_overlap: schema,
        key_viability: None,
        row_granularity: rows,
        type_consistency: types,
    };

    assert_eq!(suite.determine_outcome(), Outcome::Compatible);
    assert!(suite.key_viability.is_none());
    assert_eq!(suite.row_granularity.key_overlap, None);
    assert_eq!(suite.row_granularity.keys_old_only, None);
    assert_eq!(suite.row_granularity.keys_new_only, None);
}

#[test]
fn check_suite_assembly_key_missing_is_incompatible_with_expected_nulls() {
    let schema = evaluate_schema_overlap(
        &[b"loan_id".to_vec(), b"balance".to_vec()],
        &[b"id".to_vec(), b"balance".to_vec()],
        None,
    );
    let old_scan = key_scan(&[b"K1", b"K2"], 0, 0);
    let key = evaluate_key_viability(b"loan_id".to_vec(), true, false, Some(&old_scan), None);
    let rows = evaluate_row_granularity(2, 2, None);
    let types = TypeConsistencyResult {
        status: CheckStatus::Pass,
        numeric_columns: 1,
        type_shifts: vec![],
    };
    let suite = CheckSuite {
        schema_overlap: schema,
        key_viability: Some(key),
        row_granularity: rows,
        type_consistency: types,
    };

    assert_eq!(suite.determine_outcome(), Outcome::Incompatible);
    let key = suite
        .key_viability
        .as_ref()
        .expect("key check should exist");
    assert!(key.found_old);
    assert!(!key.found_new);
    assert_eq!(key.unique_old, Some(true));
    assert_eq!(key.unique_new, None);
    assert_eq!(key.coverage, None);
    assert_eq!(suite.row_granularity.key_overlap, None);
    assert_eq!(
        build_reasons(&suite),
        vec!["Key viability: loan_id not found in new file".to_string()]
    );
}

#[test]
fn human_compatible_without_key_omits_key_lines_and_overlap_metrics() {
    let suite = CheckSuite {
        schema_overlap: evaluate_schema_overlap(&[b"a".to_vec()], &[b"a".to_vec()], None),
        key_viability: None,
        row_granularity: evaluate_row_granularity(3_214, 3_201, None),
        type_consistency: TypeConsistencyResult {
            status: CheckStatus::Pass,
            numeric_columns: 12,
            type_shifts: vec![],
        },
    };

    let rendered = render_compatible(
        "nov.csv",
        "dec.csv",
        Dialect::default(),
        Dialect::default(),
        &suite,
        true,
    );

    assert!(rendered.contains("SHAPE\n\nCOMPATIBLE"));
    assert!(!rendered.contains("\nKey: "));
    assert!(rendered.contains("Rows:      3,214 old / 3,201 new"));
    assert!(!rendered.contains("Rows:      3,214 old / 3,201 new ("));
}

#[test]
fn human_incompatible_missing_key_keeps_rows_line_without_overlap_detail() {
    let suite = CheckSuite {
        schema_overlap: evaluate_schema_overlap(
            &[b"loan_id".to_vec(), b"amount".to_vec()],
            &[b"amount".to_vec(), b"status".to_vec()],
            None,
        ),
        key_viability: Some(evaluate_key_viability(
            b"loan_id".to_vec(),
            true,
            false,
            Some(&key_scan(&[b"K1", b"K2"], 0, 0)),
            None,
        )),
        row_granularity: evaluate_row_granularity(4_183, 4_201, None),
        type_consistency: TypeConsistencyResult {
            status: CheckStatus::Pass,
            numeric_columns: 12,
            type_shifts: vec![],
        },
    };

    let reasons = vec!["Key viability: loan_id not found in new file".to_string()];
    let rendered = render_incompatible(
        "nov.csv",
        "dec.csv",
        Dialect::default(),
        Dialect::default(),
        &suite,
        &reasons,
        true,
    );

    assert!(rendered.contains("INCOMPATIBLE"));
    assert!(rendered.contains("Key: loan_id (NOT FOUND in new file)"));
    assert!(rendered.contains("Key:       loan_id — not found in new file"));
    assert!(rendered.contains("Rows:      4,183 old / 4,201 new"));
    assert!(!rendered.contains("Rows:      4,183 old / 4,201 new ("));
    assert!(rendered.contains("Reasons:\n  1. Key viability: loan_id not found in new file"));
}

#[test]
fn human_refusal_context_omits_unknown_new_dialect() {
    let refusal = RefusalPayload::empty("dec.csv", 0);
    let context = RefusalRenderContext {
        old_path: "nov.csv",
        new_path: "dec.csv",
        dialect_old: Some(Dialect::default()),
        dialect_new: None,
    };

    let rendered = render_refusal_with_context(&refusal, &context);

    assert!(rendered.contains("SHAPE ERROR (E_EMPTY)"));
    assert!(rendered.contains("Compared: nov.csv -> dec.csv"));
    assert!(rendered.contains("Dialect(old): delimiter=, quote=\" escape=none"));
    assert!(!rendered.contains("Dialect(new):"));
    assert!(rendered.contains("Next: provide non-empty datasets."));
}

fn key_scan(values: &[&[u8]], duplicate_count: u64, empty_count: u64) -> KeyScan {
    KeyScan {
        values: values
            .iter()
            .map(|value| value.to_vec())
            .collect::<HashSet<_>>(),
        duplicate_count,
        empty_count,
    }
}
