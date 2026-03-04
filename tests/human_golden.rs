mod helpers;

use helpers::{ShapeInvocation, fixture_path, run_shape_with_fixtures};

const BASIC_OLD: &str = "basic_old.csv";
const BASIC_NEW: &str = "basic_new.csv";
const TYPE_SHIFT_OLD: &str = "type_shift_old.csv";
const TYPE_SHIFT_NEW: &str = "type_shift_new.csv";
const DUP_KEY_OLD: &str = "dup_key_old.csv";
const DUP_KEY_NEW: &str = "dup_key_new.csv";
const EMPTY: &str = "empty.csv";
const NO_HEADER: &str = "no_header.csv";
const DIALECT_AMBIGUOUS: &str = "dialect_ambiguous.csv";
const KEY_LOAN_ID: &str = "loan_id";

fn assert_human_stdout_only(
    old_fixture: &str,
    new_fixture: &str,
    extra_args: &[&str],
    expected_status: i32,
    context: &str,
) -> ShapeInvocation {
    let result = run_shape_with_fixtures(old_fixture, new_fixture, extra_args);
    assert_eq!(result.status, expected_status);
    assert!(
        result.stderr.trim().is_empty(),
        "{context} should not write stderr: {}",
        result.stderr
    );
    result
}

fn assert_human_stderr_only(
    old_fixture: &str,
    new_fixture: &str,
    extra_args: &[&str],
    expected_status: i32,
    context: &str,
) -> ShapeInvocation {
    let result = run_shape_with_fixtures(old_fixture, new_fixture, extra_args);
    assert_eq!(result.status, expected_status);
    assert!(
        result.stdout.trim().is_empty(),
        "{context} should not write stdout: {}",
        result.stdout
    );
    result
}

fn assert_json_domain_stdout_only(
    old_fixture: &str,
    new_fixture: &str,
    expected_status: i32,
    context: &str,
) {
    let result = run_shape_with_fixtures(old_fixture, new_fixture, &["--json"]);
    assert_eq!(result.status, expected_status);
    assert!(
        result.stderr.trim().is_empty(),
        "{context} should not write stderr: {}",
        result.stderr
    );
    assert!(
        !result.stdout.trim().is_empty(),
        "{context} should write JSON payload to stdout"
    );
}

#[test]
fn human_compatible_with_key_matches_golden_snapshot() {
    let result = assert_human_stdout_only(
        BASIC_OLD,
        BASIC_NEW,
        &["--key", KEY_LOAN_ID],
        0,
        "human compatible output",
    );

    let expected = format!(
        concat!(
            "SHAPE\n\n",
            "COMPATIBLE\n\n",
            "Compared: {} -> {}\n",
            "Key: loan_id (unique in both files)\n",
            "Dialect(old): delimiter=, quote=\" escape=none\n",
            "Dialect(new): delimiter=, quote=\" escape=none\n\n",
            "Schema:    3 common / 3 total (100% overlap)\n",
            "Key:       loan_id — unique in both, coverage=0.67\n",
            "Rows:      3 old / 3 new (1 removed, 1 added, 2 overlap)\n",
            "Types:     1 numeric columns, 0 type shifts\n"
        ),
        fixture_path(BASIC_OLD).display(),
        fixture_path(BASIC_NEW).display()
    );

    assert_eq!(result.stdout, expected);
}

#[test]
fn human_compatible_without_key_matches_golden_snapshot() {
    let result = assert_human_stdout_only(BASIC_OLD, BASIC_NEW, &[], 0, "human compatible output");

    let expected = format!(
        concat!(
            "SHAPE\n\n",
            "COMPATIBLE\n\n",
            "Compared: {} -> {}\n",
            "Dialect(old): delimiter=, quote=\" escape=none\n",
            "Dialect(new): delimiter=, quote=\" escape=none\n\n",
            "Schema:    3 common / 3 total (100% overlap)\n",
            "Rows:      3 old / 3 new\n",
            "Types:     1 numeric columns, 0 type shifts\n"
        ),
        fixture_path(BASIC_OLD).display(),
        fixture_path(BASIC_NEW).display()
    );

    assert_eq!(result.stdout, expected);
}

#[test]
fn human_incompatible_matches_golden_snapshot() {
    let result = assert_human_stdout_only(
        TYPE_SHIFT_OLD,
        TYPE_SHIFT_NEW,
        &[],
        1,
        "human incompatible output",
    );

    let expected = format!(
        concat!(
            "SHAPE\n\n",
            "INCOMPATIBLE\n\n",
            "Compared: {} -> {}\n",
            "Dialect(old): delimiter=, quote=\" escape=none\n",
            "Dialect(new): delimiter=, quote=\" escape=none\n\n",
            "Schema:    3 common / 3 total (100% overlap)\n",
            "Rows:      2 old / 2 new\n",
            "Types:     0 numeric columns, 1 type shift\n\n",
            "Reasons:\n",
            "  1. Type shift: balance changed from numeric to non-numeric\n"
        ),
        fixture_path(TYPE_SHIFT_OLD).display(),
        fixture_path(TYPE_SHIFT_NEW).display()
    );

    assert_eq!(result.stdout, expected);
}

#[test]
fn human_incompatible_with_key_non_viable_matches_golden_snapshot() {
    let result = assert_human_stdout_only(
        DUP_KEY_OLD,
        DUP_KEY_NEW,
        &["--key", KEY_LOAN_ID],
        1,
        "human incompatible key viability output",
    );

    let expected = format!(
        concat!(
            "SHAPE\n\n",
            "INCOMPATIBLE\n\n",
            "Compared: {} -> {}\n",
            "Key: loan_id (NOT VIABLE — 1 duplicate in old)\n",
            "Dialect(old): delimiter=, quote=\" escape=none\n",
            "Dialect(new): delimiter=, quote=\" escape=none\n\n",
            "Schema:    3 common / 3 total (100% overlap)\n",
            "Key:       loan_id — 1 duplicate in old, coverage=0.67\n",
            "Rows:      3 old / 3 new (0 removed, 1 added, 2 overlap)\n",
            "Types:     1 numeric columns, 0 type shifts\n\n",
            "Reasons:\n",
            "  1. Key viability: loan_id has 1 duplicate value in old file\n"
        ),
        fixture_path(DUP_KEY_OLD).display(),
        fixture_path(DUP_KEY_NEW).display()
    );

    assert_eq!(result.stdout, expected);
}

#[test]
fn human_refusal_matches_golden_snapshot_and_routes_to_stderr() {
    let result = assert_human_stderr_only(BASIC_OLD, EMPTY, &[], 2, "human refusal output");

    let expected = format!(
        concat!(
            "SHAPE ERROR (E_EMPTY)\n\n",
            "Compared: {} -> {}\n",
            "Dialect(old): delimiter=, quote=\" escape=none\n",
            "Dialect(new): delimiter=, quote=\" escape=none\n\n",
            "One or both files empty (no data rows after header)\n",
            "Next: provide non-empty datasets.\n"
        ),
        fixture_path(BASIC_OLD).display(),
        fixture_path(EMPTY).display()
    );

    assert_eq!(result.stderr, expected);
}

#[test]
fn human_refusal_without_dialect_context_matches_golden_snapshot() {
    let result = assert_human_stderr_only(NO_HEADER, BASIC_NEW, &[], 2, "human refusal output");

    let old = fixture_path(NO_HEADER).display().to_string();
    let new = fixture_path(BASIC_NEW).display().to_string();
    let expected = format!(
        concat!(
            "SHAPE ERROR (E_HEADERS)\n\n",
            "Compared: {} -> {}\n\n",
            "Missing header or duplicate headers\n",
            "Next: fix headers or re-export.\n"
        ),
        old, new
    );

    assert_eq!(result.stderr, expected);
}

#[test]
fn human_refusal_dialect_with_next_command_matches_golden_snapshot() {
    let result = assert_human_stderr_only(
        DIALECT_AMBIGUOUS,
        BASIC_NEW,
        &[],
        2,
        "human dialect refusal output",
    );

    let old = fixture_path(DIALECT_AMBIGUOUS).display().to_string();
    let new = fixture_path(BASIC_NEW).display().to_string();
    let expected = format!(
        concat!(
            "SHAPE ERROR (E_DIALECT)\n\n",
            "Compared: {} -> {}\n\n",
            "Delimiter ambiguous or undetectable\n",
            "Next: shape {} {} --delimiter comma --json\n"
        ),
        old, new, old, new
    );

    assert_eq!(result.stderr, expected);
}

#[test]
fn json_mode_routes_all_domain_outcomes_to_stdout() {
    assert_json_domain_stdout_only(BASIC_OLD, BASIC_NEW, 0, "json compatible");
    assert_json_domain_stdout_only(TYPE_SHIFT_OLD, TYPE_SHIFT_NEW, 1, "json incompatible");
    assert_json_domain_stdout_only(BASIC_OLD, EMPTY, 2, "json refusal");
}
