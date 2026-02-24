use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use shape::checks::suite::Outcome;
use shape::cli::args::Args;
use shape::orchestrator::{PipelineResult, run};

const KEY_LOAN_ID: &str = "loan_id";

#[test]
fn run_handoff_emits_compatible_in_human_and_json_modes() {
    let (old, new) = temp_pair(
        "handoff-compatible-old",
        "loan_id,balance\nA1,100\nA2,200\n",
        "handoff-compatible-new",
        "loan_id,balance\nA1,150\nA2,250\n",
    );

    let (human, json) = run_human_and_json_modes(
        &old.path,
        &new.path,
        Some(KEY_LOAN_ID),
        "human compatible run should succeed",
        "json compatible run should succeed",
    );
    assert_eq!(human.outcome, Outcome::Compatible);
    assert!(human.output.contains("SHAPE\n\nCOMPATIBLE"));
    assert_compared_line(
        &human.output,
        &old.path,
        &new.path,
        "human output should include compared line",
    );
    assert!(human.output.contains("Schema:"));
    assert!(human.output.contains("Rows:"));
    assert!(human.output.contains("Types:"));

    assert_eq!(json.outcome, Outcome::Compatible);
    let value = parse_json_output(&json, "valid compatible json output");
    assert_eq!(value["outcome"], "COMPATIBLE");
    assert!(value["checks"].is_object());
    assert!(
        value["reasons"]
            .as_array()
            .expect("reasons array expected")
            .is_empty()
    );
    assert!(value["refusal"].is_null());
}

#[test]
fn run_handoff_emits_incompatible_in_human_and_json_modes() {
    let (old, new) = temp_pair(
        "handoff-incompatible-old",
        "loan_id,balance\nA1,100\nA2,200\n",
        "handoff-incompatible-new",
        "loan_id,balance\nA1,not_numeric\nA2,still_not_numeric\n",
    );

    let (human, json) = run_human_and_json_modes(
        &old.path,
        &new.path,
        Some(KEY_LOAN_ID),
        "human incompatible run should succeed",
        "json incompatible run should succeed",
    );
    assert_eq!(human.outcome, Outcome::Incompatible);
    assert!(human.output.contains("INCOMPATIBLE"));
    assert!(human.output.contains("Reasons:"));
    assert!(
        human
            .output
            .contains("Type shift: balance changed from numeric to non-numeric")
    );

    assert_eq!(json.outcome, Outcome::Incompatible);
    let value = parse_json_output(&json, "valid incompatible json");
    assert_eq!(value["outcome"], "INCOMPATIBLE");
    assert_eq!(value["checks"]["type_consistency"]["status"], "fail");
    assert_eq!(
        value["checks"]["type_consistency"]["type_shifts"][0]["column"],
        "u8:balance"
    );
    assert_eq!(
        value["reasons"][0],
        "Type shift: balance changed from numeric to non-numeric"
    );
    assert!(value["refusal"].is_null());
}

#[test]
fn run_refusal_keeps_old_context_when_new_file_read_fails() {
    let old = TempCsv::new("handoff-refusal-old", "loan_id,balance\nA1,100\n");
    let missing_new = unique_path("handoff-missing-new");

    let (human, json) = run_human_and_json_modes(
        &old.path,
        &missing_new,
        Some(KEY_LOAN_ID),
        "human refusal run should succeed",
        "json refusal run should succeed",
    );
    assert_eq!(human.outcome, Outcome::Refusal);
    assert!(human.output.contains("SHAPE ERROR (E_IO)"));
    assert_compared_line(
        &human.output,
        &old.path,
        &missing_new,
        "refusal output should include compared line",
    );
    assert!(
        human
            .output
            .contains("Dialect(old): delimiter=, quote=\" escape=none"),
        "old dialect context should be preserved on new-file IO refusal"
    );
    assert!(
        !human.output.contains("Dialect(new):"),
        "new dialect should be omitted when new file cannot be read"
    );

    assert_eq!(json.outcome, Outcome::Refusal);
    let value = parse_json_output(&json, "valid refusal json");
    assert_eq!(value["outcome"], "REFUSAL");
    assert!(value["checks"].is_null());
    assert!(value["reasons"].is_null());
    assert_eq!(value["refusal"]["code"], "E_IO");
    assert_eq!(value["dialect"]["old"]["delimiter"], ",");
    assert!(value["dialect"]["new"].is_null());
}

#[test]
fn run_refusal_from_old_parse_failure_omits_both_dialect_contexts() {
    let old = TempCsv::new("handoff-old-ambiguous", "a,b;c\n1,2;3\n");
    let new = TempCsv::new("handoff-new-valid", "loan_id,balance\nA1,100\n");

    let (human, json) = run_human_and_json_modes(
        &old.path,
        &new.path,
        None,
        "human old-parse refusal run should succeed",
        "json old-parse refusal run should succeed",
    );
    assert_eq!(human.outcome, Outcome::Refusal);
    assert!(human.output.contains("SHAPE ERROR (E_DIALECT)"));
    assert_compared_line(
        &human.output,
        &old.path,
        &new.path,
        "refusal output should include compared line",
    );
    assert!(
        !human.output.contains("Dialect(old):"),
        "old dialect should be omitted when old parsing fails before dialect is known"
    );
    assert!(
        !human.output.contains("Dialect(new):"),
        "new dialect should be omitted when old parsing fails before new parse starts"
    );

    assert_eq!(json.outcome, Outcome::Refusal);
    let value = parse_json_output(&json, "valid old-parse refusal json");
    assert_eq!(value["outcome"], "REFUSAL");
    assert_eq!(value["refusal"]["code"], "E_DIALECT");
    assert!(value["checks"].is_null());
    assert!(value["reasons"].is_null());
    assert!(value["dialect"]["old"].is_null());
    assert!(value["dialect"]["new"].is_null());
}

#[test]
fn run_refusal_from_new_forced_delimiter_parse_failure_keeps_both_dialects() {
    let old = TempCsv::new("handoff-forced-delim-old", "loan_id,balance\nA1,100\n");
    let new = TempCsv::new("handoff-forced-delim-new", "loan_id,balance\n\"A1,100\n");

    let (human, json) = run_human_and_json_modes_with_delimiter(
        &old.path,
        &new.path,
        Some(KEY_LOAN_ID),
        Some(","),
        "human forced-delimiter refusal run should succeed",
        "json forced-delimiter refusal run should succeed",
    );
    assert_eq!(human.outcome, Outcome::Refusal);
    assert!(human.output.contains("SHAPE ERROR (E_CSV_PARSE)"));
    assert_compared_line(
        &human.output,
        &old.path,
        &new.path,
        "forced-delimiter refusal output should include compared line",
    );
    assert!(
        human.output.contains("Dialect(old):"),
        "old dialect should be present after successful old parse"
    );
    assert!(
        human.output.contains("Dialect(new):"),
        "new dialect should be present for forced-delimiter parse refusal"
    );

    assert_eq!(json.outcome, Outcome::Refusal);
    let value = parse_json_output(&json, "valid forced-delimiter refusal json");
    assert_eq!(value["outcome"], "REFUSAL");
    assert_eq!(value["refusal"]["code"], "E_CSV_PARSE");
    assert_eq!(value["dialect"]["old"]["delimiter"], ",");
    assert_eq!(value["dialect"]["new"]["delimiter"], ",");
}

#[test]
fn run_compatible_with_forced_literal_delimiter_reports_literal_dialects() {
    let (old, new) = temp_pair(
        "handoff-forced-literal-old",
        "loan_id=balance\nA1=100\nA2=200\n",
        "handoff-forced-literal-new",
        "loan_id=balance\nA1=120\nA3=300\n",
    );

    let (human, json) = run_human_and_json_modes_with_delimiter(
        &old.path,
        &new.path,
        Some(KEY_LOAN_ID),
        Some("="),
        "human forced-literal-delimiter run should succeed",
        "json forced-literal-delimiter run should succeed",
    );

    assert_eq!(human.outcome, Outcome::Compatible);
    assert!(human.output.contains("COMPATIBLE"));
    assert!(
        human
            .output
            .contains("Dialect(old): delimiter== quote=\" escape=none")
    );
    assert!(
        human
            .output
            .contains("Dialect(new): delimiter== quote=\" escape=none")
    );

    assert_eq!(json.outcome, Outcome::Compatible);
    let value = parse_json_output(&json, "valid forced-literal-delimiter json");
    assert_eq!(value["outcome"], "COMPATIBLE");
    assert_eq!(value["dialect"]["old"]["delimiter"], "=");
    assert_eq!(value["dialect"]["new"]["delimiter"], "=");
}

fn args_with_delimiter(
    old: PathBuf,
    new: PathBuf,
    key: Option<&str>,
    json: bool,
    delimiter: Option<&str>,
) -> Args {
    Args {
        old: Some(old),
        new: Some(new),
        key: key.map(ToOwned::to_owned),
        delimiter: delimiter.map(ToOwned::to_owned),
        json,
        no_witness: false,
        capsule_dir: None,
        profile: None,
        profile_id: None,
        lock: vec![],
        max_rows: None,
        max_bytes: None,
        describe: false,
        command: None,
    }
}

fn run_mode_with_delimiter(
    old: PathBuf,
    new: PathBuf,
    key: Option<&str>,
    json: bool,
    delimiter: Option<&str>,
    context: &str,
) -> PipelineResult {
    run(&args_with_delimiter(old, new, key, json, delimiter)).expect(context)
}

fn run_human_and_json_modes(
    old: &Path,
    new: &Path,
    key: Option<&str>,
    human_context: &str,
    json_context: &str,
) -> (PipelineResult, PipelineResult) {
    run_human_and_json_modes_with_delimiter(old, new, key, None, human_context, json_context)
}

fn run_human_and_json_modes_with_delimiter(
    old: &Path,
    new: &Path,
    key: Option<&str>,
    delimiter: Option<&str>,
    human_context: &str,
    json_context: &str,
) -> (PipelineResult, PipelineResult) {
    let human = run_mode_with_delimiter(
        old.to_path_buf(),
        new.to_path_buf(),
        key,
        false,
        delimiter,
        human_context,
    );
    let json = run_mode_with_delimiter(
        old.to_path_buf(),
        new.to_path_buf(),
        key,
        true,
        delimiter,
        json_context,
    );
    (human, json)
}

fn parse_json_output(result: &PipelineResult, context: &str) -> Value {
    serde_json::from_str(&result.output).expect(context)
}

fn temp_pair(
    old_label: &str,
    old_contents: &str,
    new_label: &str,
    new_contents: &str,
) -> (TempCsv, TempCsv) {
    (
        TempCsv::new(old_label, old_contents),
        TempCsv::new(new_label, new_contents),
    )
}

fn assert_compared_line(output: &str, old: &Path, new: &Path, context: &str) {
    assert!(
        output.contains(&format!(
            "Compared: {} -> {}",
            old.to_string_lossy(),
            new.to_string_lossy()
        )),
        "{context}"
    );
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_path(label: &str) -> PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "shape-orchestrator-handoff-{label}-{}-{counter}-{ts}.csv",
        std::process::id(),
    ))
}

struct TempCsv {
    path: PathBuf,
}

impl TempCsv {
    fn new(label: &str, contents: &str) -> Self {
        let path = unique_path(label);
        fs::write(&path, contents).expect("failed to write temporary CSV fixture");
        Self { path }
    }
}

impl Drop for TempCsv {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
