use std::collections::HashMap;
use std::path::Path;

use crate::capsule;
use crate::checks::suite::{
    CheckSuite, Outcome, assemble_check_suite, build_reasons, determine_outcome,
};
use crate::cli::args::Args;
use crate::cli::delimiter::parse_delimiter;
use crate::csv::dialect::Dialect;
use crate::csv::input::{ParsedInput, parse_input_file_with_context};
use crate::normalize::headers::ascii_trim;
use crate::output::human::{self, RefusalRenderContext};
use crate::output::json::{self, JsonRenderContext};
use crate::refusal::payload::RefusalPayload;
use crate::scan::{ScanResult, post_scan_empty_guard, pre_scan_empty_guard, scan_file};

/// Top-level result returned by the orchestration pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineResult {
    pub outcome: Outcome,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq)]
struct PipelineDomainResult {
    old_input: ParsedInput,
    new_input: ParsedInput,
    suite: CheckSuite,
    reasons: Vec<String>,
    outcome: Outcome,
}

#[derive(Debug, Clone)]
struct PipelineRefusal {
    refusal: RefusalPayload,
    dialect_old: Option<Dialect>,
    dialect_new: Option<Dialect>,
}

/// Apply PLAN step-11 quick empty guards after both headers are parsed.
pub fn enforce_pre_scan_empty_guards(
    old_input: &ParsedInput,
    new_input: &ParsedInput,
) -> Result<(), RefusalPayload> {
    pre_scan_empty_guard(&old_input.path, &old_input.raw_bytes, old_input.data_offset)?;
    pre_scan_empty_guard(&new_input.path, &new_input.raw_bytes, new_input.data_offset)?;
    Ok(())
}

/// Apply PLAN step-16 post-scan empty guards (all-blank inputs).
pub fn enforce_post_scan_empty_guards(
    old_path: &Path,
    new_path: &Path,
    old_scan: &ScanResult,
    new_scan: &ScanResult,
) -> Result<(), RefusalPayload> {
    post_scan_empty_guard(old_path, old_scan.row_count)?;
    post_scan_empty_guard(new_path, new_scan.row_count)?;
    Ok(())
}

/// Execute the full shape pipeline.
pub fn run(args: &Args) -> Result<PipelineResult, Box<dyn std::error::Error>> {
    let old_path = args.old.as_deref().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing old input path")
    })?;
    let new_path = args.new.as_deref().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing new input path")
    })?;

    let forced_delimiter = args
        .delimiter
        .as_deref()
        .map(parse_delimiter)
        .transpose()
        .map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid --delimiter value: {error}"),
            )
        })?;

    let old_file = old_path.to_string_lossy().into_owned();
    let new_file = new_path.to_string_lossy().into_owned();

    let domain = match execute_pipeline(args, old_path, new_path, forced_delimiter) {
        Ok(result) => result,
        Err(refusal) => {
            let output = render_refusal_output(args, &old_file, &new_file, &refusal)?;
            if let Some(capsule_dir) = args.capsule_dir.as_deref() {
                capsule::write_run_capsule(
                    args,
                    Outcome::Refusal,
                    &output,
                    Some(&refusal.refusal),
                    capsule_dir,
                )?;
            }
            return Ok(PipelineResult {
                outcome: Outcome::Refusal,
                output,
            });
        }
    };

    let output = render_domain_output(args, &old_file, &new_file, &domain)?;
    if let Some(capsule_dir) = args.capsule_dir.as_deref() {
        capsule::write_run_capsule(args, domain.outcome, &output, None, capsule_dir)?;
    }
    Ok(PipelineResult {
        outcome: domain.outcome,
        output,
    })
}

fn execute_pipeline(
    args: &Args,
    old_path: &Path,
    new_path: &Path,
    forced_delimiter: Option<u8>,
) -> Result<PipelineDomainResult, PipelineRefusal> {
    let old_file = old_path.to_string_lossy().into_owned();
    let new_file = new_path.to_string_lossy().into_owned();

    let old_input = parse_input_file_with_context(old_path, forced_delimiter, &old_file, &new_file)
        .map_err(|err| PipelineRefusal {
            refusal: err.refusal,
            dialect_old: err.dialect,
            dialect_new: None,
        })?;

    let new_input = parse_input_file_with_context(new_path, forced_delimiter, &old_file, &new_file)
        .map_err(|err| PipelineRefusal {
            refusal: err.refusal,
            dialect_old: Some(old_input.dialect),
            dialect_new: err.dialect,
        })?;

    enforce_pre_scan_empty_guards(&old_input, &new_input).map_err(|refusal| PipelineRefusal {
        refusal,
        dialect_old: Some(old_input.dialect),
        dialect_new: Some(new_input.dialect),
    })?;

    let common_columns = {
        let schema = crate::checks::schema_overlap::evaluate_schema_overlap(
            &old_input.headers,
            &new_input.headers,
        );
        schema.columns_common
    };
    let old_header_indices = header_index_map(&old_input.headers);
    let new_header_indices = header_index_map(&new_input.headers);
    let old_common_indices = common_column_indices(&common_columns, &old_header_indices);
    let new_common_indices = common_column_indices(&common_columns, &new_header_indices);

    let key_column = args
        .key
        .as_ref()
        .map(|column| ascii_trim(column.as_bytes()).to_vec());
    let old_key_index = key_column
        .as_ref()
        .and_then(|key| old_header_indices.get(key.as_slice()).copied());
    let new_key_index = key_column
        .as_ref()
        .and_then(|key| new_header_indices.get(key.as_slice()).copied());
    let key_found_old = old_key_index.is_some();
    let key_found_new = new_key_index.is_some();

    let old_scan = scan_file(
        old_path,
        &old_input.raw_bytes,
        old_input.data_offset,
        &old_input.dialect,
        &old_common_indices,
        old_key_index,
    )
    .map_err(|refusal| PipelineRefusal {
        refusal,
        dialect_old: Some(old_input.dialect),
        dialect_new: Some(new_input.dialect),
    })?;

    let new_scan = scan_file(
        new_path,
        &new_input.raw_bytes,
        new_input.data_offset,
        &new_input.dialect,
        &new_common_indices,
        new_key_index,
    )
    .map_err(|refusal| PipelineRefusal {
        refusal,
        dialect_old: Some(old_input.dialect),
        dialect_new: Some(new_input.dialect),
    })?;

    enforce_post_scan_empty_guards(old_path, new_path, &old_scan, &new_scan).map_err(
        |refusal| PipelineRefusal {
            refusal,
            dialect_old: Some(old_input.dialect),
            dialect_new: Some(new_input.dialect),
        },
    )?;

    let suite = assemble_check_suite(
        &old_input.headers,
        &new_input.headers,
        key_column,
        key_found_old,
        key_found_new,
        &old_scan,
        &new_scan,
    );
    let reasons = build_reasons(&suite);
    let outcome = determine_outcome(&suite);

    Ok(PipelineDomainResult {
        old_input,
        new_input,
        suite,
        reasons,
        outcome,
    })
}

fn render_domain_output(
    args: &Args,
    old_file: &str,
    new_file: &str,
    domain: &PipelineDomainResult,
) -> Result<String, Box<dyn std::error::Error>> {
    if args.json {
        return Ok(json::render_shape_json(JsonRenderContext {
            outcome: domain.outcome,
            old_file,
            new_file,
            dialect_old: Some(domain.old_input.dialect),
            dialect_new: Some(domain.new_input.dialect),
            checks: Some(&domain.suite),
            reasons: Some(&domain.reasons),
            refusal: None,
            profile_id: args.profile_id.as_deref(),
            profile_sha256: None,
            input_verification: None,
        })?);
    }

    let output = match domain.outcome {
        Outcome::Compatible => human::render_compatible(
            old_file,
            new_file,
            domain.old_input.dialect,
            domain.new_input.dialect,
            &domain.suite,
        ),
        Outcome::Incompatible => human::render_incompatible(
            old_file,
            new_file,
            domain.old_input.dialect,
            domain.new_input.dialect,
            &domain.suite,
            &domain.reasons,
        ),
        Outcome::Refusal => {
            return Err(std::io::Error::other(
                "internal invariant violated: domain pipeline returned refusal outcome",
            )
            .into());
        }
    };
    Ok(output)
}

fn render_refusal_output(
    args: &Args,
    old_file: &str,
    new_file: &str,
    refusal: &PipelineRefusal,
) -> Result<String, Box<dyn std::error::Error>> {
    if args.json {
        return Ok(json::render_shape_json(JsonRenderContext {
            outcome: Outcome::Refusal,
            old_file,
            new_file,
            dialect_old: refusal.dialect_old,
            dialect_new: refusal.dialect_new,
            checks: None,
            reasons: None,
            refusal: Some(&refusal.refusal),
            profile_id: args.profile_id.as_deref(),
            profile_sha256: None,
            input_verification: None,
        })?);
    }

    Ok(human::render_refusal_with_context(
        &refusal.refusal,
        &RefusalRenderContext {
            old_path: old_file,
            new_path: new_file,
            dialect_old: refusal.dialect_old,
            dialect_new: refusal.dialect_new,
        },
    ))
}

fn header_index_map(headers: &[Vec<u8>]) -> HashMap<Vec<u8>, usize> {
    headers
        .iter()
        .enumerate()
        .map(|(index, column)| (column.clone(), index))
        .collect()
}

fn common_column_indices(
    common_columns: &[Vec<u8>],
    header_indices: &HashMap<Vec<u8>, usize>,
) -> Vec<usize> {
    common_columns
        .iter()
        .filter_map(|column| header_indices.get(column.as_slice()).copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::checks::suite::CheckStatus;
    use crate::cli::args::Args;
    use crate::csv::dialect::Dialect;
    use crate::csv::input::ParsedInput;
    use crate::refusal::codes::RefusalCode;
    use crate::scan::{ColumnClassification, ScanResult};

    use super::{enforce_post_scan_empty_guards, enforce_pre_scan_empty_guards, execute_pipeline};

    fn parsed_input(path: &str, raw_bytes: &[u8], data_offset: usize) -> ParsedInput {
        ParsedInput::new(
            PathBuf::from(path),
            raw_bytes.to_vec(),
            Dialect::default(),
            vec![b"loan_id".to_vec()],
            data_offset,
        )
    }

    fn scan_result(row_count: u64) -> ScanResult {
        ScanResult {
            row_count,
            key_scan: None,
            column_types: vec![ColumnClassification::AllMissing],
        }
    }

    #[test]
    fn pre_scan_empty_guards_reject_header_only_input() {
        let old = parsed_input("old.csv", b"loan_id,balance\n", "loan_id,balance\n".len());
        let new = parsed_input(
            "new.csv",
            b"loan_id,balance\n1,2\n",
            "loan_id,balance\n".len(),
        );

        let refusal = enforce_pre_scan_empty_guards(&old, &new)
            .expect_err("header-only old file should be rejected at step 11");
        assert_eq!(refusal.code.as_str(), "E_EMPTY");
        assert_eq!(refusal.detail["file"].as_str(), Some("old.csv"));
    }

    #[test]
    fn post_scan_empty_guards_reject_all_blank_dataset() {
        let old_scan = scan_result(0);
        let new_scan = scan_result(2);

        let refusal = enforce_post_scan_empty_guards(
            PathBuf::from("old.csv").as_path(),
            PathBuf::from("new.csv").as_path(),
            &old_scan,
            &new_scan,
        )
        .expect_err("all-blank old dataset should be rejected at step 16");
        assert_eq!(refusal.code.as_str(), "E_EMPTY");
        assert_eq!(refusal.detail["file"].as_str(), Some("old.csv"));
    }

    #[test]
    fn execute_pipeline_fails_fast_when_old_input_cannot_be_read() {
        let old_missing = unique_path("missing-old");
        let new_file = TempCsv::new("new-valid", "loan_id,balance\nA1,100\n");
        let args = args(old_missing.clone(), new_file.path.clone(), None, false);

        let refusal = execute_pipeline(&args, old_missing.as_path(), new_file.path.as_path(), None)
            .expect_err("missing old file should fail fast");

        assert_eq!(refusal.refusal.code, RefusalCode::EIo);
        assert_eq!(
            refusal.refusal.detail["file"].as_str(),
            Some(old_missing.to_string_lossy().as_ref())
        );
        assert!(refusal.dialect_old.is_none());
        assert!(refusal.dialect_new.is_none());
    }

    #[test]
    fn execute_pipeline_preserves_old_context_when_new_parsing_fails() {
        let old_file = TempCsv::new("old-valid", "loan_id,balance\nA1,100\n");
        let new_file = TempCsv::new("new-duplicate", "loan_id,loan_id\nA1,100\n");
        let args = args(old_file.path.clone(), new_file.path.clone(), None, false);

        let refusal = execute_pipeline(
            &args,
            old_file.path.as_path(),
            new_file.path.as_path(),
            None,
        )
        .expect_err("duplicate headers in new file should refuse");

        assert_eq!(refusal.refusal.code, RefusalCode::EHeaders);
        assert_eq!(
            refusal.refusal.detail["file"].as_str(),
            Some(new_file.path.to_string_lossy().as_ref())
        );
        assert_eq!(refusal.dialect_old, Some(Dialect::default()));
        assert_eq!(refusal.dialect_new, Some(Dialect::default()));
    }

    #[test]
    fn execute_pipeline_happy_path_with_key_is_compatible() {
        let old_file = TempCsv::new("old-key", "loan_id,balance\nA1,100\nA2,200\n");
        let new_file = TempCsv::new("new-key", "loan_id,balance\nA1,150\nA2,250\n");
        let args = args(
            old_file.path.clone(),
            new_file.path.clone(),
            Some("loan_id"),
            false,
        );

        let domain = execute_pipeline(
            &args,
            old_file.path.as_path(),
            new_file.path.as_path(),
            None,
        )
        .expect("pipeline should succeed");

        assert_eq!(domain.outcome, crate::checks::suite::Outcome::Compatible);
        let key_viability = domain
            .suite
            .key_viability
            .as_ref()
            .expect("key viability should be present when --key is provided");
        assert_eq!(key_viability.status, CheckStatus::Pass);
        assert_eq!(domain.suite.row_granularity.rows_old, 2);
        assert_eq!(domain.suite.row_granularity.rows_new, 2);
    }

    #[test]
    fn execute_pipeline_happy_path_without_key_uses_nullable_key_fields() {
        let old_file = TempCsv::new("old-no-key", "loan_id,balance\nA1,100\nA2,200\n");
        let new_file = TempCsv::new("new-no-key", "loan_id,balance\nA1,150\nA2,250\n");
        let args = args(old_file.path.clone(), new_file.path.clone(), None, false);

        let domain = execute_pipeline(
            &args,
            old_file.path.as_path(),
            new_file.path.as_path(),
            None,
        )
        .expect("pipeline should succeed");

        assert_eq!(domain.outcome, crate::checks::suite::Outcome::Compatible);
        assert!(domain.suite.key_viability.is_none());
        assert!(domain.suite.row_granularity.key_overlap.is_none());
        assert!(domain.suite.row_granularity.keys_old_only.is_none());
        assert!(domain.suite.row_granularity.keys_new_only.is_none());
    }

    #[test]
    fn run_renders_compatible_outcome_for_human_and_json() {
        let old_file = TempCsv::new("run-compatible-old", "loan_id,balance\nA1,100\nA2,200\n");
        let new_file = TempCsv::new("run-compatible-new", "loan_id,balance\nA1,150\nA2,250\n");

        let human_args = args(
            old_file.path.clone(),
            new_file.path.clone(),
            Some("loan_id"),
            false,
        );
        let human = super::run(&human_args).expect("human run should succeed");
        assert_eq!(human.outcome, crate::checks::suite::Outcome::Compatible);
        assert!(human.output.contains("SHAPE\n\nCOMPATIBLE"));
        assert!(human.output.contains("Compared:"));

        let json_args = args(
            old_file.path.clone(),
            new_file.path.clone(),
            Some("loan_id"),
            true,
        );
        let json = super::run(&json_args).expect("json run should succeed");
        assert_eq!(json.outcome, crate::checks::suite::Outcome::Compatible);
        let value: serde_json::Value = serde_json::from_str(&json.output).expect("valid json");
        assert_eq!(value["outcome"], "COMPATIBLE");
        assert!(value["checks"].is_object());
        assert!(
            value["reasons"]
                .as_array()
                .is_some_and(|reasons| reasons.is_empty())
        );
        assert!(value["refusal"].is_null());
    }

    #[test]
    fn run_renders_incompatible_outcome_for_human_and_json() {
        let old_file = TempCsv::new("run-incompatible-old", "loan_id,balance\nA1,100\nA2,200\n");
        let new_file = TempCsv::new("run-incompatible-new", "id,amount\nA1,150\nA2,250\n");

        let human_args = args(
            old_file.path.clone(),
            new_file.path.clone(),
            Some("loan_id"),
            false,
        );
        let human = super::run(&human_args).expect("human run should succeed");
        assert_eq!(human.outcome, crate::checks::suite::Outcome::Incompatible);
        assert!(human.output.contains("INCOMPATIBLE"));
        assert!(human.output.contains("Reasons:"));

        let json_args = args(
            old_file.path.clone(),
            new_file.path.clone(),
            Some("loan_id"),
            true,
        );
        let json = super::run(&json_args).expect("json run should succeed");
        assert_eq!(json.outcome, crate::checks::suite::Outcome::Incompatible);
        let value: serde_json::Value = serde_json::from_str(&json.output).expect("valid json");
        assert_eq!(value["outcome"], "INCOMPATIBLE");
        assert!(value["checks"].is_object());
        assert!(
            value["reasons"]
                .as_array()
                .is_some_and(|reasons| !reasons.is_empty())
        );
        assert!(value["refusal"].is_null());
    }

    #[test]
    fn run_refusal_preserves_old_context_when_new_read_fails() {
        let old_file = TempCsv::new("run-refusal-old", "loan_id,balance\nA1,100\n");
        let missing_new = unique_path("run-refusal-missing-new");

        let human_args = args(old_file.path.clone(), missing_new.clone(), None, false);
        let human = super::run(&human_args).expect("human run should return refusal payload");
        assert_eq!(human.outcome, crate::checks::suite::Outcome::Refusal);
        assert!(human.output.contains("SHAPE ERROR (E_IO)"));
        assert!(
            human
                .output
                .contains("Dialect(old): delimiter=, quote=\" escape=none")
        );
        assert!(!human.output.contains("Dialect(new):"));

        let json_args = args(old_file.path.clone(), missing_new, None, true);
        let json = super::run(&json_args).expect("json run should return refusal payload");
        assert_eq!(json.outcome, crate::checks::suite::Outcome::Refusal);
        let value: serde_json::Value = serde_json::from_str(&json.output).expect("valid json");
        assert_eq!(value["outcome"], "REFUSAL");
        assert!(value["dialect"]["old"].is_object());
        assert!(value["dialect"]["new"].is_null());
        assert_eq!(value["refusal"]["code"], "E_IO");
    }

    #[test]
    fn run_refusal_from_old_parse_failure_omits_both_dialect_contexts() {
        let old_file = TempCsv::new("run-refusal-old-parse", "a,b;c\n1,2;3\n");
        let new_file = TempCsv::new("run-refusal-new-parse", "loan_id,balance\nA1,100\n");

        let human_args = args(old_file.path.clone(), new_file.path.clone(), None, false);
        let human = super::run(&human_args).expect("human run should return refusal payload");
        assert_eq!(human.outcome, crate::checks::suite::Outcome::Refusal);
        assert!(human.output.contains("SHAPE ERROR (E_DIALECT)"));
        assert!(!human.output.contains("Dialect(old):"));
        assert!(!human.output.contains("Dialect(new):"));

        let json_args = args(old_file.path.clone(), new_file.path.clone(), None, true);
        let json = super::run(&json_args).expect("json run should return refusal payload");
        assert_eq!(json.outcome, crate::checks::suite::Outcome::Refusal);
        let value: serde_json::Value = serde_json::from_str(&json.output).expect("valid json");
        assert_eq!(value["outcome"], "REFUSAL");
        assert!(value["dialect"]["old"].is_null());
        assert!(value["dialect"]["new"].is_null());
        assert_eq!(value["refusal"]["code"], "E_DIALECT");
    }

    #[test]
    fn run_refusal_from_scan_parse_failure_reports_source_line_number() {
        let old_file = TempCsv::new(
            "run-refusal-old-scan-parse",
            "loan_id,balance\nA1,100\nA2,200,extra\n",
        );
        let new_file = TempCsv::new("run-refusal-new-scan-parse", "loan_id,balance\nA1,110\n");

        let mut json_args = args(
            old_file.path.clone(),
            new_file.path.clone(),
            Some("loan_id"),
            true,
        );
        json_args.delimiter = Some("comma".to_string());
        let json = super::run(&json_args).expect("json run should return refusal payload");
        assert_eq!(json.outcome, crate::checks::suite::Outcome::Refusal);

        let value: serde_json::Value = serde_json::from_str(&json.output).expect("valid json");
        assert_eq!(value["refusal"]["code"], "E_CSV_PARSE");
        assert_eq!(
            value["refusal"]["detail"]["file"].as_str(),
            Some(old_file.path.to_string_lossy().as_ref())
        );
        assert_eq!(value["refusal"]["detail"]["line"].as_u64(), Some(3));
    }

    #[test]
    fn run_forced_delimiter_consumes_sep_directive_before_header_parse() {
        let old_file = TempCsv::new("run-forced-sep-old", "sep=;\nloan_id,balance\nA1,100\n");
        let new_file = TempCsv::new("run-forced-sep-new", "sep=;\nloan_id,balance\nA1,150\n");
        let mut args = args(
            old_file.path.clone(),
            new_file.path.clone(),
            Some("loan_id"),
            false,
        );
        args.delimiter = Some("comma".to_string());

        let result = super::run(&args).expect("forced delimiter run should succeed");
        assert_eq!(result.outcome, crate::checks::suite::Outcome::Compatible);
        assert!(result.output.contains("COMPATIBLE"));
        assert!(!result.output.contains("sep=;"));
    }

    #[test]
    fn run_with_capsule_dir_writes_manifest_and_artifacts() {
        let old_file = TempCsv::new("run-capsule-old", "loan_id,balance\nA1,100\nA2,200\n");
        let new_file = TempCsv::new("run-capsule-new", "loan_id,balance\nA1,110\nA2,210\n");
        let capsule_dir = unique_path("run-capsule-dir");
        let mut run_args = args(
            old_file.path.clone(),
            new_file.path.clone(),
            Some("loan_id"),
            true,
        );
        run_args.capsule_dir = Some(capsule_dir.clone());

        let result = super::run(&run_args).expect("run with capsule dir should succeed");
        assert_eq!(result.outcome, crate::checks::suite::Outcome::Compatible);
        assert!(capsule_dir.join("manifest.json").is_file());
        assert!(capsule_dir.join("inputs/old.csv").is_file());
        assert!(capsule_dir.join("inputs/new.csv").is_file());
        assert!(capsule_dir.join("outputs/report.txt").is_file());

        let manifest_text =
            fs::read_to_string(capsule_dir.join("manifest.json")).expect("read manifest");
        let manifest: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("manifest json should be valid");
        assert_eq!(manifest["schema_version"], "shape.capsule.v0");
        assert_eq!(manifest["result"]["outcome"], "COMPATIBLE");
        assert_eq!(manifest["replay"]["argv"][0], "shape");
        assert_eq!(manifest["replay"]["argv"][1], "inputs/old.csv");
        assert_eq!(manifest["replay"]["argv"][2], "inputs/new.csv");

        let _ = fs::remove_dir_all(capsule_dir);
    }

    fn args(old: PathBuf, new: PathBuf, key: Option<&str>, json: bool) -> Args {
        Args {
            old: Some(old),
            new: Some(new),
            key: key.map(ToOwned::to_owned),
            delimiter: None,
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

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_path(label: &str) -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "shape-orchestrator-{label}-{}-{counter}-{ts}.csv",
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
}
