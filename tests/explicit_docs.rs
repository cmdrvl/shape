mod helpers;

use helpers::{fixture_path, run_shape};
use serde_json::Value;

fn fixture_args(old: &str, new: &str, extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        fixture_path(old).to_string_lossy().into_owned(),
        fixture_path(new).to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|arg| (*arg).to_owned()));
    args
}

#[test]
fn help_documents_default_redaction_and_explicit() {
    let result = run_shape(["--help"]);

    assert_eq!(result.status, 0);
    assert!(result.stdout.contains("--explicit"));
    assert!(result.stdout.contains("redacted by default"));
    assert!(result.stdout.contains("NOT yet redacted"));
    assert!(result.stdout.contains("bd-pitd"));
    assert!(
        result
            .stdout
            .contains("Do not treat default output as zero-retention safe yet"),
        "help must not claim an unqualified zero-retention guarantee"
    );
}

#[test]
fn robot_docs_guide_documents_default_redaction_and_explicit() {
    let result = run_shape(["robot-docs", "guide"]);

    assert_eq!(result.status, 0);
    assert!(result.stdout.contains("Redaction (--explicit):"));
    assert!(result.stdout.contains("redacted by default"));
    assert!(result.stdout.contains("NOT yet redacted"));
    assert!(result.stdout.contains("bd-pitd"));
    assert!(!result.stdout.contains("redacted for zero-retention safety"));
    assert!(result.stdout.contains("--explicit"));
    assert!(
        result.stdout.contains("`reasons`"),
        "guide must warn that reasons text is not redacted"
    );
}

#[test]
fn readme_documents_default_redaction_and_explicit() {
    let readme = include_str!("../README.md");

    assert!(readme.contains("Redacted by default (partially)"));
    assert!(readme.contains("Not yet redacted"));
    assert!(readme.contains("bd-pitd"));
    assert!(
        !readme.contains("zero-retention safe out of the box"),
        "README must not claim an unqualified zero-retention guarantee"
    );
    assert!(readme.contains("| `--explicit` |"));
    assert!(readme.contains("shape nov.csv dec.csv --key loan_id --explicit"));
}

/// The documented behaviour must be the real behaviour.
#[test]
fn documented_redaction_matches_json_output() {
    let redacted = run_shape(fixture_args(
        "type_shift_old.csv",
        "type_shift_new.csv",
        &["--json", "--no-witness"],
    ));
    let explicit = run_shape(fixture_args(
        "type_shift_old.csv",
        "type_shift_new.csv",
        &["--json", "--no-witness", "--explicit"],
    ));

    let redacted: Value = serde_json::from_str(&redacted.stdout).expect("default JSON parses");
    let explicit: Value = serde_json::from_str(&explicit.stdout).expect("explicit JSON parses");

    assert_eq!(
        redacted["checks"]["type_consistency"]["type_shifts"][0]["column"],
        "[REDACTED]"
    );
    assert_eq!(
        explicit["checks"]["type_consistency"]["type_shifts"][0]["column"],
        "u8:balance"
    );
}

#[test]
fn documented_redaction_matches_human_schema_lists() {
    let redacted = run_shape(fixture_args(
        "schema_drift_old.csv",
        "schema_drift_new.csv",
        &["--no-witness"],
    ));
    let explicit = run_shape(fixture_args(
        "schema_drift_old.csv",
        "schema_drift_new.csv",
        &["--no-witness", "--explicit"],
    ));

    assert!(!redacted.stdout.contains("retired_field"));
    assert!(!redacted.stdout.contains("old_only"));
    assert!(explicit.stdout.contains("old_only: [retired_field]"));
    assert!(explicit.stdout.contains("new_only: [new_field]"));
}
