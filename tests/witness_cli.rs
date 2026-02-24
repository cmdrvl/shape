mod helpers;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use helpers::{ShapeInvocation, fixture_path};

const BASIC_OLD: &str = "basic_old.csv";
const BASIC_NEW: &str = "basic_new.csv";

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
            .expect("failed to execute shape binary for witness test"),
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
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "shape-witness-cli-{label}-{}-{counter}-{nanos}.jsonl",
        std::process::id()
    ))
}

fn sample_ledger_contents() -> String {
    [
        r#"{"id":"id-1","tool":"shape","version":"0.1.0","binary_hash":"b1","inputs":[{"path":"old.csv","hash":"h111","bytes":10}],"params":{},"outcome":"COMPATIBLE","exit_code":0,"output_hash":"o1","prev":null,"ts":"2026-02-01T00:00:00Z"}"#,
        r#"{"id":"id-2","tool":"shape","version":"0.1.0","binary_hash":"b2","inputs":[{"path":"new.csv","hash":"h222","bytes":11}],"params":{},"outcome":"INCOMPATIBLE","exit_code":1,"output_hash":"o2","prev":"id-1","ts":"2026-02-02T00:00:00Z"}"#,
    ]
    .join("\n")
        + "\n"
}

fn run_compare_with_ledger(extra_args: &[&str], ledger_path: &Path) -> ShapeInvocation {
    let old = fixture_path(BASIC_OLD).to_string_lossy().into_owned();
    let new = fixture_path(BASIC_NEW).to_string_lossy().into_owned();
    let mut args = vec![old, new];
    args.extend(extra_args.iter().map(|value| (*value).to_owned()));
    run_shape_with_ledger(args, ledger_path)
}

#[test]
fn witness_no_match_semantics_use_exit_one_and_expected_streams() {
    let ledger = unique_ledger_path("missing");
    let _ = fs::remove_file(&ledger);

    let query_human = run_shape_with_ledger(["witness", "query"], &ledger);
    assert_eq!(query_human.status, 1);
    assert!(query_human.stdout.trim().is_empty());
    assert!(query_human.stderr.contains("no matching witness records"));

    let query_json = run_shape_with_ledger(["witness", "query", "--json"], &ledger);
    assert_eq!(query_json.status, 1);
    assert!(query_json.stderr.contains("no matching witness records"));
    let query_payload: serde_json::Value =
        serde_json::from_str(query_json.stdout.trim()).expect("query --json should return JSON");
    assert_eq!(query_payload, serde_json::json!([]));

    let last_human = run_shape_with_ledger(["witness", "last"], &ledger);
    assert_eq!(last_human.status, 1);
    assert!(last_human.stdout.trim().is_empty());
    assert!(last_human.stderr.contains("witness ledger is empty"));

    let last_json = run_shape_with_ledger(["witness", "last", "--json"], &ledger);
    assert_eq!(last_json.status, 1);
    assert!(last_json.stderr.contains("witness ledger is empty"));
    let last_payload: serde_json::Value =
        serde_json::from_str(last_json.stdout.trim()).expect("last --json should return JSON");
    assert_eq!(last_payload, serde_json::Value::Null);

    let count_human = run_shape_with_ledger(["witness", "count"], &ledger);
    assert_eq!(count_human.status, 1);
    assert!(count_human.stdout.trim().is_empty());
    assert!(count_human.stderr.contains("no matching witness records"));

    let count_json = run_shape_with_ledger(["witness", "count", "--json"], &ledger);
    assert_eq!(count_json.status, 1);
    assert!(count_json.stderr.contains("no matching witness records"));
    let count_payload: serde_json::Value =
        serde_json::from_str(count_json.stdout.trim()).expect("count --json should return JSON");
    assert_eq!(count_payload, serde_json::json!({"count": 0}));
}

#[test]
fn witness_success_paths_read_records_from_epistemic_witness_ledger() {
    let ledger = unique_ledger_path("records");
    fs::write(&ledger, sample_ledger_contents()).expect("write test witness ledger");

    let last_json = run_shape_with_ledger(["witness", "last", "--json"], &ledger);
    assert_eq!(last_json.status, 0);
    assert!(last_json.stderr.trim().is_empty());
    let last_payload: serde_json::Value =
        serde_json::from_str(last_json.stdout.trim()).expect("last --json should return JSON");
    assert_eq!(last_payload["id"], "id-2");

    let query_json = run_shape_with_ledger(["witness", "query", "--json", "--limit", "1"], &ledger);
    assert_eq!(query_json.status, 0);
    assert!(query_json.stderr.trim().is_empty());
    let query_payload: serde_json::Value =
        serde_json::from_str(query_json.stdout.trim()).expect("query --json should return JSON");
    let rows = query_payload
        .as_array()
        .expect("query --json payload should be array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "id-1");
    for key in [
        "id",
        "tool",
        "version",
        "binary_hash",
        "inputs",
        "params",
        "outcome",
        "exit_code",
        "output_hash",
        "prev",
        "ts",
    ] {
        assert!(
            rows[0].get(key).is_some(),
            "query payload record should include witness field: {key}"
        );
    }

    let count_json = run_shape_with_ledger(
        ["witness", "count", "--json", "--outcome", "COMPATIBLE"],
        &ledger,
    );
    assert_eq!(count_json.status, 0);
    assert!(count_json.stderr.trim().is_empty());
    let count_payload: serde_json::Value =
        serde_json::from_str(count_json.stdout.trim()).expect("count --json should return JSON");
    assert_eq!(count_payload["count"], 1);

    let _ = fs::remove_file(&ledger);
}

#[test]
fn witness_returns_exit_two_when_ledger_path_is_unreadable() {
    let unreadable_path = std::env::temp_dir();
    let result = run_shape_with_ledger(["witness", "query"], &unreadable_path);

    assert_eq!(result.status, 2);
    assert!(result.stdout.trim().is_empty());
    assert!(
        result
            .stderr
            .contains("shape: witness: failed to read ledger"),
        "unexpected stderr for unreadable ledger path: {}",
        result.stderr
    );
}

#[test]
fn compare_mode_appends_witness_record_by_default() {
    let ledger = unique_ledger_path("compare-default");
    let _ = fs::remove_file(&ledger);

    let result = run_compare_with_ledger(&[], &ledger);
    assert_eq!(result.status, 0);
    assert!(result.stdout.contains("COMPATIBLE"));
    assert!(
        result.stderr.trim().is_empty(),
        "compare mode should not write stderr on successful append: {}",
        result.stderr
    );

    let content = fs::read_to_string(&ledger).expect("witness ledger should be created");
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "compare run should append exactly one record"
    );

    let record: serde_json::Value =
        serde_json::from_str(lines[0]).expect("ledger line should be valid witness JSON");
    assert_eq!(record["tool"], "shape");
    assert_eq!(record["outcome"], "COMPATIBLE");
    assert_eq!(record["exit_code"], 0);
    assert_eq!(
        record["inputs"].as_array().map(Vec::len),
        Some(2),
        "record should include both input files"
    );

    let _ = fs::remove_file(&ledger);
}

#[test]
fn compare_mode_no_witness_suppresses_ambient_append() {
    let ledger = unique_ledger_path("compare-suppressed");
    let _ = fs::remove_file(&ledger);

    let result = run_compare_with_ledger(&["--no-witness"], &ledger);
    assert_eq!(result.status, 0);
    assert!(result.stdout.contains("COMPATIBLE"));
    assert!(
        result.stderr.trim().is_empty(),
        "compare mode with --no-witness should not write stderr: {}",
        result.stderr
    );
    assert!(
        !ledger.exists(),
        "no witness file should be created when --no-witness is set"
    );
}

#[test]
fn compare_mode_preserves_domain_exit_when_witness_append_fails() {
    let ledger_dir = unique_ledger_path("compare-append-error-dir");
    fs::create_dir_all(&ledger_dir).expect("create directory to force append failure");

    let result = run_compare_with_ledger(&[], &ledger_dir);
    assert_eq!(
        result.status, 0,
        "witness append failure must not change domain exit"
    );
    assert!(result.stdout.contains("COMPATIBLE"));
    assert!(
        result.stderr.contains("shape: witness:"),
        "append failure should be reported on stderr: {}",
        result.stderr
    );

    let _ = fs::remove_dir_all(&ledger_dir);
}

#[test]
fn compare_refusal_path_does_not_emit_witness_warning_noise() {
    let ledger = unique_ledger_path("compare-refusal-noise");
    let _ = fs::remove_file(&ledger);

    let missing_old = unique_ledger_path("missing-old-input");
    let new = fixture_path(BASIC_NEW).to_string_lossy().into_owned();
    let missing_old_str = missing_old.to_string_lossy().into_owned();
    let result = run_shape_with_ledger([missing_old_str.as_str(), new.as_str()], &ledger);

    assert_eq!(result.status, 2);
    assert!(result.stdout.trim().is_empty());
    assert!(result.stderr.contains("SHAPE ERROR (E_IO)"));
    assert!(
        !result.stderr.contains("shape: witness:"),
        "refusal output should not be contaminated by witness append warnings: {}",
        result.stderr
    );
}

#[test]
fn describe_operator_contract_includes_witness_usage_and_no_witness_option() {
    let result = run_shape_with_ledger(["--describe"], Path::new("/tmp/unused-ledger"));
    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "--describe should not write stderr: {}",
        result.stderr
    );

    let payload: serde_json::Value =
        serde_json::from_str(result.stdout.trim()).expect("--describe should emit valid JSON");

    let usage = payload["invocation"]["usage"]
        .as_array()
        .expect("invocation.usage should be array");
    assert!(
        usage.iter().any(|entry| {
            entry
                .as_str()
                .is_some_and(|value| value.contains("shape witness <query|last|count>"))
        }),
        "operator usage should include witness subcommand syntax"
    );

    let options = payload["options"]
        .as_array()
        .expect("options should be an array");
    assert!(
        options.iter().any(|entry| {
            entry["name"].as_str() == Some("no_witness")
                && entry["flag"].as_str() == Some("--no-witness")
        }),
        "operator options should include --no-witness"
    );

    assert_eq!(payload["subcommands"][0]["name"].as_str(), Some("witness"));
    let actions = payload["subcommands"][0]["actions"]
        .as_array()
        .expect("witness actions should be an array");

    let query_usage = actions
        .iter()
        .find(|entry| entry["name"].as_str() == Some("query"))
        .and_then(|entry| entry["usage"].as_str())
        .expect("query action usage should be present");
    assert!(
        query_usage.contains("--limit <n>"),
        "query usage should continue to advertise --limit"
    );

    let count_usage = actions
        .iter()
        .find(|entry| entry["name"].as_str() == Some("count"))
        .and_then(|entry| entry["usage"].as_str())
        .expect("count action usage should be present");
    assert!(
        !count_usage.contains("--limit"),
        "count usage must not advertise --limit: {count_usage}"
    );
}
