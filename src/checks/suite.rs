use crate::checks::key_viability::KeyViabilityResult;
use crate::checks::key_viability::evaluate_key_viability;
use crate::checks::row_granularity::RowGranularityResult;
use crate::checks::row_granularity::{compute_key_overlap_metrics, evaluate_row_granularity};
use crate::checks::schema_overlap::SchemaOverlapResult;
use crate::checks::schema_overlap::evaluate_schema_overlap;
use crate::checks::type_consistency::{TypeConsistencyResult, evaluate_type_consistency};
use crate::scan::ColumnClassification;
use crate::scan::ScanResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Compatible,
    Incompatible,
    Refusal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CheckSuite {
    pub schema_overlap: SchemaOverlapResult,
    pub key_viability: Option<KeyViabilityResult>,
    pub row_granularity: RowGranularityResult,
    pub type_consistency: TypeConsistencyResult,
}

impl CheckSuite {
    pub fn determine_outcome(&self) -> Outcome {
        let failed = self.schema_overlap.status == CheckStatus::Fail
            || self
                .key_viability
                .as_ref()
                .is_some_and(|k| k.status == CheckStatus::Fail)
            || self.type_consistency.status == CheckStatus::Fail;

        if failed {
            Outcome::Incompatible
        } else {
            Outcome::Compatible
        }
    }
}

pub fn determine_outcome(suite: &CheckSuite) -> Outcome {
    suite.determine_outcome()
}

pub fn assemble_check_suite(
    old_headers: &[Vec<u8>],
    new_headers: &[Vec<u8>],
    key_column: Option<Vec<u8>>,
    key_found_old: bool,
    key_found_new: bool,
    old_scan: &ScanResult,
    new_scan: &ScanResult,
) -> CheckSuite {
    let schema_overlap = evaluate_schema_overlap(old_headers, new_headers);
    let key_viability = key_column.map(|key| {
        evaluate_key_viability(
            key,
            key_found_old,
            key_found_new,
            old_scan.key_scan.as_ref(),
            new_scan.key_scan.as_ref(),
        )
    });

    let key_metrics = match (old_scan.key_scan.as_ref(), new_scan.key_scan.as_ref()) {
        (Some(old), Some(new)) => Some(compute_key_overlap_metrics(old, new)),
        _ => None,
    };

    let row_granularity =
        evaluate_row_granularity(old_scan.row_count, new_scan.row_count, key_metrics);
    let type_consistency = evaluate_type_consistency(
        &schema_overlap.columns_common,
        &old_scan.column_types,
        &new_scan.column_types,
    );

    CheckSuite {
        schema_overlap,
        key_viability,
        row_granularity,
        type_consistency,
    }
}

pub fn build_reasons(suite: &CheckSuite) -> Vec<String> {
    let mut reasons = Vec::new();

    if suite.schema_overlap.status == CheckStatus::Fail {
        let old_count =
            suite.schema_overlap.columns_common.len() + suite.schema_overlap.columns_old_only.len();
        let new_count =
            suite.schema_overlap.columns_common.len() + suite.schema_overlap.columns_new_only.len();
        reasons.push(format!(
            "Schema overlap: {} common columns (old={}, new={})",
            suite.schema_overlap.columns_common.len(),
            old_count,
            new_count
        ));
    }

    if let Some(key) = suite.key_viability.as_ref()
        && key.status == CheckStatus::Fail
    {
        let key_name = String::from_utf8_lossy(&key.key_column);
        if !key.found_old {
            reasons.push(format!("Key viability: {} not found in old file", key_name));
        }
        if !key.found_new {
            reasons.push(format!("Key viability: {} not found in new file", key_name));
        }
        if key.found_old && key.found_new {
            push_key_viability_count_reasons(
                &mut reasons,
                key_name.as_ref(),
                "old",
                key.duplicate_values_old,
                key.empty_values_old,
            );
            push_key_viability_count_reasons(
                &mut reasons,
                key_name.as_ref(),
                "new",
                key.duplicate_values_new,
                key.empty_values_new,
            );
        }
    }

    if suite.type_consistency.status == CheckStatus::Fail {
        for shift in &suite.type_consistency.type_shifts {
            let column = String::from_utf8_lossy(&shift.column);
            reasons.push(format!(
                "Type shift: {} changed from {} to {}",
                column,
                classification_label(shift.old_type),
                classification_label(shift.new_type)
            ));
        }
    }

    reasons
}

fn push_key_viability_count_reasons(
    reasons: &mut Vec<String>,
    key_name: &str,
    file_label: &str,
    duplicate_values: Option<u64>,
    empty_values: Option<u64>,
) {
    if let Some(duplicates) = duplicate_values.filter(|count| *count > 0) {
        let noun = if duplicates == 1 {
            "duplicate value"
        } else {
            "duplicate values"
        };
        reasons.push(format!(
            "Key viability: {key_name} has {duplicates} {noun} in {file_label} file"
        ));
    }
    if let Some(empty) = empty_values.filter(|count| *count > 0) {
        let noun = if empty == 1 {
            "empty value"
        } else {
            "empty values"
        };
        reasons.push(format!(
            "Key viability: {key_name} has {empty} {noun} in {file_label} file"
        ));
    }
}

fn classification_label(classification: ColumnClassification) -> &'static str {
    match classification {
        ColumnClassification::Numeric => "numeric",
        ColumnClassification::NonNumeric => "non-numeric",
        ColumnClassification::AllMissing => "all-missing",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::assemble_check_suite;
    use super::{CheckStatus, CheckSuite, Outcome, build_reasons, determine_outcome};
    use crate::checks::key_viability::KeyViabilityResult;
    use crate::checks::row_granularity::RowGranularityResult;
    use crate::checks::schema_overlap::SchemaOverlapResult;
    use crate::checks::type_consistency::{TypeConsistencyResult, TypeShift};
    use crate::scan::{ColumnClassification, KeyScan, ScanResult};

    #[test]
    fn determine_outcome_ignores_row_granularity_status() {
        let suite = CheckSuite {
            schema_overlap: schema_pass(),
            key_viability: None,
            row_granularity: RowGranularityResult {
                status: CheckStatus::Fail,
                rows_old: 0,
                rows_new: 0,
                key_overlap: None,
                keys_old_only: None,
                keys_new_only: None,
            },
            type_consistency: type_pass(),
        };

        assert_eq!(determine_outcome(&suite), Outcome::Compatible);
    }

    #[test]
    fn build_reasons_is_empty_for_compatible_suite() {
        let suite = CheckSuite {
            schema_overlap: schema_pass(),
            key_viability: Some(key_pass()),
            row_granularity: row_pass(),
            type_consistency: type_pass(),
        };

        assert!(build_reasons(&suite).is_empty());
    }

    #[test]
    fn build_reasons_includes_schema_overlap_template() {
        let suite = CheckSuite {
            schema_overlap: SchemaOverlapResult {
                status: CheckStatus::Fail,
                columns_common: vec![],
                columns_old_only: vec![b"loan_id".to_vec(), b"amount".to_vec()],
                columns_new_only: vec![b"loan_id_v2".to_vec()],
                overlap_ratio: 0.0,
            },
            key_viability: None,
            row_granularity: row_pass(),
            type_consistency: type_pass(),
        };

        assert_eq!(
            build_reasons(&suite),
            vec!["Schema overlap: 0 common columns (old=2, new=1)".to_string()]
        );
    }

    #[test]
    fn build_reasons_includes_key_missing_template() {
        let suite = CheckSuite {
            schema_overlap: schema_pass(),
            key_viability: Some(KeyViabilityResult {
                status: CheckStatus::Fail,
                key_column: b"loan_id".to_vec(),
                found_old: true,
                found_new: false,
                unique_old: Some(true),
                unique_new: None,
                duplicate_values_old: Some(0),
                duplicate_values_new: None,
                empty_values_old: Some(0),
                empty_values_new: None,
                coverage: None,
            }),
            row_granularity: row_pass(),
            type_consistency: type_pass(),
        };

        assert_eq!(
            build_reasons(&suite),
            vec!["Key viability: loan_id not found in new file".to_string()]
        );
    }

    #[test]
    fn build_reasons_includes_key_non_viable_template_for_old_file() {
        let suite = CheckSuite {
            schema_overlap: schema_pass(),
            key_viability: Some(KeyViabilityResult {
                status: CheckStatus::Fail,
                key_column: b"loan_id".to_vec(),
                found_old: true,
                found_new: true,
                unique_old: Some(false),
                unique_new: Some(true),
                duplicate_values_old: Some(2),
                duplicate_values_new: Some(0),
                empty_values_old: Some(1),
                empty_values_new: Some(0),
                coverage: Some(0.5),
            }),
            row_granularity: row_pass(),
            type_consistency: type_pass(),
        };

        assert_eq!(
            build_reasons(&suite),
            vec![
                "Key viability: loan_id has 2 duplicate values in old file".to_string(),
                "Key viability: loan_id has 1 empty value in old file".to_string()
            ]
        );
    }

    #[test]
    fn build_reasons_includes_key_non_viable_template_for_both_files() {
        let suite = CheckSuite {
            schema_overlap: schema_pass(),
            key_viability: Some(KeyViabilityResult {
                status: CheckStatus::Fail,
                key_column: b"loan_id".to_vec(),
                found_old: true,
                found_new: true,
                unique_old: Some(false),
                unique_new: Some(false),
                duplicate_values_old: Some(2),
                duplicate_values_new: Some(3),
                empty_values_old: Some(1),
                empty_values_new: Some(4),
                coverage: Some(0.0),
            }),
            row_granularity: row_pass(),
            type_consistency: type_pass(),
        };

        assert_eq!(
            build_reasons(&suite),
            vec![
                "Key viability: loan_id has 2 duplicate values in old file".to_string(),
                "Key viability: loan_id has 1 empty value in old file".to_string(),
                "Key viability: loan_id has 3 duplicate values in new file".to_string(),
                "Key viability: loan_id has 4 empty values in new file".to_string()
            ]
        );
    }

    #[test]
    fn build_reasons_includes_type_shift_template() {
        let suite = CheckSuite {
            schema_overlap: schema_pass(),
            key_viability: None,
            row_granularity: row_pass(),
            type_consistency: TypeConsistencyResult {
                status: CheckStatus::Fail,
                numeric_columns: 0,
                type_shifts: vec![TypeShift {
                    column: b"balance".to_vec(),
                    old_type: ColumnClassification::Numeric,
                    new_type: ColumnClassification::NonNumeric,
                }],
            },
        };

        assert_eq!(
            build_reasons(&suite),
            vec!["Type shift: balance changed from numeric to non-numeric".to_string()]
        );
    }

    #[test]
    fn build_reasons_has_deterministic_multi_failure_order() {
        let suite = CheckSuite {
            schema_overlap: SchemaOverlapResult {
                status: CheckStatus::Fail,
                columns_common: vec![],
                columns_old_only: vec![b"a".to_vec()],
                columns_new_only: vec![b"b".to_vec()],
                overlap_ratio: 0.0,
            },
            key_viability: Some(KeyViabilityResult {
                status: CheckStatus::Fail,
                key_column: b"loan_id".to_vec(),
                found_old: false,
                found_new: true,
                unique_old: None,
                unique_new: Some(true),
                duplicate_values_old: None,
                duplicate_values_new: Some(0),
                empty_values_old: None,
                empty_values_new: Some(0),
                coverage: None,
            }),
            row_granularity: row_pass(),
            type_consistency: TypeConsistencyResult {
                status: CheckStatus::Fail,
                numeric_columns: 0,
                type_shifts: vec![TypeShift {
                    column: b"balance".to_vec(),
                    old_type: ColumnClassification::NonNumeric,
                    new_type: ColumnClassification::Numeric,
                }],
            },
        };

        assert_eq!(
            build_reasons(&suite),
            vec![
                "Schema overlap: 0 common columns (old=1, new=1)".to_string(),
                "Key viability: loan_id not found in old file".to_string(),
                "Type shift: balance changed from non-numeric to numeric".to_string()
            ]
        );
    }

    #[test]
    fn assemble_suite_with_key_populates_all_check_blocks() {
        let old_headers = vec![b"loan_id".to_vec(), b"amount".to_vec()];
        let new_headers = vec![b"loan_id".to_vec(), b"amount".to_vec()];

        let old_scan = ScanResult {
            row_count: 3,
            key_scan: Some(KeyScan {
                values: HashSet::from([b"L1".to_vec(), b"L2".to_vec(), b"L3".to_vec()]),
                duplicate_count: 0,
                empty_count: 0,
            }),
            column_types: vec![
                ColumnClassification::NonNumeric,
                ColumnClassification::Numeric,
            ],
        };
        let new_scan = ScanResult {
            row_count: 2,
            key_scan: Some(KeyScan {
                values: HashSet::from([b"L2".to_vec(), b"L3".to_vec()]),
                duplicate_count: 0,
                empty_count: 0,
            }),
            column_types: vec![
                ColumnClassification::NonNumeric,
                ColumnClassification::Numeric,
            ],
        };

        let suite = assemble_check_suite(
            &old_headers,
            &new_headers,
            Some(b"loan_id".to_vec()),
            true,
            true,
            &old_scan,
            &new_scan,
        );

        assert_eq!(suite.schema_overlap.status, CheckStatus::Pass);
        assert!(suite.key_viability.is_some());
        assert_eq!(suite.row_granularity.key_overlap, Some(2));
        assert_eq!(suite.row_granularity.keys_old_only, Some(1));
        assert_eq!(suite.row_granularity.keys_new_only, Some(0));
        assert_eq!(suite.type_consistency.status, CheckStatus::Pass);
    }

    #[test]
    fn assemble_suite_without_key_sets_nullable_key_outputs_to_none() {
        let old_headers = vec![b"amount".to_vec()];
        let new_headers = vec![b"amount".to_vec()];
        let old_scan = ScanResult {
            row_count: 1,
            key_scan: None,
            column_types: vec![ColumnClassification::Numeric],
        };
        let new_scan = ScanResult {
            row_count: 1,
            key_scan: None,
            column_types: vec![ColumnClassification::Numeric],
        };

        let suite = assemble_check_suite(
            &old_headers,
            &new_headers,
            None,
            false,
            false,
            &old_scan,
            &new_scan,
        );

        assert!(suite.key_viability.is_none());
        assert_eq!(suite.row_granularity.key_overlap, None);
        assert_eq!(suite.row_granularity.keys_old_only, None);
        assert_eq!(suite.row_granularity.keys_new_only, None);
    }

    #[test]
    fn assemble_suite_with_missing_key_keeps_key_viability_some_but_row_key_metrics_none() {
        let old_headers = vec![b"loan_id".to_vec(), b"amount".to_vec()];
        let new_headers = vec![b"amount".to_vec()];
        let old_scan = ScanResult {
            row_count: 2,
            key_scan: Some(KeyScan {
                values: HashSet::from([b"L1".to_vec(), b"L2".to_vec()]),
                duplicate_count: 0,
                empty_count: 0,
            }),
            column_types: vec![ColumnClassification::Numeric],
        };
        let new_scan = ScanResult {
            row_count: 2,
            key_scan: None,
            column_types: vec![ColumnClassification::Numeric],
        };

        let suite = assemble_check_suite(
            &old_headers,
            &new_headers,
            Some(b"loan_id".to_vec()),
            true,
            false,
            &old_scan,
            &new_scan,
        );

        assert!(suite.key_viability.is_some());
        assert_eq!(
            suite.key_viability.as_ref().map(|k| k.found_new),
            Some(false)
        );
        assert_eq!(suite.row_granularity.key_overlap, None);
        assert_eq!(suite.row_granularity.keys_old_only, None);
        assert_eq!(suite.row_granularity.keys_new_only, None);
    }

    fn schema_pass() -> SchemaOverlapResult {
        SchemaOverlapResult {
            status: CheckStatus::Pass,
            columns_common: vec![b"loan_id".to_vec()],
            columns_old_only: vec![],
            columns_new_only: vec![],
            overlap_ratio: 1.0,
        }
    }

    fn key_pass() -> KeyViabilityResult {
        KeyViabilityResult {
            status: CheckStatus::Pass,
            key_column: b"loan_id".to_vec(),
            found_old: true,
            found_new: true,
            unique_old: Some(true),
            unique_new: Some(true),
            duplicate_values_old: Some(0),
            duplicate_values_new: Some(0),
            empty_values_old: Some(0),
            empty_values_new: Some(0),
            coverage: Some(1.0),
        }
    }

    fn row_pass() -> RowGranularityResult {
        RowGranularityResult {
            status: CheckStatus::Pass,
            rows_old: 1,
            rows_new: 1,
            key_overlap: Some(1),
            keys_old_only: Some(0),
            keys_new_only: Some(0),
        }
    }

    fn type_pass() -> TypeConsistencyResult {
        TypeConsistencyResult {
            status: CheckStatus::Pass,
            numeric_columns: 1,
            type_shifts: vec![],
        }
    }
}
