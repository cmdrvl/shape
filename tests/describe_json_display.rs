mod helpers;

use helpers::{ParseFailureCase, assert_parse_failure_routes_to_stderr_matrix, run_shape};
use serde_json::Value;

const MISSING_OLD: &str = "missing-old.csv";
const MISSING_NEW: &str = "missing-new.csv";
const INVALID_DELIMITER: &str = "bad";

fn assert_operator_payload(stdout: &str) {
    let payload: Value =
        serde_json::from_str(stdout).expect("--describe should emit valid operator.json payload");
    assert_eq!(payload["schema_version"], "operator.v0");
    assert_eq!(payload["name"], "shape");

    let refusals = payload["refusals"]
        .as_array()
        .expect("operator payload must include refusal definitions");
    let too_large = refusals
        .iter()
        .find(|entry| entry["code"] == "E_TOO_LARGE")
        .expect("operator payload must include E_TOO_LARGE refusal");

    assert_eq!(too_large["action"], "retry_with_flag");
    assert_eq!(too_large["flag_source"], "detail.limit_flag");
    assert_eq!(too_large["value_source"], "detail.actual");
}

fn assert_describe_success(args: &[&str]) {
    let result = run_shape(args);

    assert_eq!(
        result.status, 0,
        "--describe should exit 0 for args {:?}",
        args
    );
    assert!(
        result.stderr.trim().is_empty(),
        "--describe should not write stderr for args {:?}: {}",
        args,
        result.stderr
    );

    assert_operator_payload(result.stdout_trimmed());
}

fn assert_describe_success_cases(cases: &[&[&str]]) {
    for args in cases {
        assert_describe_success(args);
    }
}

fn assert_describe_parse_failure_variants(
    args_without_json: &[&str],
    args_with_json: &[&str],
    expected_stderr_fragment: &str,
) {
    let cases = [
        ParseFailureCase {
            args: args_without_json,
            expected_stderr_fragment,
        },
        ParseFailureCase {
            args: args_with_json,
            expected_stderr_fragment,
        },
    ];

    assert_parse_failure_routes_to_stderr_matrix(&cases);
}

#[test]
fn describe_with_json_remains_display_mode_and_emits_operator_payload() {
    assert_describe_success(&["--describe", "--json"]);
}

#[test]
fn describe_short_circuits_before_input_validation() {
    let cases: &[&[&str]] = &[
        &[MISSING_OLD, MISSING_NEW, "--describe"],
        &[MISSING_OLD, MISSING_NEW, "--describe", "--json"],
    ];

    assert_describe_success_cases(cases);
}

#[test]
fn describe_still_surfaces_parse_validation_errors() {
    assert_describe_parse_failure_variants(
        &["--describe", "--max-rows", "abc"],
        &["--describe", "--json", "--max-rows", "abc"],
        "--max-rows",
    );
}

#[test]
fn describe_still_surfaces_profile_selector_conflicts() {
    assert_describe_parse_failure_variants(
        &[
            "--describe",
            "--profile",
            "profile.toml",
            "--profile-id",
            "monthly",
        ],
        &[
            "--describe",
            "--json",
            "--profile",
            "profile.toml",
            "--profile-id",
            "monthly",
        ],
        "--profile-id",
    );
}

#[test]
fn describe_bypasses_runtime_delimiter_validation() {
    let cases: &[&[&str]] = &[
        &["--describe", "--delimiter", INVALID_DELIMITER],
        &["--describe", "--json", "--delimiter", INVALID_DELIMITER],
    ];

    assert_describe_success_cases(cases);
}

#[test]
fn describe_with_positionals_still_bypasses_runtime_delimiter_validation() {
    let cases: &[&[&str]] = &[
        &[
            MISSING_OLD,
            MISSING_NEW,
            "--describe",
            "--delimiter",
            INVALID_DELIMITER,
        ],
        &[
            MISSING_OLD,
            MISSING_NEW,
            "--describe",
            "--json",
            "--delimiter",
            INVALID_DELIMITER,
        ],
        &[
            MISSING_OLD,
            MISSING_NEW,
            "--delimiter",
            INVALID_DELIMITER,
            "--describe",
        ],
    ];

    assert_describe_success_cases(cases);
}
