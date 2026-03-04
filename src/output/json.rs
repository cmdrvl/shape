use serde::Serialize;
use serde_json::Value;

use crate::checks::suite::{CheckStatus, CheckSuite, Outcome};
use crate::csv::dialect::{Dialect, EscapeMode};
use crate::format::ident::encode_identifier;
use crate::refusal::payload::RefusalPayload;
use crate::scan::ColumnClassification;

#[derive(Debug, Clone)]
pub struct JsonRenderContext<'a> {
    pub outcome: Outcome,
    pub old_file: &'a str,
    pub new_file: &'a str,
    pub dialect_old: Option<Dialect>,
    pub dialect_new: Option<Dialect>,
    pub checks: Option<&'a CheckSuite>,
    pub reasons: Option<&'a [String]>,
    pub refusal: Option<&'a RefusalPayload>,
    pub profile_id: Option<&'a str>,
    pub profile_sha256: Option<&'a str>,
    pub input_verification: Option<Value>,
    pub explicit: bool,
}

impl<'a> JsonRenderContext<'a> {
    pub fn minimal(outcome: Outcome, refusal: Option<&'a RefusalPayload>) -> Self {
        Self {
            outcome,
            old_file: "",
            new_file: "",
            dialect_old: None,
            dialect_new: None,
            checks: None,
            reasons: None,
            refusal,
            profile_id: None,
            profile_sha256: None,
            input_verification: None,
            explicit: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct JsonOutcome<'a> {
    version: &'static str,
    outcome: &'static str,
    profile_id: Option<&'a str>,
    profile_sha256: Option<&'a str>,
    input_verification: Option<Value>,
    files: JsonFiles<'a>,
    dialect: JsonDialectContext,
    checks: Option<JsonChecks>,
    reasons: Option<Vec<String>>,
    refusal: Option<&'a RefusalPayload>,
}

#[derive(Debug, Clone, Serialize)]
struct JsonFiles<'a> {
    old: &'a str,
    new: &'a str,
}

#[derive(Debug, Clone, Serialize)]
struct JsonDialectContext {
    old: Option<JsonDialect>,
    new: Option<JsonDialect>,
}

#[derive(Debug, Clone, Serialize)]
struct JsonDialect {
    delimiter: String,
    quote: String,
    escape: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct JsonChecks {
    schema_overlap: JsonSchemaOverlap,
    key_viability: Option<JsonKeyViability>,
    row_granularity: JsonRowGranularity,
    type_consistency: JsonTypeConsistency,
}

#[derive(Debug, Clone, Serialize)]
struct JsonSchemaOverlap {
    status: &'static str,
    columns_common: u64,
    columns_old_only: Vec<String>,
    columns_new_only: Vec<String>,
    overlap_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
struct JsonKeyViability {
    status: &'static str,
    key_column: String,
    #[serde(skip_serializing_if = "is_single_key")]
    key_columns: Vec<String>,
    found_old: bool,
    found_new: bool,
    unique_old: Option<bool>,
    unique_new: Option<bool>,
    coverage: Option<f64>,
}

fn is_single_key(columns: &[String]) -> bool {
    columns.len() <= 1
}

#[derive(Debug, Clone, Serialize)]
struct JsonRowGranularity {
    status: &'static str,
    rows_old: u64,
    rows_new: u64,
    key_overlap: Option<u64>,
    keys_old_only: Option<u64>,
    keys_new_only: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct JsonTypeConsistency {
    status: &'static str,
    numeric_columns: u64,
    type_shifts: Vec<JsonTypeShift>,
}

#[derive(Debug, Clone, Serialize)]
struct JsonTypeShift {
    column: String,
    old_type: &'static str,
    new_type: &'static str,
}

pub fn render_shape_json(context: JsonRenderContext<'_>) -> Result<String, serde_json::Error> {
    let explicit = context.explicit;
    let checks = context.checks.map(|suite| json_checks(suite, explicit));
    let reasons = match context.outcome {
        Outcome::Refusal => None,
        _ => Some(context.reasons.unwrap_or(&[]).to_vec()),
    };

    let refusal = if context.outcome == Outcome::Refusal {
        context.refusal
    } else {
        None
    };

    let payload = JsonOutcome {
        version: "shape.v0",
        outcome: outcome_str(context.outcome),
        profile_id: context.profile_id,
        profile_sha256: context.profile_sha256,
        input_verification: context.input_verification,
        files: JsonFiles {
            old: context.old_file,
            new: context.new_file,
        },
        dialect: JsonDialectContext {
            old: context.dialect_old.map(json_dialect),
            new: context.dialect_new.map(json_dialect),
        },
        checks,
        reasons,
        refusal,
    };
    let mut rendered = serde_json::to_string(&payload)?;
    rendered.push('\n');
    Ok(rendered)
}

pub fn render_outcome(
    outcome: Outcome,
    refusal: Option<&RefusalPayload>,
) -> Result<String, serde_json::Error> {
    render_shape_json(JsonRenderContext::minimal(outcome, refusal))
}

fn outcome_str(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Compatible => "COMPATIBLE",
        Outcome::Incompatible => "INCOMPATIBLE",
        Outcome::Refusal => "REFUSAL",
    }
}

fn check_status(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => "pass",
        CheckStatus::Fail => "fail",
    }
}

fn column_type_label(classification: ColumnClassification) -> &'static str {
    match classification {
        ColumnClassification::Numeric => "numeric",
        ColumnClassification::NonNumeric => "non-numeric",
        ColumnClassification::AllMissing => "all-missing",
    }
}

fn json_checks(suite: &CheckSuite, explicit: bool) -> JsonChecks {
    JsonChecks {
        schema_overlap: JsonSchemaOverlap {
            status: check_status(suite.schema_overlap.status),
            columns_common: suite.schema_overlap.columns_common.len() as u64,
            columns_old_only: if explicit {
                suite
                    .schema_overlap
                    .columns_old_only
                    .iter()
                    .map(|column| encode_identifier(column))
                    .collect()
            } else {
                vec![]
            },
            columns_new_only: if explicit {
                suite
                    .schema_overlap
                    .columns_new_only
                    .iter()
                    .map(|column| encode_identifier(column))
                    .collect()
            } else {
                vec![]
            },
            overlap_ratio: suite.schema_overlap.overlap_ratio,
        },
        key_viability: suite.key_viability.as_ref().map(|key| {
            let key_columns: Vec<String> = key
                .key_columns
                .iter()
                .map(|col| encode_identifier(col))
                .collect();
            let key_column = if key_columns.len() > 1 {
                key_columns.join(" + ")
            } else {
                encode_identifier(&key.key_column)
            };
            JsonKeyViability {
                status: check_status(key.status),
                key_column,
                key_columns,
                found_old: key.found_old,
                found_new: key.found_new,
                unique_old: key.unique_old,
                unique_new: key.unique_new,
                coverage: key.coverage,
            }
        }),
        row_granularity: JsonRowGranularity {
            status: check_status(suite.row_granularity.status),
            rows_old: suite.row_granularity.rows_old,
            rows_new: suite.row_granularity.rows_new,
            key_overlap: suite.row_granularity.key_overlap,
            keys_old_only: suite.row_granularity.keys_old_only,
            keys_new_only: suite.row_granularity.keys_new_only,
        },
        type_consistency: JsonTypeConsistency {
            status: check_status(suite.type_consistency.status),
            numeric_columns: suite.type_consistency.numeric_columns,
            type_shifts: suite
                .type_consistency
                .type_shifts
                .iter()
                .map(|shift| JsonTypeShift {
                    column: if explicit {
                        encode_identifier(&shift.column)
                    } else {
                        "[REDACTED]".to_string()
                    },
                    old_type: column_type_label(shift.old_type),
                    new_type: column_type_label(shift.new_type),
                })
                .collect(),
        },
    }
}

fn json_dialect(dialect: Dialect) -> JsonDialect {
    JsonDialect {
        delimiter: display_byte(dialect.delimiter),
        quote: display_byte(dialect.quote),
        escape: match dialect.escape {
            EscapeMode::None => "none",
            EscapeMode::Backslash => "backslash",
        },
    }
}

fn display_byte(byte: u8) -> String {
    if byte == b'\t' {
        return "\\t".to_owned();
    }
    if byte.is_ascii_graphic() || byte == b' ' {
        return (byte as char).to_string();
    }
    format!("0x{byte:02x}")
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{JsonRenderContext, render_shape_json};
    use crate::checks::key_viability::KeyViabilityResult;
    use crate::checks::row_granularity::RowGranularityResult;
    use crate::checks::schema_overlap::SchemaOverlapResult;
    use crate::checks::suite::{CheckStatus, CheckSuite, Outcome};
    use crate::checks::type_consistency::{TypeConsistencyResult, TypeShift};
    use crate::csv::dialect::{Dialect, EscapeMode};
    use crate::refusal::payload::RefusalPayload;
    use crate::scan::ColumnClassification;

    #[test]
    fn compatible_with_key_emits_full_top_level_contract() {
        let checks = sample_suite(Some(KeyViabilityResult {
            status: CheckStatus::Pass,
            key_column: b"loan_id".to_vec(),
            key_columns: vec![b"loan_id".to_vec()],
            found_old: true,
            found_new: true,
            found_components_old: vec![true],
            found_components_new: vec![true],
            unique_old: Some(true),
            unique_new: Some(true),
            duplicate_values_old: Some(0),
            duplicate_values_new: Some(0),
            empty_values_old: Some(0),
            empty_values_new: Some(0),
            coverage: Some(1.0),
        }));
        let reasons: Vec<String> = vec![];
        let rendered = render_shape_json(JsonRenderContext {
            outcome: Outcome::Compatible,
            old_file: "old.csv",
            new_file: "new.csv",
            dialect_old: Some(Dialect::default()),
            dialect_new: Some(Dialect::default()),
            checks: Some(&checks),
            reasons: Some(&reasons),
            refusal: None,
            profile_id: None,
            profile_sha256: None,
            input_verification: None,
            explicit: true,
        })
        .expect("render json");

        let value: serde_json::Value = serde_json::from_str(&rendered).expect("parse json");
        assert_eq!(value["version"], "shape.v0");
        assert_eq!(value["outcome"], "COMPATIBLE");
        assert_eq!(value["files"]["old"], "old.csv");
        assert_eq!(value["files"]["new"], "new.csv");
        assert_eq!(value["checks"]["key_viability"]["key_column"], "u8:loan_id");
        assert!(
            value["reasons"]
                .as_array()
                .expect("reasons array")
                .is_empty()
        );
        assert!(value["refusal"].is_null());
    }

    #[test]
    fn compatible_without_key_renders_nullable_key_fields_as_null() {
        let checks = sample_suite(None);
        let reasons: Vec<String> = vec![];
        let rendered = render_shape_json(JsonRenderContext {
            outcome: Outcome::Compatible,
            old_file: "old.csv",
            new_file: "new.csv",
            dialect_old: Some(Dialect::default()),
            dialect_new: Some(Dialect::default()),
            checks: Some(&checks),
            reasons: Some(&reasons),
            refusal: None,
            profile_id: None,
            profile_sha256: None,
            input_verification: None,
            explicit: true,
        })
        .expect("render json");

        let value: serde_json::Value = serde_json::from_str(&rendered).expect("parse json");
        assert!(value["checks"]["key_viability"].is_null());
        assert!(value["checks"]["row_granularity"]["key_overlap"].is_null());
        assert!(value["checks"]["row_granularity"]["keys_old_only"].is_null());
        assert!(value["checks"]["row_granularity"]["keys_new_only"].is_null());
    }

    #[test]
    fn incompatible_renders_non_null_reasons_and_null_refusal() {
        let mut checks = sample_suite(None);
        checks.type_consistency = TypeConsistencyResult {
            status: CheckStatus::Fail,
            numeric_columns: 0,
            type_shifts: vec![TypeShift {
                column: b"balance".to_vec(),
                old_type: ColumnClassification::Numeric,
                new_type: ColumnClassification::NonNumeric,
            }],
        };
        let reasons = vec!["Type shift: balance changed from numeric to non-numeric".to_string()];
        let rendered = render_shape_json(JsonRenderContext {
            outcome: Outcome::Incompatible,
            old_file: "old.csv",
            new_file: "new.csv",
            dialect_old: Some(Dialect::default()),
            dialect_new: Some(Dialect::default()),
            checks: Some(&checks),
            reasons: Some(&reasons),
            refusal: None,
            profile_id: None,
            profile_sha256: None,
            input_verification: None,
            explicit: true,
        })
        .expect("render json");

        let value: serde_json::Value = serde_json::from_str(&rendered).expect("parse json");
        assert_eq!(value["outcome"], "INCOMPATIBLE");
        assert_eq!(value["checks"]["type_consistency"]["status"], "fail");
        assert_eq!(value["reasons"][0], reasons[0]);
        assert!(value["refusal"].is_null());
    }

    #[test]
    fn refusal_renders_partial_dialect_context_and_null_checks_reasons() {
        let refusal = RefusalPayload::empty("new.csv", 0);
        let rendered = render_shape_json(JsonRenderContext {
            outcome: Outcome::Refusal,
            old_file: "old.csv",
            new_file: "new.csv",
            dialect_old: Some(Dialect::default()),
            dialect_new: None,
            checks: None,
            reasons: None,
            refusal: Some(&refusal),
            profile_id: None,
            profile_sha256: None,
            input_verification: None,
            explicit: true,
        })
        .expect("render json");

        let value: serde_json::Value = serde_json::from_str(&rendered).expect("parse json");
        assert_eq!(value["outcome"], "REFUSAL");
        assert!(value["checks"].is_null());
        assert!(value["reasons"].is_null());
        assert_eq!(value["refusal"]["code"], "E_EMPTY");
        assert!(!value["dialect"]["old"].is_null());
        assert!(value["dialect"]["new"].is_null());
    }

    #[test]
    fn renders_f64_values_without_forced_rounding() {
        let checks = CheckSuite {
            schema_overlap: SchemaOverlapResult {
                status: CheckStatus::Pass,
                columns_common: vec![b"loan_id".to_vec(), b"amount".to_vec()],
                columns_old_only: vec![],
                columns_new_only: vec![b"new_col".to_vec()],
                overlap_ratio: 0.8823529411764706,
            },
            key_viability: None,
            row_granularity: RowGranularityResult {
                status: CheckStatus::Pass,
                rows_old: 1,
                rows_new: 1,
                key_overlap: None,
                keys_old_only: None,
                keys_new_only: None,
            },
            type_consistency: TypeConsistencyResult {
                status: CheckStatus::Pass,
                numeric_columns: 1,
                type_shifts: vec![],
            },
        };
        let rendered = render_shape_json(JsonRenderContext {
            outcome: Outcome::Compatible,
            old_file: "old.csv",
            new_file: "new.csv",
            dialect_old: Some(Dialect {
                delimiter: b',',
                quote: b'"',
                escape: EscapeMode::None,
            }),
            dialect_new: Some(Dialect::default()),
            checks: Some(&checks),
            reasons: Some(&[]),
            refusal: None,
            profile_id: None,
            profile_sha256: None,
            input_verification: None,
            explicit: true,
        })
        .expect("render json");

        assert!(
            rendered.contains("0.8823529411764706"),
            "expected full-precision overlap ratio in JSON: {rendered}"
        );
    }

    fn sample_suite(key_viability: Option<KeyViabilityResult>) -> CheckSuite {
        let (key_overlap, keys_old_only, keys_new_only) = if key_viability.is_some() {
            (Some(3), Some(0), Some(0))
        } else {
            (None, None, None)
        };

        CheckSuite {
            schema_overlap: SchemaOverlapResult {
                status: CheckStatus::Pass,
                columns_common: vec![b"loan_id".to_vec(), b"amount".to_vec()],
                columns_old_only: vec![],
                columns_new_only: vec![],
                overlap_ratio: 1.0,
            },
            key_viability,
            row_granularity: RowGranularityResult {
                status: CheckStatus::Pass,
                rows_old: 3,
                rows_new: 3,
                key_overlap,
                keys_old_only,
                keys_new_only,
            },
            type_consistency: TypeConsistencyResult {
                status: CheckStatus::Pass,
                numeric_columns: 1,
                type_shifts: vec![],
            },
        }
    }

    #[allow(dead_code)]
    fn _sample_key_scan() -> HashSet<Vec<u8>> {
        HashSet::from([b"L1".to_vec(), b"L2".to_vec()])
    }
}
