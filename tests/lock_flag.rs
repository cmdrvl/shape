mod helpers;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use helpers::{fixture_path, run_shape, run_shape_with_env};
use serde_json::Value;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn scratch_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "shape-lock-flag-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

fn sha256_of(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let bytes = fs::read(path).expect("fixture should be readable");
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn write_lock(dir: &Path, members: &[(&Path, &str)]) -> PathBuf {
    let members = members
        .iter()
        .map(|(path, hash)| {
            serde_json::json!({ "path": path.to_string_lossy(), "bytes_hash": hash })
        })
        .collect::<Vec<_>>();
    let path = dir.join("inputs.lock.json");
    fs::write(
        &path,
        serde_json::json!({
            "version": "lock.v0",
            "lock_hash": "sha256:cafe",
            "members": members
        })
        .to_string(),
    )
    .expect("lockfile should be writable");
    path
}

fn run_with_lock(old: &Path, new: &Path, lock: &Path) -> helpers::ShapeInvocation {
    run_shape([
        old.to_string_lossy().as_ref(),
        new.to_string_lossy().as_ref(),
        "--lock",
        lock.to_string_lossy().as_ref(),
        "--json",
    ])
}

fn refusal_code(result: &helpers::ShapeInvocation) -> String {
    let parsed = serde_json::from_str::<Value>(&result.stdout);
    assert!(parsed.is_ok(), "refusal should be JSON: {}", result.stdout);
    let value = parsed.unwrap_or(Value::Null);
    assert_eq!(value["outcome"], "REFUSAL", "{}", result.stdout);
    value["refusal"]["code"]
        .as_str()
        .expect("refusal code should be a string")
        .to_owned()
}

#[test]
fn operator_json_version_matches_cargo_package_version() {
    let operator: Value = serde_json::from_str(include_str!("../operator.json"))
        .expect("operator.json should be valid JSON");
    assert_eq!(operator["tool_version"], env!("CARGO_PKG_VERSION"));

    let described = run_shape(["--describe"]);
    assert_eq!(described.status, 0);
    let described: Value = serde_json::from_str(&described.stdout).expect("describe is JSON");
    assert_eq!(described["tool_version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn operator_json_no_longer_calls_lock_reserved() {
    let operator: Value = serde_json::from_str(include_str!("../operator.json"))
        .expect("operator.json should be valid JSON");
    let lock = operator["options"]
        .as_array()
        .and_then(|options| options.iter().find(|option| option["flag"] == "--lock"))
        .expect("operator.json should declare --lock");
    let description = lock["description"].as_str().unwrap_or_default();
    assert!(
        !description.contains("Reserved") && !description.contains("deferred"),
        "--lock is enforced; description must not say otherwise: {description}"
    );
}

#[test]
fn lock_passes_when_both_inputs_match_their_locked_hashes() {
    let dir = scratch_dir();
    let old = fixture_path("basic_old.csv");
    let new = fixture_path("basic_new.csv");
    let lock = write_lock(&dir, &[(&old, &sha256_of(&old)), (&new, &sha256_of(&new))]);

    let result = run_with_lock(&old, &new, &lock);

    assert!(
        result.status == 0 || result.status == 1,
        "locked inputs should compare normally, got status {}: {}",
        result.status,
        result.stdout
    );
    let value: Value = serde_json::from_str(&result.stdout).expect("result should be JSON");
    assert_ne!(value["outcome"], "REFUSAL", "{}", result.stdout);
    assert_eq!(value["input_verification"]["verified"], true);
    assert_eq!(
        value["input_verification"]["locks"][0]["path"],
        lock.to_string_lossy().as_ref()
    );
    assert_eq!(
        value["input_verification"]["locks"][0]["lock_hash"],
        "sha256:cafe"
    );
    assert_eq!(
        value["input_verification"]["matched_inputs"]["old"],
        old.to_string_lossy().as_ref()
    );
    assert_eq!(
        value["input_verification"]["matched_inputs"]["new"],
        new.to_string_lossy().as_ref()
    );
    assert!(value["input_verification"]["files"].is_null());
    assert!(!result.stdout.contains("bytes_hash"));
}

fn assert_no_input_sha256(text: &str, old: &Path, new: &Path) {
    for path in [old, new] {
        let digest = sha256_of(path);
        let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
        assert!(
            !text.contains(hex),
            "input digest leaked in output: {hex}: {text}"
        );
    }
}

#[test]
fn lock_receipt_does_not_disclose_input_hashes_on_default_surfaces() {
    let dir = scratch_dir();
    let old = fixture_path("basic_old.csv");
    let new = fixture_path("basic_new.csv");
    let lock = write_lock(&dir, &[(&old, &sha256_of(&old)), (&new, &sha256_of(&new))]);

    let json = run_shape([
        old.to_string_lossy().as_ref(),
        new.to_string_lossy().as_ref(),
        "--lock",
        lock.to_string_lossy().as_ref(),
        "--json",
        "--no-witness",
    ]);
    assert_no_input_sha256(&json.stdout, &old, &new);
    let json_value: Value = serde_json::from_str(&json.stdout).expect("default JSON");

    let explicit = run_shape([
        old.to_string_lossy().as_ref(),
        new.to_string_lossy().as_ref(),
        "--lock",
        lock.to_string_lossy().as_ref(),
        "--json",
        "--explicit",
        "--no-witness",
    ]);
    assert_no_input_sha256(&explicit.stdout, &old, &new);
    let explicit_value: Value = serde_json::from_str(&explicit.stdout).expect("explicit JSON");
    assert_eq!(
        explicit_value["input_verification"],
        json_value["input_verification"]
    );

    let human = run_shape([
        old.to_string_lossy().as_ref(),
        new.to_string_lossy().as_ref(),
        "--lock",
        lock.to_string_lossy().as_ref(),
        "--no-witness",
    ]);
    assert_no_input_sha256(&human.stdout, &old, &new);
    assert_no_input_sha256(&human.stderr, &old, &new);

    let ledger = dir.join("witness.jsonl");
    let witnessed = run_shape_with_env(
        &[
            old.to_string_lossy().as_ref(),
            new.to_string_lossy().as_ref(),
            "--lock",
            lock.to_string_lossy().as_ref(),
            "--json",
        ],
        &[("EPISTEMIC_WITNESS", ledger.to_string_lossy().as_ref())],
    );
    assert_eq!(witnessed.status, 0, "{}", witnessed.stderr);
    let ledger_text = fs::read_to_string(&ledger).expect("witness ledger should be written");
    assert_no_input_sha256(&ledger_text, &old, &new);
    assert!(!ledger_text.contains("input_verification"));
    assert!(!ledger_text.contains("bytes_hash"));

    let capsule = dir.join("capsule");
    let capsule_result = run_shape([
        old.to_string_lossy().as_ref(),
        new.to_string_lossy().as_ref(),
        "--lock",
        lock.to_string_lossy().as_ref(),
        "--json",
        "--no-witness",
        "--capsule-dir",
        capsule.to_string_lossy().as_ref(),
    ]);
    assert_eq!(capsule_result.status, 0, "{}", capsule_result.stderr);
    for entry in walk_files(&capsule) {
        let contents = fs::read_to_string(&entry);
        assert!(
            contents.is_ok(),
            "capsule artifact should be text for this test: {}: {:?}",
            entry.display(),
            contents.as_ref().err()
        );
        let contents = contents.unwrap_or_default();
        assert_no_input_sha256(&contents, &old, &new);
    }
}

fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).expect("capsule directory should be readable") {
            let entry = entry.expect("capsule entry should be readable");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn lock_refuses_input_missing_from_lockfile() {
    let dir = scratch_dir();
    let old = fixture_path("basic_old.csv");
    let new = fixture_path("basic_new.csv");
    // Planted negative: only `old` is locked.
    let lock = write_lock(&dir, &[(&old, &sha256_of(&old))]);

    let result = run_with_lock(&old, &new, &lock);

    assert_eq!(result.status, 2, "{}", result.stdout);
    assert_eq!(refusal_code(&result), "E_INPUT_NOT_LOCKED");
    let value: Value = serde_json::from_str(&result.stdout).expect("result should be JSON");
    assert!(value["input_verification"].is_null());
}

#[test]
fn lock_refuses_when_locked_hash_does_not_match_input_bytes() {
    let dir = scratch_dir();
    let old = fixture_path("basic_old.csv");
    let new = fixture_path("basic_new.csv");
    // Planted negative: `new` is listed with a stale hash.
    let lock = write_lock(&dir, &[(&old, &sha256_of(&old)), (&new, "sha256:deadbeef")]);

    let result = run_with_lock(&old, &new, &lock);

    assert_eq!(result.status, 2, "{}", result.stdout);
    assert_eq!(refusal_code(&result), "E_INPUT_DRIFT");
    let value: Value = serde_json::from_str(&result.stdout).expect("result should be JSON");
    assert!(value["input_verification"].is_null());
}
