mod helpers;

use serde_json::Value;

use helpers::{ShapeInvocation, fixture_path, run_shape};

const BASIC_OLD: &str = "basic_old.csv";
const BASIC_NEW: &str = "basic_new.csv";
const TYPE_SHIFT_OLD: &str = "type_shift_old.csv";
const TYPE_SHIFT_NEW: &str = "type_shift_new.csv";
const NO_HEADER: &str = "no_header.csv";
const EMPTY: &str = "empty.csv";
const KEY_LOAN_ID: &str = "loan_id";

struct ExitCodeCase<'a> {
    old_fixture: &'a str,
    new_fixture: &'a str,
    extra_args: &'a [&'a str],
    expected_status: i32,
    expect_refusal: bool,
}

fn run_case(old_fixture: &str, new_fixture: &str, extra_args: &[&str]) -> ShapeInvocation {
    let old = fixture_path(old_fixture).display().to_string();
    let new = fixture_path(new_fixture).display().to_string();
    let mut args = vec![old, new];
    args.extend(extra_args.iter().map(|arg| (*arg).to_string()));
    run_shape(args)
}

fn compared_line(old_fixture: &str, new_fixture: &str) -> String {
    format!(
        "Compared: {} -> {}",
        fixture_path(old_fixture).display(),
        fixture_path(new_fixture).display()
    )
}

fn assert_contains_compared_line(
    output: &str,
    old_fixture: &str,
    new_fixture: &str,
    context: &str,
) {
    assert!(
        output.contains(&compared_line(old_fixture, new_fixture)),
        "{context}"
    );
}

fn parse_json_output(invocation: &ShapeInvocation, context: &str) -> Value {
    serde_json::from_str(invocation.stdout_trimmed()).expect(context)
}

fn assert_human_non_refusal(
    invocation: &ShapeInvocation,
    expected_status: i32,
    expected_outcome_fragment: &str,
) {
    assert_eq!(invocation.status, expected_status);
    assert!(
        invocation.stderr.trim().is_empty(),
        "unexpected stderr: {:?}",
        invocation.stderr
    );
    assert!(invocation.stdout.contains(expected_outcome_fragment));
}

fn assert_human_refusal(invocation: &ShapeInvocation, expected_status: i32) {
    assert_eq!(invocation.status, expected_status);
    assert!(invocation.stdout.trim().is_empty());
    assert!(invocation.stderr.contains("SHAPE ERROR"));
}

fn assert_json_outcome(
    invocation: &ShapeInvocation,
    expected_status: i32,
    expected_outcome: &str,
    context: &str,
) {
    assert_eq!(invocation.status, expected_status);
    assert!(
        invocation.stderr.trim().is_empty(),
        "unexpected stderr: {:?}",
        invocation.stderr
    );
    let value = parse_json_output(invocation, context);
    assert_eq!(value["outcome"], expected_outcome);
}

fn assert_exit_code_case(case: &ExitCodeCase<'_>) {
    let invocation = run_case(case.old_fixture, case.new_fixture, case.extra_args);
    if case.expect_refusal {
        assert_human_refusal(&invocation, case.expected_status);
    } else {
        assert_eq!(invocation.status, case.expected_status);
    }
}

#[test]
fn harness_runs_human_and_json_through_shared_invoker() {
    let human = run_case(BASIC_OLD, BASIC_NEW, &["--key", KEY_LOAN_ID]);
    assert_human_non_refusal(&human, 0, "COMPATIBLE");

    let json = run_case(BASIC_OLD, BASIC_NEW, &["--json", "--key", KEY_LOAN_ID]);
    assert_json_outcome(&json, 0, "COMPATIBLE", "json output");
}

#[test]
fn harness_verifies_exit_codes_across_core_outcomes() {
    let cases = [
        ExitCodeCase {
            old_fixture: BASIC_OLD,
            new_fixture: BASIC_NEW,
            extra_args: &[],
            expected_status: 0,
            expect_refusal: false,
        },
        ExitCodeCase {
            old_fixture: TYPE_SHIFT_OLD,
            new_fixture: TYPE_SHIFT_NEW,
            extra_args: &["--json"],
            expected_status: 1,
            expect_refusal: false,
        },
        ExitCodeCase {
            old_fixture: EMPTY,
            new_fixture: BASIC_NEW,
            extra_args: &[],
            expected_status: 2,
            expect_refusal: true,
        },
    ];

    for case in cases {
        assert_exit_code_case(&case);
    }
}

#[test]
fn harness_checks_path_echo_and_partial_context_refusal() {
    let human = run_case(BASIC_OLD, NO_HEADER, &[]);
    assert_eq!(human.status, 2);
    assert_contains_compared_line(
        &human.stderr,
        BASIC_OLD,
        NO_HEADER,
        "human refusal should include compared line",
    );
    assert!(human.stderr.contains("Dialect(old):"));
    assert!(!human.stderr.contains("Dialect(new):"));

    let json = run_case(BASIC_OLD, NO_HEADER, &["--json"]);
    assert_eq!(json.status, 2);
    let value = parse_json_output(&json, "json refusal output");
    assert_eq!(
        value["files"]["old"],
        fixture_path(BASIC_OLD).display().to_string()
    );
    assert_eq!(
        value["files"]["new"],
        fixture_path(NO_HEADER).display().to_string()
    );
    assert!(value["dialect"]["old"].is_object());
    assert!(value["dialect"]["new"].is_null());
}

#[test]
fn harness_checks_old_parse_refusal_omits_both_dialect_contexts() {
    let human = run_case(NO_HEADER, BASIC_NEW, &[]);
    assert_eq!(human.status, 2);
    assert_contains_compared_line(
        &human.stderr,
        NO_HEADER,
        BASIC_NEW,
        "human refusal should include compared line",
    );
    assert!(!human.stderr.contains("Dialect(old):"));
    assert!(!human.stderr.contains("Dialect(new):"));

    let json = run_case(NO_HEADER, BASIC_NEW, &["--json"]);
    assert_eq!(json.status, 2);
    let value = parse_json_output(&json, "json refusal output");
    assert_eq!(value["refusal"]["code"], "E_HEADERS");
    assert_eq!(
        value["files"]["old"],
        fixture_path(NO_HEADER).display().to_string()
    );
    assert_eq!(
        value["files"]["new"],
        fixture_path(BASIC_NEW).display().to_string()
    );
    assert!(value["dialect"]["old"].is_null());
    assert!(value["dialect"]["new"].is_null());
}
