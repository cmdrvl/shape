mod helpers;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use helpers::{fixture_path, run_shape};
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
        value["input_verification"]["files"]["old"]["path"],
        old.to_string_lossy().as_ref()
    );
    assert_eq!(
        value["input_verification"]["files"]["old"]["bytes_hash"],
        sha256_of(&old)
    );
    assert_eq!(
        value["input_verification"]["files"]["new"]["path"],
        new.to_string_lossy().as_ref()
    );
    assert_eq!(
        value["input_verification"]["files"]["new"]["bytes_hash"],
        sha256_of(&new)
    );
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
