use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use shape::api::{EvaluateOptions, evaluate_bytes_with_options};

fn format_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/formats")
        .join(name)
}

fn cli_report(old: &Path, new: &Path, explicit: bool) -> (i32, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_shape"));
    command
        .arg(old)
        .arg(new)
        .args(["--key", "loan_id", "--json", "--no-witness"]);
    if explicit {
        command.arg("--explicit");
    }
    let output = command.output().expect("shape CLI should start");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8(output.stdout).expect("shape JSON should be UTF-8"),
    )
}

fn api_report(old: &Path, new: &Path, explicit: bool) -> Value {
    let options = EvaluateOptions {
        old_name_hint: Some(old.to_string_lossy().into_owned()),
        new_name_hint: Some(new.to_string_lossy().into_owned()),
        key_columns: vec![b"loan_id".to_vec()],
        explicit,
        ..EvaluateOptions::default()
    };
    evaluate_bytes_with_options(
        &std::fs::read(old).expect("old fixture should be readable"),
        &std::fs::read(new).expect("new fixture should be readable"),
        &options,
    )
    .expect("in-memory API should compare the fixture")
}

fn api_result(old: &Path, new: &Path, explicit: bool) -> Result<Value, Value> {
    let options = EvaluateOptions {
        old_name_hint: Some(old.to_string_lossy().into_owned()),
        new_name_hint: Some(new.to_string_lossy().into_owned()),
        key_columns: vec![b"loan_id".to_vec()],
        explicit,
        ..EvaluateOptions::default()
    };
    evaluate_bytes_with_options(
        &std::fs::read(old).expect("old fixture should be readable"),
        &std::fs::read(new).expect("new fixture should be readable"),
        &options,
    )
    .map_err(|refusal| serde_json::to_value(refusal).expect("refusal should serialize"))
}

#[test]
fn in_memory_api_json_is_byte_identical_to_cli_json_for_supported_formats() {
    for explicit in [false, true] {
        for old_ext in ["csv", "tsv", "jsonl", "parquet", "xlsx"] {
            for new_ext in ["csv", "tsv", "jsonl", "parquet", "xlsx"] {
                let old = format_fixture(&format!("old.{old_ext}"));
                let new = format_fixture(&format!("new.{new_ext}"));
                let (status, cli_stdout) = cli_report(&old, &new, explicit);
                assert_eq!(status, 0, "CLI refused {old_ext} -> {new_ext}");
                let api = api_report(&old, &new, explicit);
                let cli: Value = serde_json::from_str(&cli_stdout).expect("CLI JSON is valid");
                assert_eq!(api, cli, "API/CLI mismatch for {old_ext} -> {new_ext}");
                assert_eq!(
                    serde_json::to_string(&api).expect("API report serializes"),
                    cli_stdout.trim_end(),
                    "API report bytes differ for {old_ext} -> {new_ext}"
                );
            }
        }
    }
}

#[test]
fn api_refusal_is_typed_and_does_not_touch_the_filesystem() {
    let error = shape::api::evaluate_bytes(b"loan_id,balance\n", b"loan_id,balance\n")
        .expect_err("header-only bytes must refuse");
    assert_eq!(error.code.as_str(), "E_EMPTY");
    assert_eq!(error.detail["file"], "old.csv");
}

#[test]
fn in_memory_api_matches_cli_for_incompatible_and_refusal_goldens() {
    let incompatible = [
        ("type_shift_old.csv", "type_shift_new.csv"),
        ("dup_key_old.csv", "dup_key_new.csv"),
    ];
    let refusals = [
        ("basic_old.csv", "empty.csv"),
        ("basic_old.csv", "no_header.csv"),
        ("no_header.csv", "basic_new.csv"),
        ("dialect_ambiguous.csv", "basic_new.csv"),
    ];

    for explicit in [false, true] {
        for (old_name, new_name) in incompatible {
            let old = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(old_name);
            let new = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(new_name);
            let (status, cli_stdout) = cli_report(&old, &new, explicit);
            assert_eq!(status, 1, "CLI should report INCOMPATIBLE");
            let cli: Value = serde_json::from_str(&cli_stdout).expect("CLI JSON is valid");
            assert_eq!(cli["outcome"], "INCOMPATIBLE");

            let api = api_result(&old, &new, explicit)
                .expect("in-memory API should report incompatibility");
            assert_eq!(serde_json::to_string(&api).unwrap() + "\n", cli_stdout);
        }

        for (old_name, new_name) in refusals {
            let old = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(old_name);
            let new = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(new_name);
            let (status, cli_stdout) = cli_report(&old, &new, explicit);
            assert_eq!(status, 2, "CLI should refuse");
            let cli: Value = serde_json::from_str(&cli_stdout).expect("CLI refusal is JSON");
            assert_eq!(cli["outcome"], "REFUSAL");

            let api = api_result(&old, &new, explicit).expect_err("API should refuse");
            assert_eq!(api["code"], cli["refusal"]["code"]);
            assert_eq!(
                serde_json::to_string(&api).unwrap(),
                serde_json::to_string(&cli["refusal"]).unwrap(),
                "API refusal JSON differs from the CLI refusal object"
            );
        }
    }
}
