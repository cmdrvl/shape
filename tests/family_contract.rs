//! bd-k3eu: spine-rules v2 family-contract behaviors that the generated golden suite cannot
//! reach on its own: doctor-unhealthy exit, bare-invocation guidance, NO_COLOR/TERM=dumb,
//! SOURCE_DATE_EPOCH, and the deprecated spellings kept as aliases.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

fn temp_dir(label: &str) -> PathBuf {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("shape_family_{label}_{}_{n}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn shape_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shape"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .expect("run shape")
}

fn shape(args: &[&str]) -> Output {
    shape_in(Path::new(env!("CARGO_MANIFEST_DIR")), args)
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not JSON ({e}): {}", String::from_utf8_lossy(&out.stdout)))
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

// ── doctor: cmdrvl.doctor.v1, unhealthy exits 1 ─────────────────────────────

/// A directory that looks like a shape source checkout but does not ignore `.doctor/`,
/// which makes the `source-gitignore-doctor` check fail.
fn unhealthy_checkout() -> PathBuf {
    let dir = temp_dir("unhealthy");
    fs::write(dir.join("Cargo.toml"), "[package]\nname = \"shape\"\n").unwrap();
    fs::write(dir.join("operator.json"), "{}").unwrap();
    dir
}

#[test]
fn doctor_unhealthy_exits_one_with_error_finding_and_fix() {
    let dir = unhealthy_checkout();
    let out = shape_in(&dir, &["doctor", "--json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "unhealthy doctor must exit 1, never 2"
    );
    let report = json(&out);
    assert_eq!(report["schema_version"], "cmdrvl.doctor.v1");
    assert_eq!(report["mode"], "health");
    assert_eq!(report["healthy"], false);
    let errors: Vec<&Value> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["severity"] == "error")
        .collect();
    assert_eq!(errors.len(), 1, "{report}");
    assert_eq!(errors[0]["id"], "source-gitignore-doctor");
    assert!(errors[0]["fix"].as_str().unwrap().contains(".doctor/"));
}

#[test]
fn doctor_healthy_when_gitignore_covers_doctor_dir() {
    let dir = unhealthy_checkout();
    fs::write(dir.join(".gitignore"), ".doctor/\n").unwrap();
    let out = shape_in(&dir, &["doctor", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(json(&out)["healthy"], true);
}

#[test]
fn doctor_triage_unhealthy_has_next_actions_and_exit_one() {
    let dir = unhealthy_checkout();
    let out = shape_in(&dir, &["doctor", "triage", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let report = json(&out);
    assert_eq!(report["mode"], "triage");
    assert!(!report["next_actions"].as_array().unwrap().is_empty());
}

#[test]
fn doctor_capabilities_embeds_the_capabilities_contract() {
    let report = json(&shape(&["doctor", "capabilities", "--json"]));
    assert_eq!(report["mode"], "capabilities");
    assert_eq!(
        report["contract"],
        json(&shape(&["capabilities", "--json"]))
    );
    assert_eq!(report["contract"], json(&shape(&["--describe"])));
}

#[test]
fn deprecated_spellings_still_emit_doctor_v1() {
    for args in [&["--robot-triage"][..], &["doctor", "--robot-triage"][..]] {
        let report = json(&shape(args));
        assert_eq!(report["schema_version"], "cmdrvl.doctor.v1", "{args:?}");
        assert_eq!(report["mode"], "triage", "{args:?}");
    }
    let health = json(&shape(&["doctor", "health", "--json"]));
    assert_eq!(health["schema_version"], "cmdrvl.doctor.v1");
    assert_eq!(health["mode"], "health");
}

#[test]
fn shape_verdict_exit_codes_are_untouched_by_the_doctor_rework() {
    // COMPATIBLE stays 0 and a refusal stays 2; the doctor's exit 1 is a separate domain.
    let ok = shape(&[
        &fixture("basic_old.csv"),
        &fixture("basic_new.csv"),
        "--json",
        "--no-witness",
    ]);
    assert_eq!(ok.status.code(), Some(0));
    let refused = shape(&[
        "/nonexistent/a.csv",
        "/nonexistent/b.csv",
        "--json",
        "--no-witness",
    ]);
    assert_eq!(refused.status.code(), Some(2));
}

// ── bare invocation ─────────────────────────────────────────────────────────

#[test]
fn bare_invocation_gives_agent_guidance_and_does_not_pass_the_gate() {
    let out = shape(&[]);
    // A gate must never report success because arguments were forgotten, so bare `shape`
    // stays a usage error (exit 2) rather than a friendly exit 0.
    assert_eq!(out.status.code(), Some(2));
    assert!(
        out.stdout.is_empty(),
        "guidance belongs on stderr, stdout stays clean"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("shape --robot-triage"), "{stderr}");
    assert!(stderr.contains("shape capabilities --json"), "{stderr}");
    assert!(stderr.contains("shape doctor"), "{stderr}");
}

// ── NO_COLOR / TERM=dumb / non-TTY ──────────────────────────────────────────

fn has_ansi(out: &Output) -> bool {
    let esc = |b: &[u8]| b.windows(2).any(|w| w == b"\x1b[");
    esc(&out.stdout) || esc(&out.stderr)
}

fn run_with_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_shape"));
    cmd.args(args)
        .stdin(Stdio::null())
        .env_remove("NO_COLOR")
        .env_remove("TERM");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("run shape")
}

#[test]
fn forced_color_is_stripped_by_no_color_and_term_dumb() {
    // Positive control: when color is forced and nothing disables it, clap output is styled.
    // If this ever stops holding, the assertions below would pass vacuously.
    let forced = run_with_env(&["--bogus-flag"], &[("CLICOLOR_FORCE", "1")]);
    assert!(
        has_ansi(&forced),
        "control: CLICOLOR_FORCE should style clap output"
    );

    for env in [
        &[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")][..],
        &[("CLICOLOR_FORCE", "1"), ("TERM", "dumb")][..],
    ] {
        for args in [&["--bogus-flag"][..], &["--help"][..], &[][..]] {
            let out = run_with_env(args, env);
            assert!(!has_ansi(&out), "ANSI leaked for {args:?} with {env:?}");
        }
    }
}

// ── SOURCE_DATE_EPOCH ───────────────────────────────────────────────────────

#[test]
fn witness_timestamp_honors_source_date_epoch() {
    let old = fixture("basic_old.csv");
    let new = fixture("basic_new.csv");
    let run = |label: &str, epoch: Option<&str>| {
        let ledger = temp_dir(label).join("witness.jsonl");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_shape"));
        cmd.args([old.as_str(), new.as_str()])
            .env("EPISTEMIC_WITNESS", &ledger)
            .env_remove("SOURCE_DATE_EPOCH");
        if let Some(epoch) = epoch {
            cmd.env("SOURCE_DATE_EPOCH", epoch);
        }
        cmd.output().expect("run shape");
        let text = fs::read_to_string(&ledger).expect("ledger written");
        let line = text.lines().last().expect("one record");
        serde_json::from_str::<Value>(line).unwrap()["ts"]
            .as_str()
            .unwrap()
            .to_string()
    };

    // 1700000000 == 2023-11-14T22:13:20Z
    assert_eq!(run("sde_a", Some("1700000000")), "2023-11-14T22:13:20Z");
    assert_eq!(run("sde_b", Some("1700000000")), "2023-11-14T22:13:20Z");
    // Control: without the variable the wall clock is used, so the pinned value is not a constant.
    assert_ne!(run("sde_none", None), "2023-11-14T22:13:20Z");
}
