use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

#[derive(Debug)]
struct ShapeInvocation {
    status: i32,
    stdout: String,
    stderr: String,
}

fn run_shape_with_ledger<I, S>(args: I, ledger_path: &Path) -> ShapeInvocation
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(env!("CARGO_BIN_EXE_shape"));
    command.args(args);
    command.env("EPISTEMIC_WITNESS", ledger_path);

    shape_invocation_from_output(
        command
            .output()
            .expect("failed to execute shape binary for witness schema test"),
    )
}

fn shape_invocation_from_output(output: Output) -> ShapeInvocation {
    ShapeInvocation {
        status: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn unique_ledger_path(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();

    std::env::temp_dir().join(format!(
        "shape-witness-schema-{label}-{}-{counter}-{nanos}.jsonl",
        std::process::id()
    ))
}

fn sample_ledger_contents() -> String {
    [
        r#"{"id":"id-1","tool":"shape","version":"0.1.0","binary_hash":"blake3:b1","inputs":[{"path":"old.csv","hash":"blake3:h111","bytes":10}],"params":{"json":false},"outcome":"COMPATIBLE","exit_code":0,"output_hash":"blake3:o1","prev":null,"ts":"2026-02-01T00:00:00Z"}"#,
        r#"{"id":"id-2","tool":"shape","version":"0.1.0","binary_hash":"blake3:b2","inputs":[{"path":"new.csv","hash":"blake3:h222","bytes":11}],"params":{"json":true},"outcome":"INCOMPATIBLE","exit_code":1,"output_hash":"blake3:o2","prev":"id-1","ts":"2026-02-02T00:00:00Z"}"#,
    ]
    .join("\n")
        + "\n"
}

fn assert_witness_record_shape(record: &Value) {
    let object = record.as_object().expect("record should be object");

    assert!(object.contains_key("id"));
    assert!(object["id"].as_str().is_some_and(|value| !value.is_empty()));
    assert_eq!(object["tool"].as_str(), Some("shape"));
    assert!(
        object["version"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert!(
        object["binary_hash"]
            .as_str()
            .is_some_and(|value| value.starts_with("blake3:"))
    );
    assert!(object["params"].is_object());
    assert!(
        object["outcome"]
            .as_str()
            .is_some_and(|value| matches!(value, "COMPATIBLE" | "INCOMPATIBLE" | "REFUSAL"))
    );
    assert!(object["exit_code"].as_u64().is_some());
    assert!(
        object["output_hash"]
            .as_str()
            .is_some_and(|value| value.starts_with("blake3:"))
    );
    assert!(
        object["prev"].is_null()
            || object["prev"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
    );
    assert!(
        object["ts"]
            .as_str()
            .is_some_and(|value| value.ends_with('Z'))
    );

    let inputs = object["inputs"]
        .as_array()
        .expect("inputs should be an array");
    assert!(!inputs.is_empty());
    for input in inputs {
        let input_object = input.as_object().expect("input should be object");
        assert!(
            input_object["path"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        assert!(
            input_object["hash"]
                .as_str()
                .is_some_and(|value| value.starts_with("blake3:"))
        );
        assert!(input_object["bytes"].as_u64().is_some());
    }
}

#[test]
fn witness_query_json_records_conform_to_schema_shape() {
    let ledger = unique_ledger_path("query");
    fs::write(&ledger, sample_ledger_contents()).expect("write witness ledger fixture");

    let result = run_shape_with_ledger(["witness", "query", "--json"], &ledger);
    assert_eq!(result.status, 0);
    assert!(result.stderr.trim().is_empty());

    let payload: Value =
        serde_json::from_str(result.stdout.trim()).expect("query --json should return JSON array");
    let rows = payload.as_array().expect("query payload should be array");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], "id-1");
    assert_eq!(rows[1]["id"], "id-2");

    for row in rows {
        assert_witness_record_shape(row);
    }

    let _ = fs::remove_file(&ledger);
}

#[test]
fn witness_last_json_returns_latest_record_with_schema_shape() {
    let ledger = unique_ledger_path("last");
    fs::write(&ledger, sample_ledger_contents()).expect("write witness ledger fixture");

    let result = run_shape_with_ledger(["witness", "last", "--json"], &ledger);
    assert_eq!(result.status, 0);
    assert!(result.stderr.trim().is_empty());

    let payload: Value =
        serde_json::from_str(result.stdout.trim()).expect("last --json should return JSON object");
    assert_eq!(payload["id"], "id-2");
    assert_witness_record_shape(&payload);

    let _ = fs::remove_file(&ledger);
}

#[test]
fn witness_count_json_returns_count_object() {
    let ledger = unique_ledger_path("count");
    fs::write(&ledger, sample_ledger_contents()).expect("write witness ledger fixture");

    let result = run_shape_with_ledger(
        ["witness", "count", "--json", "--outcome", "COMPATIBLE"],
        &ledger,
    );
    assert_eq!(result.status, 0);
    assert!(result.stderr.trim().is_empty());

    let payload: Value =
        serde_json::from_str(result.stdout.trim()).expect("count --json should return JSON object");
    assert_eq!(payload, serde_json::json!({"count": 1}));

    let _ = fs::remove_file(&ledger);
}

#[test]
fn witness_query_no_match_json_contract_stays_stable() {
    let ledger = unique_ledger_path("no-match");
    fs::write(&ledger, sample_ledger_contents()).expect("write witness ledger fixture");

    let result = run_shape_with_ledger(
        ["witness", "query", "--json", "--outcome", "REFUSAL"],
        &ledger,
    );

    assert_eq!(result.status, 1);
    assert!(result.stderr.contains("shape: no matching witness records"));
    let payload: Value =
        serde_json::from_str(result.stdout.trim()).expect("query no-match should return JSON");
    assert_eq!(payload, serde_json::json!([]));

    let _ = fs::remove_file(&ledger);
}

#[test]
fn witness_query_ledger_read_error_returns_exit_two() {
    let ledger_dir = unique_ledger_path("read-error-dir");
    fs::create_dir_all(&ledger_dir).expect("create witness ledger directory fixture");

    let result = run_shape_with_ledger(["witness", "query", "--json"], &ledger_dir);
    assert_eq!(result.status, 2);
    assert!(result.stdout.trim().is_empty());
    assert!(
        result
            .stderr
            .contains("shape: witness: failed to read ledger")
    );

    let _ = fs::remove_dir_all(&ledger_dir);
}

#[test]
fn witness_query_skips_malformed_ledger_lines_but_keeps_valid_records() {
    let ledger = unique_ledger_path("malformed-lines");
    let valid_a = r#"{"id":"id-a","tool":"shape","version":"0.1.0","binary_hash":"blake3:ba","inputs":[{"path":"old.csv","hash":"blake3:ha","bytes":10}],"params":{},"outcome":"COMPATIBLE","exit_code":0,"output_hash":"blake3:oa","prev":null,"ts":"2026-02-03T00:00:00Z"}"#;
    let valid_b = r#"{"id":"id-b","tool":"shape","version":"0.1.0","binary_hash":"blake3:bb","inputs":[{"path":"new.csv","hash":"blake3:hb","bytes":11}],"params":{},"outcome":"INCOMPATIBLE","exit_code":1,"output_hash":"blake3:ob","prev":"id-a","ts":"2026-02-04T00:00:00Z"}"#;
    fs::write(&ledger, format!("{valid_a}\nnot-json\n\n{valid_b}\n"))
        .expect("write malformed witness ledger fixture");

    let result = run_shape_with_ledger(["witness", "query", "--json"], &ledger);
    assert_eq!(result.status, 0);
    assert!(result.stderr.trim().is_empty());

    let payload: Value =
        serde_json::from_str(result.stdout.trim()).expect("query --json should return JSON array");
    let rows = payload.as_array().expect("query payload should be array");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], "id-a");
    assert_eq!(rows[1]["id"], "id-b");

    let _ = fs::remove_file(&ledger);
}

#[test]
fn describe_reports_ambient_witness_recording_contract() {
    let result = run_shape_with_ledger(["--describe"], Path::new("/tmp/unused-ledger"));
    assert_eq!(result.status, 0);
    assert!(result.stderr.trim().is_empty());

    let payload: Value =
        serde_json::from_str(result.stdout.trim()).expect("--describe should emit valid JSON");

    assert_eq!(
        payload["capabilities"]["witness"]["ambient_recording"].as_str(),
        Some("enabled_by_default")
    );
    assert_eq!(
        payload["capabilities"]["witness"]["no_witness_flag"].as_str(),
        Some("suppresses_recording")
    );

    let options = payload["options"]
        .as_array()
        .expect("operator options should be an array");
    let no_witness_description = options
        .iter()
        .find(|entry| entry["name"].as_str() == Some("no_witness"))
        .and_then(|entry| entry["description"].as_str())
        .expect("operator options should define no_witness");
    assert!(
        no_witness_description.contains("Suppress ambient witness"),
        "unexpected no_witness description: {no_witness_description}"
    );

    let actions = payload["subcommands"][0]["actions"]
        .as_array()
        .expect("operator witness actions should be an array");
    let count_usage = actions
        .iter()
        .find(|entry| entry["name"].as_str() == Some("count"))
        .and_then(|entry| entry["usage"].as_str())
        .expect("operator witness count usage should be present");
    assert!(
        !count_usage.contains("--limit"),
        "count usage must not advertise --limit: {count_usage}"
    );
}
