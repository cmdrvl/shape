mod helpers;

use helpers::{
    assert_parse_failure_with_fixtures_routes_to_stderr, fixture_path, run_shape_with_fixtures,
};
use serde_json::Value;

const BASIC_OLD: &str = "basic_old.csv";
const BASIC_NEW: &str = "basic_new.csv";
const TYPE_SHIFT_OLD: &str = "type_shift_old.csv";
const TYPE_SHIFT_NEW: &str = "type_shift_new.csv";
const EMPTY: &str = "empty.csv";
const NO_HEADER: &str = "no_header.csv";
const KEY_LOAN_ID: &str = "loan_id";

struct DomainOutcomeCase {
    old_fixture: &'static str,
    new_fixture: &'static str,
    extra_args: &'static [&'static str],
    expected_status: i32,
    expected_outcome: &'static str,
}

fn assert_human_domain_outcome(
    old_fixture: &str,
    new_fixture: &str,
    extra_args: &[&str],
    expected_status: i32,
    expected_outcome: &str,
) {
    let result = run_shape_with_fixtures(old_fixture, new_fixture, extra_args);
    assert_eq!(result.status, expected_status);

    if expected_status == 2 {
        assert!(result.stdout.is_empty());
        assert!(result.stderr.contains("SHAPE ERROR"));
    } else {
        assert!(result.stdout.contains(expected_outcome));
        assert!(result.stderr.is_empty());
    }
}

fn assert_json_domain_outcome(
    old_fixture: &str,
    new_fixture: &str,
    extra_args: &[&str],
    expected_status: i32,
    expected_outcome: &str,
) {
    let result = run_shape_with_fixtures(old_fixture, new_fixture, extra_args);
    assert_eq!(result.status, expected_status);
    assert!(result.stderr.is_empty());

    let payload: Value =
        serde_json::from_str(result.stdout_trimmed()).expect("stdout should contain valid JSON");
    assert_eq!(payload["outcome"], expected_outcome);
}

fn assert_human_domain_outcome_matrix(cases: &[DomainOutcomeCase]) {
    for case in cases {
        assert_human_domain_outcome(
            case.old_fixture,
            case.new_fixture,
            case.extra_args,
            case.expected_status,
            case.expected_outcome,
        );
    }
}

fn assert_json_domain_outcome_matrix(cases: &[DomainOutcomeCase]) {
    for case in cases {
        assert_json_domain_outcome(
            case.old_fixture,
            case.new_fixture,
            case.extra_args,
            case.expected_status,
            case.expected_outcome,
        );
    }
}

#[test]
fn human_mode_routes_refusal_to_stderr_and_other_outcomes_to_stdout() {
    let cases = [
        DomainOutcomeCase {
            old_fixture: BASIC_OLD,
            new_fixture: BASIC_NEW,
            extra_args: &["--key", KEY_LOAN_ID],
            expected_status: 0,
            expected_outcome: "COMPATIBLE",
        },
        DomainOutcomeCase {
            old_fixture: TYPE_SHIFT_OLD,
            new_fixture: TYPE_SHIFT_NEW,
            extra_args: &[],
            expected_status: 1,
            expected_outcome: "INCOMPATIBLE",
        },
        DomainOutcomeCase {
            old_fixture: EMPTY,
            new_fixture: BASIC_NEW,
            extra_args: &[],
            expected_status: 2,
            expected_outcome: "REFUSAL",
        },
        DomainOutcomeCase {
            old_fixture: NO_HEADER,
            new_fixture: BASIC_NEW,
            extra_args: &[],
            expected_status: 2,
            expected_outcome: "REFUSAL",
        },
    ];

    assert_human_domain_outcome_matrix(&cases);
}

#[test]
fn json_mode_routes_all_outcomes_to_stdout() {
    let cases = [
        DomainOutcomeCase {
            old_fixture: BASIC_OLD,
            new_fixture: BASIC_NEW,
            extra_args: &["--json", "--key", KEY_LOAN_ID],
            expected_status: 0,
            expected_outcome: "COMPATIBLE",
        },
        DomainOutcomeCase {
            old_fixture: TYPE_SHIFT_OLD,
            new_fixture: TYPE_SHIFT_NEW,
            extra_args: &["--json"],
            expected_status: 1,
            expected_outcome: "INCOMPATIBLE",
        },
        DomainOutcomeCase {
            old_fixture: EMPTY,
            new_fixture: BASIC_NEW,
            extra_args: &["--json"],
            expected_status: 2,
            expected_outcome: "REFUSAL",
        },
        DomainOutcomeCase {
            old_fixture: NO_HEADER,
            new_fixture: BASIC_NEW,
            extra_args: &["--json"],
            expected_status: 2,
            expected_outcome: "REFUSAL",
        },
    ];

    assert_json_domain_outcome_matrix(&cases);
}

#[test]
fn json_mode_process_level_failures_stay_on_stderr() {
    assert_parse_failure_with_fixtures_routes_to_stderr(
        BASIC_OLD,
        BASIC_NEW,
        &["--json", "--delimiter", "bad"],
        &["invalid --delimiter value"],
    );
}

#[test]
fn empty_refusal_json_includes_both_dialects_and_rows_detail() {
    let result = run_shape_with_fixtures(BASIC_OLD, EMPTY, &["--json"]);
    assert_eq!(result.status, 2);
    assert!(
        result.stderr.trim().is_empty(),
        "json domain outcomes should not write stderr: {}",
        result.stderr
    );

    let payload: Value =
        serde_json::from_str(result.stdout_trimmed()).expect("stdout should contain valid JSON");
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_EMPTY");
    assert_eq!(
        payload["refusal"]["detail"]["file"],
        fixture_path(EMPTY).display().to_string()
    );
    assert_eq!(payload["refusal"]["detail"]["rows"], 0);
    assert!(payload["refusal"]["next_command"].is_null());
    assert!(payload["dialect"]["old"].is_object());
    assert!(payload["dialect"]["new"].is_object());
}
