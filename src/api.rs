//! Filesystem-free structural comparison for browser and embedded callers.
//!
//! The API accepts caller-owned bytes and returns the same `shape.v0` JSON value as the CLI.
//! It does not read paths, the environment, the clock or the witness ledger.  The CLI remains
//! on its existing path-bound orchestrator, so adding this API does not change CLI output.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::checks::schema_overlap::evaluate_schema_overlap;
use crate::checks::suite::{
    assemble_check_suite_with_min_coverage, build_reasons_for_output, determine_outcome,
};
use crate::csv::dialect::Dialect;
use crate::csv::input::{ParseInputError, parse_input_bytes_with_sheet};
use crate::normalize::headers::canonicalize_headers_or_refusal;
use crate::output::json::{JsonRenderContext, render_shape_json};
use crate::refusal::codes::RefusalCode;
use crate::refusal::payload::RefusalPayload;
use crate::scan::scan_file;

/// Explicit inputs for the in-memory comparison API.
#[derive(Clone, Debug, Default)]
pub struct EvaluateOptions {
    /// Optional name hints used for tabular format detection and report `files` labels.
    pub old_name_hint: Option<String>,
    pub new_name_hint: Option<String>,
    /// Optional worksheet selectors for workbook name hints.
    pub old_sheet: Option<String>,
    pub new_sheet: Option<String>,
    /// Override CSV delimiter detection for both inputs.
    pub delimiter: Option<u8>,
    /// Key columns, in canonical byte form, used by the existing key checks.
    pub key_columns: Vec<Vec<u8>>,
    /// Require this inclusive key-overlap ratio when both inputs have rows.
    pub min_coverage: Option<f64>,
    /// Show identifying column names in the report, matching CLI `--explicit`.
    pub explicit: bool,
}

/// Compare two CSV/TSV or, with the `tabular` feature, shared-reader containers in memory.
/// The returned value is the CLI's `shape.v0` JSON report without filesystem-only lock/profile
/// or witness fields.  Refusals remain typed `RefusalPayload` values.
pub fn evaluate_bytes(old: &[u8], new: &[u8]) -> Result<Value, RefusalPayload> {
    evaluate_bytes_with_options(old, new, &EvaluateOptions::default())
}

/// Compare two in-memory inputs using explicit format, key and rendering options.
pub fn evaluate_bytes_with_options(
    old: &[u8],
    new: &[u8],
    options: &EvaluateOptions,
) -> Result<Value, RefusalPayload> {
    let old_name = options.old_name_hint.as_deref().unwrap_or("old.csv");
    let new_name = options.new_name_hint.as_deref().unwrap_or("new.csv");

    let old_input = parse_input_bytes_with_sheet(
        old,
        old_name,
        options.old_sheet.as_deref(),
        options.delimiter,
        old_name,
        new_name,
    )
    .map_err(ParseInputError::into_refusal)?;
    let new_input = parse_input_bytes_with_sheet(
        new,
        new_name,
        options.new_sheet.as_deref(),
        options.delimiter,
        old_name,
        new_name,
    )
    .map_err(ParseInputError::into_refusal)?;

    let old_headers = canonicalize_headers_or_refusal(old_name, &old_input.headers, None)?;
    let new_headers = canonicalize_headers_or_refusal(new_name, &new_input.headers, None)?;
    let schema = evaluate_schema_overlap(&old_headers, &new_headers, None);
    let old_indices = header_index_map(&old_headers);
    let new_indices = header_index_map(&new_headers);
    let old_common_indices = common_column_indices(&schema.columns_common, &old_indices);
    let new_common_indices = common_column_indices(&schema.columns_common, &new_indices);
    let keys_found_old = found_components(&options.key_columns, &old_indices);
    let keys_found_new = found_components(&options.key_columns, &new_indices);
    let old_key_indices =
        key_indices_if_complete(&options.key_columns, &keys_found_old, &old_indices);
    let new_key_indices =
        key_indices_if_complete(&options.key_columns, &keys_found_new, &new_indices);

    let old_scan = scan_file(
        Path::new(old_name),
        &old_input.raw_bytes,
        old_input.data_offset,
        &old_input.dialect,
        &old_common_indices,
        &old_key_indices,
    )?;
    let new_scan = scan_file(
        Path::new(new_name),
        &new_input.raw_bytes,
        new_input.data_offset,
        &new_input.dialect,
        &new_common_indices,
        &new_key_indices,
    )?;

    let suite = assemble_check_suite_with_min_coverage(
        &old_headers,
        &new_headers,
        options.key_columns.clone(),
        keys_found_old,
        keys_found_new,
        &old_scan,
        &new_scan,
        None,
        options.min_coverage,
    );
    let outcome = determine_outcome(&suite);
    let reasons = build_reasons_for_output(&suite, options.explicit);
    let rendered = render_shape_json(JsonRenderContext {
        outcome,
        old_file: old_name,
        new_file: new_name,
        dialect_old: dialect_for_input(old_name, old_input.dialect),
        dialect_new: dialect_for_input(new_name, new_input.dialect),
        checks: Some(&suite),
        reasons: Some(&reasons),
        refusal: None,
        profile_id: None,
        profile_sha256: None,
        input_verification: None,
        explicit: options.explicit,
    })
    .map_err(|error| {
        RefusalPayload::new(
            RefusalCode::EInputFormat,
            "could not serialize shape.v0 report",
        )
        .with_detail(serde_json::json!({ "error": error.to_string() }))
    })?;

    serde_json::from_str(&rendered).map_err(|error| {
        RefusalPayload::new(RefusalCode::EInputFormat, "could not parse shape.v0 report")
            .with_detail(serde_json::json!({ "error": error.to_string() }))
    })
}

/// Compare two already table-shaped in-memory CSV byte streams. This is an explicit alias for
/// callers that have already selected a container and want the default CSV name hints.
pub fn evaluate_tables(old: &[u8], new: &[u8]) -> Result<Value, RefusalPayload> {
    evaluate_bytes(old, new)
}

fn dialect_for_input(name: &str, dialect: Dialect) -> Option<Dialect> {
    if crate::tabular_input::is_tabular_name(name) {
        None
    } else {
        Some(dialect)
    }
}

fn header_index_map(headers: &[Vec<u8>]) -> HashMap<Vec<u8>, usize> {
    headers
        .iter()
        .enumerate()
        .map(|(index, header)| (header.clone(), index))
        .collect()
}

fn common_column_indices(
    common_columns: &[Vec<u8>],
    header_indices: &HashMap<Vec<u8>, usize>,
) -> Vec<usize> {
    common_columns
        .iter()
        .filter_map(|column| header_indices.get(column).copied())
        .collect()
}

fn found_components(
    key_columns: &[Vec<u8>],
    header_indices: &HashMap<Vec<u8>, usize>,
) -> Vec<bool> {
    key_columns
        .iter()
        .map(|column| header_indices.contains_key(column))
        .collect()
}

fn key_indices_if_complete(
    key_columns: &[Vec<u8>],
    found_components: &[bool],
    header_indices: &HashMap<Vec<u8>, usize>,
) -> Vec<usize> {
    if key_columns.is_empty() || !found_components.iter().all(|&found| found) {
        return Vec::new();
    }

    key_columns
        .iter()
        .filter_map(|column| header_indices.get(column).copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{EvaluateOptions, evaluate_bytes, evaluate_bytes_with_options, evaluate_tables};

    const OLD: &[u8] = b"loan_id,balance\nA,10\nB,20\n";
    const NEW: &[u8] = b"loan_id,balance\nA,11\nB,21\n";

    #[test]
    fn bytes_api_returns_shape_v0_and_matches_table_alias() {
        let report = evaluate_bytes(OLD, NEW).expect("CSV bytes should compare");
        assert_eq!(report["version"], "shape.v0");
        assert_eq!(report["outcome"], "COMPATIBLE");
        assert_eq!(report["files"]["old"], "old.csv");
        assert_eq!(
            report,
            evaluate_tables(OLD, NEW).expect("alias should match")
        );
    }

    #[test]
    fn bytes_api_preserves_explicit_and_key_options_without_filesystem_access() {
        let options = EvaluateOptions {
            old_name_hint: Some("old.tsv".to_owned()),
            new_name_hint: Some("new.tsv".to_owned()),
            delimiter: Some(b'\t'),
            key_columns: vec![b"loan_id".to_vec()],
            explicit: true,
            ..EvaluateOptions::default()
        };
        let report = evaluate_bytes_with_options(
            b"loan_id\tbalance\nA\t10\nB\t20\n",
            b"loan_id\tbalance\nA\t11\nB\t21\n",
            &options,
        )
        .expect("TSV bytes should compare");
        assert_eq!(report["dialect"]["old"]["delimiter"], "\\t");
        assert_eq!(
            report["checks"]["key_viability"]["key_column"],
            "u8:loan_id"
        );
    }

    #[cfg(feature = "tabular")]
    #[test]
    fn bytes_api_uses_tabular_open_bytes_for_jsonl() {
        let options = EvaluateOptions {
            old_name_hint: Some("old.jsonl".to_owned()),
            new_name_hint: Some("new.jsonl".to_owned()),
            ..EvaluateOptions::default()
        };
        let report = evaluate_bytes_with_options(
            b"{\"loan_id\":\"A\",\"balance\":10}\n",
            b"{\"loan_id\":\"A\",\"balance\":11}\n",
            &options,
        )
        .expect("JSONL bytes should compare through tabular");
        assert_eq!(report["version"], "shape.v0");
        assert_eq!(report["dialect"]["old"], serde_json::Value::Null);
    }
}
