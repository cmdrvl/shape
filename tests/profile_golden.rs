mod helpers;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use helpers::{fixture_path, run_shape, run_shape_with_env, run_shape_with_fixtures};

const BASIC_OLD: &str = "basic_old.csv";
const BASIC_NEW: &str = "basic_new.csv";
const RENT_ROLL_OLD: &str = "rent_roll_old.csv";
const RENT_ROLL_NEW: &str = "rent_roll_new.csv";
const ANNEX_OLD: &str = "annex_old.csv";
const ANNEX_NEW: &str = "annex_new.csv";
const PROFILE_LOAN_TAPE: &str = "profile_loan_tape.yaml";
const PROFILE_RENT_ROLL: &str = "profile_rent_roll.yaml";
const PROFILE_ANNEX_REGISTRY: &str = "profile_annex_registry.yaml";
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

#[test]
fn schema_contract_describes_composite_key_columns() {
    let schema_result = run_shape(["--schema"]);
    assert_eq!(schema_result.status, 0);
    assert!(
        schema_result.stderr.trim().is_empty(),
        "unexpected schema stderr: {}",
        schema_result.stderr
    );
    let schema = parse_json_output(&schema_result.stdout);
    let key_columns_schema =
        &schema["properties"]["checks"]["properties"]["key_viability"]["properties"]["key_columns"];
    assert_eq!(key_columns_schema["type"], "array");
    assert_eq!(key_columns_schema["items"]["type"], "string");

    let profile = profile_arg(PROFILE_RENT_ROLL);
    let result = run_shape_with_fixtures(
        RENT_ROLL_OLD,
        RENT_ROLL_NEW,
        &["--json", "--profile", &profile],
    );
    assert_eq!(result.status, 0);
    let payload = parse_json_output(&result.stdout);
    let key_columns = payload["checks"]["key_viability"]["key_columns"]
        .as_array()
        .expect("composite key payload should include key_columns");

    assert_eq!(key_columns.len(), 2);
    assert!(
        key_columns.iter().all(Value::is_string),
        "key_columns should satisfy emitted string-array schema: {key_columns:?}"
    );
}

#[test]
fn json_profile_key_override_suppresses_domain_warning() {
    let profile = profile_arg(PROFILE_LOAN_TAPE);
    let result = run_shape_with_fixtures(
        BASIC_OLD,
        BASIC_NEW,
        &["--json", "--profile", &profile, "--key", "balance"],
    );

    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "json domain diagnostics must not be written to stderr: {}",
        result.stderr
    );
    let payload = parse_json_output(&result.stdout);
    assert_eq!(payload["outcome"], "COMPATIBLE");
    assert_eq!(
        payload["checks"]["key_viability"]["key_column"],
        "u8:loan_id"
    );
}

#[test]
fn profile_composite_key_uses_structural_tuples_and_requires_every_component() {
    let workspace = unique_capsule_dir("profile-composite-structure");
    fs::create_dir_all(&workspace).expect("create workspace");
    let old_path = workspace.join("old.csv");
    let new_path = workspace.join("new.csv");
    let profile_path = workspace.join("profile.yaml");
    fs::write(
        &profile_path,
        "profile_id: composite.v0\nprofile_sha256: sha256:test-composite\ninclude_columns:\n  - first\n  - second\n  - amount\nkey:\n  - first\n  - second\n",
    )
    .expect("write profile");

    let distinct_tuples =
        b"first,second,amount\na\xff,b,1\na,\xffb,2\nrepeat,one,3\nrepeat,two,4\nNA,NULL,5\n";
    fs::write(&old_path, distinct_tuples).expect("write collision-resistant old fixture");
    fs::write(&new_path, distinct_tuples).expect("write collision-resistant new fixture");

    let old = old_path.to_string_lossy().into_owned();
    let new = new_path.to_string_lossy().into_owned();
    let profile = profile_path.to_string_lossy().into_owned();
    let compatible = run_shape([
        old.as_str(),
        new.as_str(),
        "--delimiter",
        "comma",
        "--json",
        "--no-witness",
        "--profile",
        profile.as_str(),
    ]);
    let payload = parse_json_output(&compatible.stdout);
    assert_eq!(compatible.status, 0);
    assert_eq!(payload["outcome"], "COMPATIBLE");
    assert_eq!(payload["checks"]["key_viability"]["unique_old"], true);
    assert_eq!(payload["checks"]["key_viability"]["unique_new"], true);
    assert_eq!(payload["checks"]["row_granularity"]["key_overlap"], 5);

    fs::write(&old_path, b"first,second,amount\nA,1,10\nA,1,20\nA,2,30\n")
        .expect("write duplicate tuple fixture");
    fs::write(&new_path, b"first,second,amount\nA,1,10\nA,2,20\nA,3,30\n")
        .expect("write unique tuple fixture");
    let duplicate = run_shape([
        old.as_str(),
        new.as_str(),
        "--delimiter",
        "comma",
        "--json",
        "--no-witness",
        "--profile",
        profile.as_str(),
    ]);
    let payload = parse_json_output(&duplicate.stdout);
    assert_eq!(duplicate.status, 1);
    assert_eq!(payload["outcome"], "INCOMPATIBLE");
    assert_eq!(payload["checks"]["key_viability"]["unique_old"], false);
    assert!(
        payload["reasons"][0]
            .as_str()
            .is_some_and(|reason| reason.contains("1 duplicate value in old file"))
    );

    fs::write(&old_path, b"first,second,amount\nA,,10\nX,Y,20\n")
        .expect("write incomplete tuple fixture");
    fs::write(&new_path, b"first,second,amount\nA,B,10\nX,Y,20\n")
        .expect("write complete tuple fixture");
    let incomplete = run_shape([
        old.as_str(),
        new.as_str(),
        "--delimiter",
        "comma",
        "--json",
        "--no-witness",
        "--profile",
        profile.as_str(),
    ]);
    let payload = parse_json_output(&incomplete.stdout);
    assert_eq!(incomplete.status, 1);
    assert_eq!(payload["outcome"], "INCOMPATIBLE");
    assert_eq!(payload["checks"]["key_viability"]["unique_old"], false);
    assert!(
        payload["reasons"][0]
            .as_str()
            .is_some_and(|reason| reason.contains("1 empty value in old file"))
    );

    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn profile_composite_key_missing_component_nulls_row_key_metrics() {
    let workspace = unique_capsule_dir("profile-composite-missing-component");
    fs::create_dir_all(&workspace).expect("create workspace");
    let old_path = workspace.join("old.csv");
    let new_path = workspace.join("new.csv");
    let profile_path = workspace.join("profile.yaml");
    fs::write(
        &profile_path,
        "profile_id: composite.v0\nprofile_sha256: sha256:test-composite\ninclude_columns:\n  - first\n  - second\n  - amount\nkey:\n  - first\n  - second\n",
    )
    .expect("write profile");
    fs::write(&old_path, b"first,second,amount\nA,1,10\nB,2,20\n").expect("write old fixture");
    fs::write(&new_path, b"first,amount\nA,10\nC,30\n").expect("write new fixture");

    let old = old_path.to_string_lossy().into_owned();
    let new = new_path.to_string_lossy().into_owned();
    let profile = profile_path.to_string_lossy().into_owned();
    let result = run_shape([
        old.as_str(),
        new.as_str(),
        "--delimiter",
        "comma",
        "--json",
        "--no-witness",
        "--profile",
        profile.as_str(),
    ]);

    assert_eq!(result.status, 1);
    assert!(
        result.stderr.trim().is_empty(),
        "unexpected stderr: {}",
        result.stderr
    );
    let payload = parse_json_output(&result.stdout);
    assert_eq!(payload["outcome"], "INCOMPATIBLE");
    assert_eq!(payload["checks"]["key_viability"]["found_old"], true);
    assert_eq!(payload["checks"]["key_viability"]["found_new"], false);
    assert_eq!(payload["checks"]["key_viability"]["unique_old"], true);
    assert!(payload["checks"]["key_viability"]["unique_new"].is_null());
    assert!(payload["checks"]["key_viability"]["coverage"].is_null());
    assert!(payload["checks"]["row_granularity"]["key_overlap"].is_null());
    assert!(payload["checks"]["row_granularity"]["keys_old_only"].is_null());
    assert!(payload["checks"]["row_granularity"]["keys_new_only"].is_null());
    assert!(
        payload["reasons"]
            .as_array()
            .expect("reasons array")
            .iter()
            .any(|reason| reason.as_str() == Some("Key viability: second not found in new file")),
        "missing component reason should name second: {}",
        payload["reasons"]
    );

    let _ = fs::remove_dir_all(workspace);
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
// 6. profile_id_migrates_legacy_default_profile_dir
// ---------------------------------------------------------------------------

#[test]
fn profile_id_migrates_legacy_default_profile_dir() {
    let workspace = unique_capsule_dir("profile-id-migration");
    let home = workspace.join("home");
    let legacy_profiles = home.join(".epistemic").join("profiles");
    fs::create_dir_all(&legacy_profiles).expect("create legacy profile directory");
    fs::write(
        legacy_profiles.join("loan.yaml"),
        "profile_id: loan-tape.v0\nprofile_sha256: sha256:test-loan-tape\ninclude_columns:\n  - loan_id\n  - balance\nkey:\n  - loan_id\n",
    )
    .expect("write legacy profile");

    let old = fixture_path(BASIC_OLD).to_string_lossy().into_owned();
    let new = fixture_path(BASIC_NEW).to_string_lossy().into_owned();
    let home_str = home.to_string_lossy().into_owned();
    let result = run_shape_with_env(
        &[
            old.as_str(),
            new.as_str(),
            "--json",
            "--no-witness",
            "--profile-id",
            "loan-tape.v0",
        ],
        &[("HOME", home_str.as_str())],
    );

    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "unexpected stderr: {}",
        result.stderr
    );

    let payload = parse_json_output(&result.stdout);
    assert_eq!(payload["profile_id"], "loan-tape.v0");
    assert!(
        home.join(".cmdrvl/config/profile/profiles/loan.yaml")
            .is_file()
    );

    let migration = fs::read_to_string(home.join(".cmdrvl/migrations/applied.jsonl"))
        .expect("profile migration record should exist");
    assert!(migration.contains("\"path_class\":\"profile_profiles\""));

    let _ = fs::remove_dir_all(workspace);
}

// ---------------------------------------------------------------------------
// 7. golden_json_profile_column_registry_support
// ---------------------------------------------------------------------------

#[test]
fn golden_json_profile_column_registry_support() {
    let (old, new) = (fixture_path(ANNEX_OLD), fixture_path(ANNEX_NEW));
    let profile = profile_arg(PROFILE_ANNEX_REGISTRY);
    let result = run_shape_with_fixtures(ANNEX_OLD, ANNEX_NEW, &["--json", "--profile", &profile]);

    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "unexpected stderr: {}",
        result.stderr
    );

    let expected = json!({
        "version": "shape.v0",
        "outcome": "COMPATIBLE",
        "profile_id": "annex-columns.v0",
        "profile_sha256": "sha256:test-annex-columns",
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
                "key_column": "u8:loan_id_number",
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
// 7. profile_column_registry_collision_refuses
// ---------------------------------------------------------------------------

#[test]
fn profile_column_registry_canonicalizes_cli_key_flag() {
    let workspace = unique_capsule_dir("profile-cli-key");
    let profile_path = workspace.join("profile.yaml");
    fs::create_dir_all(&workspace).expect("create workspace");
    fs::write(
        &profile_path,
        format!(
            "profile_id: annex-columns.v0\nprofile_sha256: sha256:test-annex-columns\ncolumn_registry: {}\ninclude_columns:\n  - loan_id_number\n  - current_balance\n  - note_rate\n",
            fixture_path("registries/annex_columns_v0").to_string_lossy()
        ),
    )
    .expect("write profile");

    let result = run_shape_with_fixtures(
        ANNEX_OLD,
        ANNEX_NEW,
        &[
            "--json",
            "--profile",
            &profile_path.to_string_lossy(),
            "--key",
            "loan_id_number",
        ],
    );

    assert_eq!(result.status, 0);
    let payload = parse_json_output(&result.stdout);
    assert_eq!(payload["outcome"], "COMPATIBLE");
    assert_eq!(
        payload["checks"]["key_viability"]["key_column"],
        "u8:loan_id_number"
    );
    assert_eq!(payload["checks"]["key_viability"]["found_old"], true);
    assert_eq!(payload["checks"]["key_viability"]["found_new"], true);

    let _ = fs::remove_dir_all(workspace);
}

// ---------------------------------------------------------------------------
// 8. profile_column_registry_collision_refuses
// ---------------------------------------------------------------------------

#[test]
fn profile_column_registry_collision_refuses() {
    let workspace = unique_capsule_dir("profile-collision");
    let registry_dir = workspace.join("registries").join("annex_columns_v0");
    fs::create_dir_all(&registry_dir).expect("create registry dir");
    fs::write(
        registry_dir.join("registry.json"),
        r#"{"id":"annex-columns-v0","version":"1.0.0"}"#,
    )
    .expect("write registry");
    fs::write(
        registry_dir.join("aliases.json"),
        r#"
[
  {
    "input": "Loan Number",
    "canonical_id": "loan_id_number",
    "canonical_type": "column_name",
    "rule_id": "ANNEX_COLUMN_ALIAS"
  },
  {
    "input": "Loan ID Number",
    "canonical_id": "loan_id_number",
    "canonical_type": "column_name",
    "rule_id": "ANNEX_COLUMN_ALIAS"
  }
]
"#,
    )
    .expect("write aliases");
    fs::write(
        workspace.join("old.csv"),
        "Loan Number,Loan ID Number\nA1,A1\nA2,A2\n",
    )
    .expect("write old");
    fs::write(workspace.join("new.csv"), "Loan ID Number\nA1\nA2\n").expect("write new");
    fs::write(
        workspace.join("profile.yaml"),
        "profile_id: annex-columns.v0\nprofile_sha256: sha256:test-annex-columns\ncolumn_registry: registries/annex_columns_v0\ninclude_columns:\n  - loan_id_number\nkey:\n  - loan_id_number\n",
    )
    .expect("write profile");

    let old = workspace.join("old.csv").to_string_lossy().into_owned();
    let new = workspace.join("new.csv").to_string_lossy().into_owned();
    let profile = workspace
        .join("profile.yaml")
        .to_string_lossy()
        .into_owned();
    let result = run_shape([
        old.as_str(),
        new.as_str(),
        "--delimiter",
        "comma",
        "--json",
        "--profile",
        profile.as_str(),
    ]);

    assert_eq!(result.status, 2);
    assert!(
        result.stderr.trim().is_empty(),
        "refusal in --json mode should stay on stdout: {}",
        result.stderr
    );

    let payload = parse_json_output(&result.stdout);
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_HEADERS");
    assert_eq!(payload["refusal"]["detail"]["issue"], "duplicate");
    assert_eq!(payload["refusal"]["detail"]["name"], "u8:loan_id_number");
    assert_eq!(payload["refusal"]["detail"]["file"], old);

    let _ = fs::remove_dir_all(workspace);
}

// ---------------------------------------------------------------------------
// 9. capsule_replay_records_profile_arg_but_replays_from_local_artifact
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
fn capsule_replay_records_profile_arg_but_replays_from_local_artifact() {
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

    assert!(
        capsule_dir.join("profile.yaml").is_file(),
        "capsule should include a local profile artifact"
    );

    // Replay argv includes the local profile artifact, not the original path.
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
        replay_argv.contains(&"profile.yaml"),
        "replay argv should include the local profile artifact: {replay_argv:?}"
    );
    assert!(
        !replay_argv.contains(&profile.as_str()),
        "replay argv should not depend on the original profile path: {replay_argv:?}"
    );

    // Replay shell string includes --profile.
    let replay_shell = manifest["replay"]["shell"]
        .as_str()
        .expect("replay shell should be a string");
    assert!(
        replay_shell.contains("--profile"),
        "replay shell command should include --profile: {replay_shell}"
    );
    assert!(
        replay_shell.contains("profile.yaml"),
        "replay shell command should use the local profile artifact: {replay_shell}"
    );

    let _ = fs::remove_dir_all(capsule_dir);
}

// ---------------------------------------------------------------------------
// 10. capsule_replay_with_column_registry_uses_local_registry_artifact
// ---------------------------------------------------------------------------

#[test]
fn capsule_replay_with_column_registry_uses_local_registry_artifact() {
    let capsule_dir = unique_capsule_dir("profile-registry-replay");
    let profile = profile_arg(PROFILE_ANNEX_REGISTRY);
    let result = run_shape_with_fixtures(
        ANNEX_OLD,
        ANNEX_NEW,
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
    assert!(capsule_dir.join("profile.yaml").is_file());
    assert!(
        capsule_dir
            .join("column_registry")
            .join("registry.json")
            .is_file()
    );
    assert!(
        capsule_dir
            .join("column_registry")
            .join("aliases.json")
            .is_file()
    );

    let profile_artifact = fs::read_to_string(capsule_dir.join("profile.yaml"))
        .expect("profile artifact should exist");
    assert!(profile_artifact.contains("column_registry: column_registry"));

    let replay_argv = manifest["replay"]["argv"]
        .as_array()
        .expect("replay argv should be array")
        .iter()
        .map(|value| value.as_str().expect("argv element should be string"))
        .collect::<Vec<_>>();
    assert!(replay_argv.contains(&"--profile"));
    assert!(replay_argv.contains(&"profile.yaml"));
    let replay = std::process::Command::new(env!("CARGO_BIN_EXE_shape"))
        .args(replay_argv.iter().skip(1))
        .current_dir(&capsule_dir)
        .env(
            "EPISTEMIC_WITNESS",
            std::env::temp_dir().join("shape-profile-registry-replay-witness.jsonl"),
        )
        .output()
        .expect("replay shape invocation should run");
    assert_eq!(replay.status.code(), Some(0));

    let _ = fs::remove_dir_all(capsule_dir);
}
