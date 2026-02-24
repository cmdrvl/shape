mod helpers;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use helpers::{ShapeInvocation, fixture_path, run_shape, run_shape_with_fixtures};
use serde_json::Value;

const BASIC_OLD: &str = "basic_old.csv";
const BASIC_NEW: &str = "basic_new.csv";
const TYPE_SHIFT_OLD: &str = "type_shift_old.csv";
const TYPE_SHIFT_NEW: &str = "type_shift_new.csv";
const EMPTY: &str = "empty.csv";
const NO_HEADER: &str = "no_header.csv";
const DIALECT_AMBIGUOUS: &str = "dialect_ambiguous.csv";
const KEY_LOAN_ID: &str = "loan_id";

struct Scenario {
    old_fixture: &'static str,
    new_fixture: &'static str,
    extra_args: &'static [&'static str],
    expected_exit: i32,
    expected_outcome: &'static str,
}

fn compared_line(old_fixture: &str, new_fixture: &str) -> String {
    let old_path = fixture_path(old_fixture);
    let new_path = fixture_path(new_fixture);
    format!(
        "Compared: {} -> {}",
        old_path.to_string_lossy(),
        new_path.to_string_lossy()
    )
}

fn assert_contains_compared_line(
    output: &str,
    old_fixture: &str,
    new_fixture: &str,
    context: &str,
) {
    assert!(
        output.contains(&compared_line(old_fixture, new_fixture)),
        "{context}"
    );
}

fn run_json_with_extra(
    old_fixture: &str,
    new_fixture: &str,
    extra_args: &[&str],
) -> ShapeInvocation {
    let mut args = Vec::with_capacity(extra_args.len() + 1);
    args.push("--json".to_string());
    args.extend(extra_args.iter().map(|arg| (*arg).to_string()));
    let json_args: Vec<&str> = args.iter().map(String::as_str).collect();
    run_shape_with_fixtures(old_fixture, new_fixture, &json_args)
}

fn run_human_scenario(scenario: &Scenario) -> ShapeInvocation {
    run_shape_with_fixtures(
        scenario.old_fixture,
        scenario.new_fixture,
        scenario.extra_args,
    )
}

fn parse_json_output(result: &ShapeInvocation, context: &str) -> Value {
    serde_json::from_str(result.stdout_trimmed()).expect(context)
}

fn run_shape_with_paths(old: &str, new: &str, extra_args: &[&str]) -> ShapeInvocation {
    let mut args = vec![old.to_string(), new.to_string()];
    args.extend(extra_args.iter().map(|arg| (*arg).to_string()));
    run_shape(args)
}

fn assert_human_outcome(scenario: &Scenario, human: &ShapeInvocation) {
    assert_eq!(
        human.status, scenario.expected_exit,
        "unexpected human exit code for {} -> {}",
        scenario.old_fixture, scenario.new_fixture
    );

    if scenario.expected_outcome == "REFUSAL" {
        assert!(
            human.stdout.trim().is_empty(),
            "human refusal should write to stderr only"
        );
        assert!(human.stderr.contains("SHAPE ERROR"));
        assert_contains_compared_line(
            &human.stderr,
            scenario.old_fixture,
            scenario.new_fixture,
            "human refusal should echo compared file paths",
        );
    } else {
        assert!(
            human.stderr.trim().is_empty(),
            "human compatible/incompatible should not write stderr: {}",
            human.stderr
        );
        assert!(human.stdout.contains(scenario.expected_outcome));
        assert_contains_compared_line(
            &human.stdout,
            scenario.old_fixture,
            scenario.new_fixture,
            "human output should echo compared file paths",
        );
    }
}

fn assert_json_outcome(scenario: &Scenario, json: &ShapeInvocation) {
    assert_eq!(
        json.status, scenario.expected_exit,
        "unexpected json exit code for {} -> {}",
        scenario.old_fixture, scenario.new_fixture
    );
    assert!(
        json.stderr.trim().is_empty(),
        "json mode should not write stderr for domain outcomes: {}",
        json.stderr
    );

    let payload = parse_json_output(json, "json output should parse");
    assert_eq!(payload["outcome"], scenario.expected_outcome);
    assert_eq!(
        payload["files"]["old"],
        fixture_path(scenario.old_fixture)
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(
        payload["files"]["new"],
        fixture_path(scenario.new_fixture)
            .to_string_lossy()
            .as_ref()
    );
}

#[test]
fn e2e_matrix_runs_human_and_json_through_shared_harness() {
    let scenarios = [
        Scenario {
            old_fixture: BASIC_OLD,
            new_fixture: BASIC_NEW,
            extra_args: &["--key", KEY_LOAN_ID],
            expected_exit: 0,
            expected_outcome: "COMPATIBLE",
        },
        Scenario {
            old_fixture: TYPE_SHIFT_OLD,
            new_fixture: TYPE_SHIFT_NEW,
            extra_args: &[],
            expected_exit: 1,
            expected_outcome: "INCOMPATIBLE",
        },
        Scenario {
            old_fixture: BASIC_OLD,
            new_fixture: EMPTY,
            extra_args: &[],
            expected_exit: 2,
            expected_outcome: "REFUSAL",
        },
    ];

    for scenario in scenarios {
        let human = run_human_scenario(&scenario);
        assert_human_outcome(&scenario, &human);

        let json = run_json_with_extra(
            scenario.old_fixture,
            scenario.new_fixture,
            scenario.extra_args,
        );
        assert_json_outcome(&scenario, &json);
    }
}

#[test]
fn e2e_refusal_from_new_parse_failure_preserves_old_context() {
    let human = run_shape_with_fixtures(BASIC_OLD, NO_HEADER, &[]);
    assert_eq!(human.status, 2);
    assert!(human.stdout.trim().is_empty());
    assert!(human.stderr.contains("SHAPE ERROR (E_HEADERS)"));
    assert_contains_compared_line(
        &human.stderr,
        BASIC_OLD,
        NO_HEADER,
        "human refusal should include compared line",
    );
    assert!(
        human
            .stderr
            .contains("Dialect(old): delimiter=, quote=\" escape=none")
    );
    assert!(
        !human.stderr.contains("Dialect(new):"),
        "new dialect should be absent when new parsing fails before dialect is known"
    );

    let json = run_shape_with_fixtures(BASIC_OLD, NO_HEADER, &["--json"]);
    assert_eq!(json.status, 2);
    assert!(json.stderr.trim().is_empty());

    let payload = parse_json_output(&json, "json refusal should parse");
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_HEADERS");
    assert!(payload["dialect"]["old"].is_object());
    assert!(payload["dialect"]["new"].is_null());
}

#[test]
fn e2e_refusal_from_old_parse_failure_omits_both_dialect_contexts() {
    let human = run_shape_with_fixtures(NO_HEADER, BASIC_NEW, &[]);
    assert_eq!(human.status, 2);
    assert!(human.stdout.trim().is_empty());
    assert!(human.stderr.contains("SHAPE ERROR (E_HEADERS)"));
    assert_contains_compared_line(
        &human.stderr,
        NO_HEADER,
        BASIC_NEW,
        "human refusal should include compared line",
    );
    assert!(
        !human.stderr.contains("Dialect(old):"),
        "old dialect should be absent when old parsing fails before dialect is known"
    );
    assert!(
        !human.stderr.contains("Dialect(new):"),
        "new dialect should be absent when old parsing fails before new parsing starts"
    );

    let json = run_shape_with_fixtures(NO_HEADER, BASIC_NEW, &["--json"]);
    assert_eq!(json.status, 2);
    assert!(json.stderr.trim().is_empty());

    let payload = parse_json_output(&json, "json refusal should parse");
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_HEADERS");
    assert!(payload["dialect"]["old"].is_null());
    assert!(payload["dialect"]["new"].is_null());
}

#[test]
fn e2e_refusal_dialect_includes_candidates_and_actionable_next_command() {
    let human = run_shape_with_fixtures(DIALECT_AMBIGUOUS, BASIC_NEW, &[]);
    assert_eq!(human.status, 2);
    assert!(human.stdout.trim().is_empty());
    assert!(human.stderr.contains("SHAPE ERROR (E_DIALECT)"));
    assert_contains_compared_line(
        &human.stderr,
        DIALECT_AMBIGUOUS,
        BASIC_NEW,
        "human refusal should include compared line",
    );
    assert!(!human.stderr.contains("Dialect(old):"));
    assert!(!human.stderr.contains("Dialect(new):"));

    let json = run_shape_with_fixtures(DIALECT_AMBIGUOUS, BASIC_NEW, &["--json"]);
    assert_eq!(json.status, 2);
    assert!(json.stderr.trim().is_empty());

    let payload = parse_json_output(&json, "json refusal should parse");
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_DIALECT");
    assert_eq!(
        payload["refusal"]["detail"]["candidates"],
        serde_json::json!(["0x2c", "0x3b"])
    );
    assert_eq!(
        payload["refusal"]["next_command"],
        format!(
            "shape {} {} --delimiter comma --json",
            fixture_path(DIALECT_AMBIGUOUS).display(),
            fixture_path(BASIC_NEW).display()
        )
    );
    assert!(payload["dialect"]["old"].is_null());
    assert!(payload["dialect"]["new"].is_null());
}

#[test]
fn e2e_refusal_from_new_forced_delimiter_parse_failure_keeps_both_contexts() {
    let old = fixture_path(BASIC_OLD);
    let new = TempCsv::new("e2e-forced-delimiter-new", "loan_id,balance\n\"A1,100\n");

    let human = run_shape_with_paths(
        &old.to_string_lossy(),
        &new.path.to_string_lossy(),
        &["--delimiter", ","],
    );
    assert_eq!(human.status, 2);
    assert!(human.stdout.trim().is_empty());
    assert!(human.stderr.contains("SHAPE ERROR (E_CSV_PARSE)"));
    assert!(
        human.stderr.contains(&format!(
            "Compared: {} -> {}",
            old.to_string_lossy(),
            new.path.to_string_lossy()
        )),
        "human refusal should include compared line"
    );
    assert!(human.stderr.contains("Dialect(old):"));
    assert!(human.stderr.contains("Dialect(new):"));

    let json = run_shape_with_paths(
        &old.to_string_lossy(),
        &new.path.to_string_lossy(),
        &["--delimiter", ",", "--json"],
    );
    assert_eq!(json.status, 2);
    assert!(json.stderr.trim().is_empty());

    let payload = parse_json_output(&json, "json forced-delimiter refusal should parse");
    assert_eq!(payload["outcome"], "REFUSAL");
    assert_eq!(payload["refusal"]["code"], "E_CSV_PARSE");
    assert_eq!(payload["files"]["old"], old.to_string_lossy().as_ref());
    assert_eq!(payload["files"]["new"], new.path.to_string_lossy().as_ref());
    assert_eq!(payload["dialect"]["old"]["delimiter"], ",");
    assert_eq!(payload["dialect"]["new"]["delimiter"], ",");
}

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_path(label: &str) -> PathBuf {
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "shape-e2e-matrix-{label}-{}-{counter}-{ts}.csv",
        std::process::id(),
    ))
}

struct TempCsv {
    path: PathBuf,
}

impl TempCsv {
    fn new(label: &str, contents: &str) -> Self {
        let path = unique_temp_path(label);
        fs::write(&path, contents).expect("failed to write temporary CSV fixture");
        Self { path }
    }
}

impl Drop for TempCsv {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
