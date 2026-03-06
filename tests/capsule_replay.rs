mod helpers;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use helpers::{ShapeInvocation, fixture_path};
use serde_json::Value;

struct ReplayScenario {
    label: &'static str,
    old_fixture: &'static str,
    new_fixture: &'static str,
    extra_args: &'static [&'static str],
    expected_status: i32,
    expected_outcome: &'static str,
    expected_refusal_code: Option<&'static str>,
}

fn unique_capsule_dir(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "shape-capsule-replay-{label}-{}-{counter}-{nanos}",
        std::process::id(),
    ))
}

fn run_shape<I, S>(args: I, current_dir: Option<&Path>) -> ShapeInvocation
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(env!("CARGO_BIN_EXE_shape"));
    command.args(args);
    if let Some(path) = current_dir {
        command.current_dir(path);
    }
    shape_invocation_from_output(
        command
            .output()
            .expect("failed to execute shape for capsule replay test"),
    )
}

fn shape_invocation_from_output(output: Output) -> ShapeInvocation {
    ShapeInvocation {
        status: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn refusal_code(payload: &Value) -> Option<&str> {
    payload
        .get("refusal")
        .and_then(|value| value.get("code"))
        .and_then(Value::as_str)
}

fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).expect("test file should be writable");
    path
}

fn run_and_assert_replay_scenario(scenario: &ReplayScenario) {
    let capsule_dir = unique_capsule_dir(scenario.label);
    fs::create_dir_all(&capsule_dir).expect("create capsule dir");

    let old = fixture_path(scenario.old_fixture)
        .to_string_lossy()
        .into_owned();
    let new = fixture_path(scenario.new_fixture)
        .to_string_lossy()
        .into_owned();

    let mut args = vec![
        old,
        new,
        "--json".to_owned(),
        "--no-witness".to_owned(),
        "--capsule-dir".to_owned(),
        capsule_dir.to_string_lossy().into_owned(),
    ];
    args.extend(scenario.extra_args.iter().map(|arg| (*arg).to_owned()));

    let first = run_shape(args.iter().map(String::as_str), None);
    assert_eq!(first.status, scenario.expected_status);
    assert!(
        first.stderr.trim().is_empty(),
        "first run should not write stderr in --json mode: {}",
        first.stderr
    );
    let first_payload: Value = serde_json::from_str(first.stdout_trimmed())
        .expect("first run should emit valid json output");
    assert_eq!(first_payload["outcome"], scenario.expected_outcome);
    assert_eq!(refusal_code(&first_payload), scenario.expected_refusal_code);

    let manifest_text = fs::read_to_string(capsule_dir.join("manifest.json"))
        .expect("capsule manifest should be written");
    let manifest: Value =
        serde_json::from_str(&manifest_text).expect("capsule manifest should parse");
    assert_eq!(manifest["result"]["outcome"], scenario.expected_outcome);
    assert_eq!(
        manifest["result"]["refusal"]["code"].as_str(),
        scenario.expected_refusal_code
    );

    let replay_argv = manifest["replay"]["argv"]
        .as_array()
        .expect("replay argv should be array")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("replay argv values should be strings")
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert!(
        replay_argv.len() >= 3,
        "replay argv should include shape and two input paths"
    );
    assert_eq!(replay_argv[0], "shape");

    let replay = run_shape(
        replay_argv.iter().skip(1).map(std::string::String::as_str),
        Some(capsule_dir.as_path()),
    );
    assert_eq!(replay.status, scenario.expected_status);
    assert!(
        replay.stderr.trim().is_empty(),
        "replay run should not write stderr in --json mode: {}",
        replay.stderr
    );
    let replay_payload: Value = serde_json::from_str(replay.stdout_trimmed())
        .expect("replay run should emit valid json output");
    assert_eq!(replay_payload["outcome"], scenario.expected_outcome);
    assert_eq!(
        refusal_code(&replay_payload),
        scenario.expected_refusal_code
    );

    let _ = fs::remove_dir_all(capsule_dir);
}

#[test]
fn capsule_replay_preserves_outcome_and_refusal_code() {
    let scenarios = [
        ReplayScenario {
            label: "compatible",
            old_fixture: "basic_old.csv",
            new_fixture: "basic_new.csv",
            extra_args: &["--key", "loan_id"],
            expected_status: 0,
            expected_outcome: "COMPATIBLE",
            expected_refusal_code: None,
        },
        ReplayScenario {
            label: "incompatible",
            old_fixture: "type_shift_old.csv",
            new_fixture: "type_shift_new.csv",
            extra_args: &[],
            expected_status: 1,
            expected_outcome: "INCOMPATIBLE",
            expected_refusal_code: None,
        },
        ReplayScenario {
            label: "refusal",
            old_fixture: "basic_old.csv",
            new_fixture: "no_header.csv",
            extra_args: &[],
            expected_status: 2,
            expected_outcome: "REFUSAL",
            expected_refusal_code: Some("E_HEADERS"),
        },
    ];

    for scenario in scenarios {
        run_and_assert_replay_scenario(&scenario);
    }
}

#[test]
fn capsule_replay_with_relative_profile_uses_local_profile_artifact() {
    let workspace = unique_capsule_dir("profile-workspace");
    let capsule_dir = workspace.join("capsule");
    fs::create_dir_all(&workspace).expect("create workspace");
    fs::create_dir_all(&capsule_dir).expect("create capsule dir");
    write_file(
        &workspace,
        "old.csv",
        "loan_id,balance,status\nA1,100,active\nA2,200,active\n",
    );
    write_file(
        &workspace,
        "new.csv",
        "loan_id,balance,status\nA1,110,active\nA2,200,active\n",
    );
    write_file(
        &workspace,
        "profile.yaml",
        "profile_id: loan-tape.v0\nprofile_sha256: sha256:test-loan-tape\ninclude_columns:\n  - loan_id\n  - balance\nkey:\n  - loan_id\n",
    );

    let first = run_shape(
        [
            "old.csv",
            "new.csv",
            "--json",
            "--no-witness",
            "--capsule-dir",
            "capsule",
            "--profile",
            "profile.yaml",
        ],
        Some(workspace.as_path()),
    );
    assert_eq!(first.status, 0);
    assert!(
        first.stderr.trim().is_empty(),
        "first run should not write stderr in --json mode: {}",
        first.stderr
    );

    let manifest_text = fs::read_to_string(capsule_dir.join("manifest.json"))
        .expect("capsule manifest should be written");
    let manifest: Value =
        serde_json::from_str(&manifest_text).expect("capsule manifest should parse");
    assert_eq!(manifest["args"]["profile"], "profile.yaml");
    assert!(capsule_dir.join("profile.yaml").is_file());

    let replay_argv = manifest["replay"]["argv"]
        .as_array()
        .expect("replay argv should be array")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("replay argv values should be strings")
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert!(replay_argv.contains(&"--profile".to_owned()));
    assert!(replay_argv.contains(&"profile.yaml".to_owned()));
    assert!(
        !replay_argv.contains(
            &workspace
                .join("profile.yaml")
                .to_string_lossy()
                .into_owned()
        )
    );

    let replay = run_shape(
        replay_argv.iter().skip(1).map(std::string::String::as_str),
        Some(capsule_dir.as_path()),
    );
    assert_eq!(replay.status, 0);
    assert!(
        replay.stderr.trim().is_empty(),
        "replay run should not write stderr in --json mode: {}",
        replay.stderr
    );
    let first_payload: Value = serde_json::from_str(first.stdout_trimmed())
        .expect("first run should emit valid json output");
    let replay_payload: Value = serde_json::from_str(replay.stdout_trimmed())
        .expect("replay run should emit valid json output");
    assert_eq!(replay_payload["outcome"], first_payload["outcome"]);
    assert_eq!(replay_payload["profile_id"], first_payload["profile_id"]);
    assert_eq!(
        replay_payload["profile_sha256"],
        first_payload["profile_sha256"]
    );

    let _ = fs::remove_dir_all(workspace);
}
