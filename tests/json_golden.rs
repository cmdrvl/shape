mod helpers;

use std::path::PathBuf;

use serde_json::{Value, json};

use helpers::{ShapeInvocation, fixture_path, run_shape_with_fixtures};

const BASIC_OLD: &str = "basic_old.csv";
const BASIC_NEW: &str = "basic_new.csv";
const AMBIGUOUS_OLD: &str = "ambiguous_old.csv";
const TYPE_SHIFT_OLD: &str = "type_shift_old.csv";
const TYPE_SHIFT_NEW: &str = "type_shift_new.csv";
const NO_HEADER: &str = "no_header.csv";
const KEY_LOAN_ID: &str = "loan_id";

fn parse_json_output(output: &str) -> Value {
    serde_json::from_str(output).expect("shape --json output should be valid JSON")
}

fn assert_json_mode_routing(result: &ShapeInvocation, expected_status: i32) {
    assert_eq!(result.status, expected_status);
    assert!(
        result.stderr.trim().is_empty(),
        "unexpected stderr: {}",
        result.stderr
    );
}

fn fixture_pair(old_fixture: &str, new_fixture: &str) -> (PathBuf, PathBuf) {
    (fixture_path(old_fixture), fixture_path(new_fixture))
}

#[test]
fn json_snapshot_compatible_without_key() {
    let (old, new) = fixture_pair(BASIC_OLD, BASIC_NEW);
    let result = run_shape_with_fixtures(BASIC_OLD, BASIC_NEW, &["--json"]);

    assert_json_mode_routing(&result, 0);

    let expected = json!({
        "version": "shape.v0",
        "outcome": "COMPATIBLE",
        "profile_id": null,
        "profile_sha256": null,
        "input_verification": null,
        "files": {
            "old": old.to_string_lossy(),
            "new": new.to_string_lossy(),
        },
        "dialect": {
            "old": {"delimiter": ",", "quote": "\"", "escape": "none"},
            "new": {"delimiter": ",", "quote": "\"", "escape": "none"},
        },
        "checks": {
            "schema_overlap": {
                "status": "pass",
                "columns_common": 3,
                "columns_old_only": [],
                "columns_new_only": [],
                "overlap_ratio": 1.0
            },
            "key_viability": null,
            "row_granularity": {
                "status": "pass",
                "rows_old": 3,
                "rows_new": 3,
                "key_overlap": null,
                "keys_old_only": null,
                "keys_new_only": null
            },
            "type_consistency": {
                "status": "pass",
                "numeric_columns": 1,
                "type_shifts": []
            }
        },
        "reasons": [],
        "refusal": null
    });

    assert_eq!(parse_json_output(&result.stdout), expected);
}

#[test]
fn json_snapshot_compatible_with_key() {
    let (old, new) = fixture_pair(BASIC_OLD, BASIC_NEW);
    let result = run_shape_with_fixtures(BASIC_OLD, BASIC_NEW, &["--json", "--key", KEY_LOAN_ID]);

    assert_json_mode_routing(&result, 0);

    let expected = json!({
        "version": "shape.v0",
        "outcome": "COMPATIBLE",
        "profile_id": null,
        "profile_sha256": null,
        "input_verification": null,
        "files": {
            "old": old.to_string_lossy(),
            "new": new.to_string_lossy(),
        },
        "dialect": {
            "old": {"delimiter": ",", "quote": "\"", "escape": "none"},
            "new": {"delimiter": ",", "quote": "\"", "escape": "none"},
        },
        "checks": {
            "schema_overlap": {
                "status": "pass",
                "columns_common": 3,
                "columns_old_only": [],
                "columns_new_only": [],
                "overlap_ratio": 1.0
            },
            "key_viability": {
                "status": "pass",
                "key_column": "u8:loan_id",
                "found_old": true,
                "found_new": true,
                "unique_old": true,
                "unique_new": true,
                "coverage": 0.6666666666666666
            },
            "row_granularity": {
                "status": "pass",
                "rows_old": 3,
                "rows_new": 3,
                "key_overlap": 2,
                "keys_old_only": 1,
                "keys_new_only": 1
            },
            "type_consistency": {
                "status": "pass",
                "numeric_columns": 1,
                "type_shifts": []
            }
        },
        "reasons": [],
        "refusal": null
    });

    assert_eq!(parse_json_output(&result.stdout), expected);
}

#[test]
fn json_snapshot_incompatible_type_shift() {
    let (old, new) = fixture_pair(TYPE_SHIFT_OLD, TYPE_SHIFT_NEW);
    let result = run_shape_with_fixtures(
        TYPE_SHIFT_OLD,
        TYPE_SHIFT_NEW,
        &["--json", "--key", KEY_LOAN_ID],
    );

    assert_json_mode_routing(&result, 1);

    let expected = json!({
        "version": "shape.v0",
        "outcome": "INCOMPATIBLE",
        "profile_id": null,
        "profile_sha256": null,
        "input_verification": null,
        "files": {
            "old": old.to_string_lossy(),
            "new": new.to_string_lossy(),
        },
        "dialect": {
            "old": {"delimiter": ",", "quote": "\"", "escape": "none"},
            "new": {"delimiter": ",", "quote": "\"", "escape": "none"},
        },
        "checks": {
            "schema_overlap": {
                "status": "pass",
                "columns_common": 3,
                "columns_old_only": [],
                "columns_new_only": [],
                "overlap_ratio": 1.0
            },
            "key_viability": {
                "status": "pass",
                "key_column": "u8:loan_id",
                "found_old": true,
                "found_new": true,
                "unique_old": true,
                "unique_new": true,
                "coverage": 1.0
            },
            "row_granularity": {
                "status": "pass",
                "rows_old": 2,
                "rows_new": 2,
                "key_overlap": 2,
                "keys_old_only": 0,
                "keys_new_only": 0
            },
            "type_consistency": {
                "status": "fail",
                "numeric_columns": 0,
                "type_shifts": [{
                    "column": "u8:balance",
                    "old_type": "numeric",
                    "new_type": "non-numeric"
                }]
            }
        },
        "reasons": ["Type shift: balance changed from numeric to non-numeric"],
        "refusal": null
    });

    assert_eq!(parse_json_output(&result.stdout), expected);
}

#[test]
fn json_snapshot_refusal_with_partial_context() {
    let (old, new) = fixture_pair(BASIC_OLD, NO_HEADER);
    let result = run_shape_with_fixtures(BASIC_OLD, NO_HEADER, &["--json"]);

    assert_json_mode_routing(&result, 2);

    let expected = json!({
        "version": "shape.v0",
        "outcome": "REFUSAL",
        "profile_id": null,
        "profile_sha256": null,
        "input_verification": null,
        "files": {
            "old": old.to_string_lossy(),
            "new": new.to_string_lossy(),
        },
        "dialect": {
            "old": {"delimiter": ",", "quote": "\"", "escape": "none"},
            "new": null,
        },
        "checks": null,
        "reasons": null,
        "refusal": {
            "code": "E_HEADERS",
            "message": "Missing header or duplicate headers",
            "detail": {
                "file": new.to_string_lossy(),
                "issue": "missing",
            },
            "next_command": null
        }
    });

    assert_eq!(parse_json_output(&result.stdout), expected);
}

#[test]
fn json_snapshot_refusal_without_dialect_context() {
    let (old, new) = fixture_pair(NO_HEADER, BASIC_NEW);
    let result = run_shape_with_fixtures(NO_HEADER, BASIC_NEW, &["--json"]);

    assert_json_mode_routing(&result, 2);

    let expected = json!({
        "version": "shape.v0",
        "outcome": "REFUSAL",
        "profile_id": null,
        "profile_sha256": null,
        "input_verification": null,
        "files": {
            "old": old.to_string_lossy(),
            "new": new.to_string_lossy(),
        },
        "dialect": {
            "old": null,
            "new": null,
        },
        "checks": null,
        "reasons": null,
        "refusal": {
            "code": "E_HEADERS",
            "message": "Missing header or duplicate headers",
            "detail": {
                "file": old.to_string_lossy(),
                "issue": "missing",
            },
            "next_command": null
        }
    });

    assert_eq!(parse_json_output(&result.stdout), expected);
}

#[test]
fn json_snapshot_refusal_ambiguous_dialect_includes_next_command() {
    let (old, new) = fixture_pair(AMBIGUOUS_OLD, BASIC_NEW);
    let result = run_shape_with_fixtures(AMBIGUOUS_OLD, BASIC_NEW, &["--json"]);

    assert_json_mode_routing(&result, 2);

    let expected = json!({
        "version": "shape.v0",
        "outcome": "REFUSAL",
        "profile_id": null,
        "profile_sha256": null,
        "input_verification": null,
        "files": {
            "old": old.to_string_lossy(),
            "new": new.to_string_lossy(),
        },
        "dialect": {
            "old": null,
            "new": null,
        },
        "checks": null,
        "reasons": null,
        "refusal": {
            "code": "E_DIALECT",
            "message": "Delimiter ambiguous or undetectable",
            "detail": {
                "file": old.to_string_lossy(),
                "candidates": ["0x2c", "0x3b"],
            },
            "next_command": format!(
                "shape {} {} --delimiter comma --json",
                old.to_string_lossy(),
                new.to_string_lossy()
            ),
        }
    });

    assert_eq!(parse_json_output(&result.stdout), expected);
}
