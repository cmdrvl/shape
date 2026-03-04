mod helpers;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use helpers::{fixture_path, run_shape_with_fixtures};

const BASIC_OLD: &str = "basic_old.csv";
const BASIC_NEW: &str = "basic_new.csv";
const RENT_ROLL_OLD: &str = "rent_roll_old.csv";
const RENT_ROLL_NEW: &str = "rent_roll_new.csv";
const PROFILE_LOAN_TAPE: &str = "profile_loan_tape.yaml";
const PROFILE_RENT_ROLL: &str = "profile_rent_roll.yaml";
#[allow(dead_code)]
const PROFILE_DRAFT: &str = "profile_draft.yaml";

fn parse_json_output(output: &str) -> Value {
    serde_json::from_str(output).expect("shape --json output should be valid JSON")
}

fn profile_arg(profile_fixture: &str) -> String {
    fixture_path(profile_fixture).to_string_lossy().into_owned()
}

// ---------------------------------------------------------------------------
// 1. golden_json_profile_scoped_overlap
// ---------------------------------------------------------------------------

#[test]
fn golden_json_profile_scoped_overlap() {
    let (old, new) = (fixture_path(BASIC_OLD), fixture_path(BASIC_NEW));
    let profile = profile_arg(PROFILE_LOAN_TAPE);
    let result = run_shape_with_fixtures(BASIC_OLD, BASIC_NEW, &["--json", "--profile", &profile]);

    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "unexpected stderr: {}",
        result.stderr
    );

    let expected = json!({
        "version": "shape.v0",
        "outcome": "COMPATIBLE",
        "profile_id": "loan-tape.v0",
        "profile_sha256": "sha256:test-loan-tape",
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
                "columns_common": 2,
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

// ---------------------------------------------------------------------------
// 2. golden_json_profile_composite_key
// ---------------------------------------------------------------------------

#[test]
fn golden_json_profile_composite_key() {
    let (old, new) = (fixture_path(RENT_ROLL_OLD), fixture_path(RENT_ROLL_NEW));
    let profile = profile_arg(PROFILE_RENT_ROLL);
    let result = run_shape_with_fixtures(
        RENT_ROLL_OLD,
        RENT_ROLL_NEW,
        &["--json", "--profile", &profile],
    );

    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "unexpected stderr: {}",
        result.stderr
    );

    let expected = json!({
        "version": "shape.v0",
        "outcome": "COMPATIBLE",
        "profile_id": "rent-roll.v0",
        "profile_sha256": "sha256:test-rent-roll",
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
                "key_column": "u8:unit_id + u8:building",
                "key_columns": ["u8:unit_id", "u8:building"],
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
                "numeric_columns": 2,
                "type_shifts": []
            }
        },
        "reasons": [],
        "refusal": null
    });

    assert_eq!(parse_json_output(&result.stdout), expected);
}

// ---------------------------------------------------------------------------
// 3. golden_human_profile_scoped_output
// ---------------------------------------------------------------------------

#[test]
fn golden_human_profile_scoped_output() {
    let profile = profile_arg(PROFILE_LOAN_TAPE);
    let result = run_shape_with_fixtures(BASIC_OLD, BASIC_NEW, &["--profile", &profile]);

    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "unexpected stderr: {}",
        result.stderr
    );

    let expected = format!(
        concat!(
            "SHAPE\n\n",
            "COMPATIBLE\n\n",
            "Compared: {} -> {}\n",
            "Key: loan_id (unique in both files)\n",
            "Dialect(old): delimiter=, quote=\" escape=none\n",
            "Dialect(new): delimiter=, quote=\" escape=none\n\n",
            "Schema:    2 common / 2 total (100% overlap)\n",
            "Key:       loan_id — unique in both, coverage=0.67\n",
            "Rows:      3 old / 3 new (1 removed, 1 added, 2 overlap)\n",
            "Types:     1 numeric columns, 0 type shifts\n"
        ),
        fixture_path(BASIC_OLD).display(),
        fixture_path(BASIC_NEW).display()
    );

    assert_eq!(result.stdout, expected);
}

// ---------------------------------------------------------------------------
// 4. profile_with_explicit_flag
// ---------------------------------------------------------------------------

#[test]
fn profile_with_explicit_flag() {
    let profile = profile_arg(PROFILE_LOAN_TAPE);
    let result = run_shape_with_fixtures(
        BASIC_OLD,
        BASIC_NEW,
        &["--json", "--explicit", "--profile", &profile],
    );

    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "unexpected stderr: {}",
        result.stderr
    );

    let value = parse_json_output(&result.stdout);
    assert_eq!(value["outcome"], "COMPATIBLE");
    assert_eq!(value["profile_id"], "loan-tape.v0");
    assert_eq!(value["profile_sha256"], "sha256:test-loan-tape");
    assert_eq!(value["checks"]["schema_overlap"]["columns_common"], 2);
    assert_eq!(value["checks"]["schema_overlap"]["overlap_ratio"], 1.0);
    assert_eq!(value["checks"]["key_viability"]["key_column"], "u8:loan_id");
}

// ---------------------------------------------------------------------------
// 5. profile_with_redacted_default
// ---------------------------------------------------------------------------

#[test]
fn profile_with_redacted_default() {
    let profile = profile_arg(PROFILE_LOAN_TAPE);
    let result = run_shape_with_fixtures(BASIC_OLD, BASIC_NEW, &["--json", "--profile", &profile]);

    assert_eq!(result.status, 0);

    let value = parse_json_output(&result.stdout);
    // Profile metadata is visible even in redacted mode.
    assert_eq!(value["profile_id"], "loan-tape.v0");
    assert_eq!(value["profile_sha256"], "sha256:test-loan-tape");
    // Column names remain redacted by default (old_only/new_only are empty here,
    // but the key_column still uses the encode_identifier prefix).
    assert_eq!(value["checks"]["key_viability"]["key_column"], "u8:loan_id");
    // Overlap is profile-scoped.
    assert_eq!(value["checks"]["schema_overlap"]["columns_common"], 2);
}

// ---------------------------------------------------------------------------
// 6. capsule_replay_includes_profile_flags
// ---------------------------------------------------------------------------

fn unique_capsule_dir(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "shape-profile-capsule-{label}-{}-{counter}-{nanos}",
        std::process::id(),
    ))
}

#[test]
fn capsule_replay_includes_profile_flags() {
    let capsule_dir = unique_capsule_dir("profile-replay");
    let profile = profile_arg(PROFILE_LOAN_TAPE);
    let result = run_shape_with_fixtures(
        BASIC_OLD,
        BASIC_NEW,
        &[
            "--json",
            "--no-witness",
            "--capsule-dir",
            &capsule_dir.to_string_lossy(),
            "--profile",
            &profile,
        ],
    );

    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "unexpected stderr: {}",
        result.stderr
    );

    let manifest_text = fs::read_to_string(capsule_dir.join("manifest.json"))
        .expect("capsule manifest should be written");
    let manifest: Value =
        serde_json::from_str(&manifest_text).expect("capsule manifest should parse");

    // Manifest records the profile arg.
    assert_eq!(
        manifest["args"]["profile"].as_str(),
        Some(profile.as_str()),
        "capsule manifest should record the --profile path"
    );

    // Replay argv includes --profile.
    let replay_argv: Vec<&str> = manifest["replay"]["argv"]
        .as_array()
        .expect("replay argv should be array")
        .iter()
        .map(|v| v.as_str().expect("argv element should be string"))
        .collect();

    assert!(
        replay_argv.contains(&"--profile"),
        "replay argv should include --profile flag: {replay_argv:?}"
    );
    assert!(
        replay_argv.contains(&profile.as_str()),
        "replay argv should include the profile path: {replay_argv:?}"
    );

    // Replay shell string includes --profile.
    let replay_shell = manifest["replay"]["shell"]
        .as_str()
        .expect("replay shell should be a string");
    assert!(
        replay_shell.contains("--profile"),
        "replay shell command should include --profile: {replay_shell}"
    );

    let _ = fs::remove_dir_all(capsule_dir);
}
