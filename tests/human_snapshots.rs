mod helpers;

use helpers::{ShapeInvocation, read_fixture, run_shape};
use serde_json::Value;

const BASIC_OLD: &str = "tests/fixtures/basic_old.csv";
const BASIC_NEW: &str = "tests/fixtures/basic_new.csv";
const AMBIGUOUS_OLD: &str = "tests/fixtures/ambiguous_old.csv";
const TYPE_SHIFT_OLD: &str = "tests/fixtures/type_shift_old.csv";
const TYPE_SHIFT_NEW: &str = "tests/fixtures/type_shift_new.csv";
const EMPTY_NEW: &str = "tests/fixtures/empty.csv";
const NO_HEADER_OLD: &str = "tests/fixtures/no_header.csv";
const KEY_LOAN_ID: &str = "loan_id";

struct JsonOutcomeCase {
    args: &'static [&'static str],
    expected_status: i32,
    expected_outcome: &'static str,
    context: &'static str,
}

fn parse_json_output(result: &ShapeInvocation, context: &str) -> Value {
    serde_json::from_str(result.stdout_trimmed()).expect(context)
}

fn assert_json_outcome(args: &[&str], expected_status: i32, expected_outcome: &str, context: &str) {
    let result = run_shape(args);
    assert_eq!(result.status, expected_status);
    assert!(result.stderr.trim().is_empty());

    let payload = parse_json_output(&result, context);
    assert_eq!(payload["outcome"], expected_outcome);
}

fn assert_json_outcome_cases(cases: &[JsonOutcomeCase]) {
    for case in cases {
        assert_json_outcome(
            case.args,
            case.expected_status,
            case.expected_outcome,
            case.context,
        );
    }
}

fn assert_human_stdout_snapshot(
    args: &[&str],
    expected_status: i32,
    snapshot_fixture: &str,
) -> ShapeInvocation {
    let result = run_shape(args);
    assert_eq!(result.status, expected_status);
    assert!(result.stderr.trim().is_empty());
    assert_eq!(result.stdout, read_fixture(snapshot_fixture));
    result
}

fn assert_human_stderr_snapshot(
    args: &[&str],
    expected_status: i32,
    snapshot_fixture: &str,
) -> ShapeInvocation {
    let result = run_shape(args);
    assert_eq!(result.status, expected_status);
    assert!(result.stdout.trim().is_empty());
    assert_eq!(result.stderr, read_fixture(snapshot_fixture));
    result
}

#[test]
fn human_compatible_with_key_matches_golden_snapshot() {
    let result = assert_human_stdout_snapshot(
        &[BASIC_OLD, BASIC_NEW, "--key", KEY_LOAN_ID],
        0,
        "goldens/human_compatible_with_key.txt",
    );
    assert!(result.stdout.contains("Schema:"));
    assert!(result.stdout.contains("Rows:"));
    assert!(result.stdout.contains("Types:"));
}

#[test]
fn human_compatible_without_key_matches_golden_snapshot() {
    let result = assert_human_stdout_snapshot(
        &[BASIC_OLD, BASIC_NEW],
        0,
        "goldens/human_compatible_without_key.txt",
    );
    assert!(!result.stdout.contains("Key:"));
}

#[test]
fn human_incompatible_matches_golden_snapshot() {
    let result = assert_human_stdout_snapshot(
        &[TYPE_SHIFT_OLD, TYPE_SHIFT_NEW],
        1,
        "goldens/human_incompatible.txt",
    );
    assert!(result.stdout.contains("Reasons:"));
    assert!(
        result
            .stdout
            .contains("Type shift: balance changed from numeric to non-numeric")
    );
}

#[test]
fn human_refusal_routes_to_stderr_and_matches_golden_snapshot() {
    let result =
        assert_human_stderr_snapshot(&[BASIC_OLD, EMPTY_NEW], 2, "goldens/human_refusal.txt");
    assert!(result.stderr.contains("SHAPE ERROR (E_EMPTY)"));
}

#[test]
fn old_parse_refusal_omits_dialect_contexts_in_human_and_json() {
    let human = run_shape([NO_HEADER_OLD, BASIC_NEW]);
    assert_eq!(human.status, 2);
    assert!(human.stdout.trim().is_empty());
    assert!(human.stderr.contains("SHAPE ERROR (E_HEADERS)"));
    assert!(!human.stderr.contains("Dialect(old):"));
    assert!(!human.stderr.contains("Dialect(new):"));

    let json = run_shape([NO_HEADER_OLD, BASIC_NEW, "--json"]);
    assert_eq!(json.status, 2);
    assert!(json.stderr.trim().is_empty());
    let payload = parse_json_output(&json, "old parse refusal json should parse");
    assert_eq!(payload["outcome"], "REFUSAL");
    assert!(payload["dialect"]["old"].is_null());
    assert!(payload["dialect"]["new"].is_null());
}

#[test]
fn ambiguous_dialect_refusal_matches_golden_snapshot_and_json_contract() {
    let human = assert_human_stderr_snapshot(
        &[AMBIGUOUS_OLD, BASIC_NEW],
        2,
        "goldens/human_refusal_dialect_ambiguous.txt",
    );
    assert!(human.stderr.contains("SHAPE ERROR (E_DIALECT)"));
    assert!(
        human.stderr.contains("--delimiter comma --json"),
        "expected actionable dialect next-command in human refusal output"
    );

    let json = run_shape([AMBIGUOUS_OLD, BASIC_NEW, "--json"]);
    assert_eq!(json.status, 2);
    assert!(json.stderr.trim().is_empty());
    let payload = parse_json_output(&json, "ambiguous dialect refusal json should parse");
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_DIALECT");
    assert_eq!(payload["dialect"]["old"], Value::Null);
    assert_eq!(payload["dialect"]["new"], Value::Null);
    assert_eq!(
        payload["refusal"]["next_command"],
        "shape tests/fixtures/ambiguous_old.csv tests/fixtures/basic_new.csv --delimiter comma --json"
    );
}

#[test]
fn json_outcomes_route_to_stdout_for_all_exit_codes() {
    let cases = [
        JsonOutcomeCase {
            args: &[BASIC_OLD, BASIC_NEW, "--json"],
            expected_status: 0,
            expected_outcome: "COMPATIBLE",
            context: "compatible json should parse",
        },
        JsonOutcomeCase {
            args: &[TYPE_SHIFT_OLD, TYPE_SHIFT_NEW, "--json"],
            expected_status: 1,
            expected_outcome: "INCOMPATIBLE",
            context: "incompatible json should parse",
        },
        JsonOutcomeCase {
            args: &[BASIC_OLD, EMPTY_NEW, "--json"],
            expected_status: 2,
            expected_outcome: "REFUSAL",
            context: "refusal json should parse",
        },
    ];

    assert_json_outcome_cases(&cases);
}
