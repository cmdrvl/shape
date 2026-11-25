use crate::checks::suite::CheckStatus;
use crate::scan::KeyScan;

#[derive(Debug, Clone, PartialEq)]
pub struct KeyViabilityResult {
    pub status: CheckStatus,
    /// Single key column label (first key column for composite keys).
    pub key_column: Vec<u8>,
    /// All key columns. For single keys this has one element.
    pub key_columns: Vec<Vec<u8>>,
    pub found_old: bool,
    pub found_new: bool,
    /// Per-component found status for composite keys.
    pub found_components_old: Vec<bool>,
    pub found_components_new: Vec<bool>,
    pub unique_old: Option<bool>,
    pub unique_new: Option<bool>,
    pub duplicate_values_old: Option<u64>,
    pub duplicate_values_new: Option<u64>,
    pub empty_values_old: Option<u64>,
    pub empty_values_new: Option<u64>,
    pub coverage: Option<f64>,
}

pub fn evaluate_key_viability(
    key_columns: Vec<Vec<u8>>,
    found_components_old: Vec<bool>,
    found_components_new: Vec<bool>,
    old_scan: Option<&KeyScan>,
    new_scan: Option<&KeyScan>,
) -> KeyViabilityResult {
    evaluate_key_viability_with_min_coverage(
        key_columns,
        found_components_old,
        found_components_new,
        old_scan,
        new_scan,
        None,
    )
}

/// Evaluate key viability with an optional minimum overlap ratio.
///
/// Two non-empty key sets always require at least one overlapping key. An
/// explicit threshold adds the normal inclusive `coverage >= minimum`
/// comparison; `--min-coverage 0` is therefore a valid no-op floor, not a way
/// to make a zero-overlap pair compatible.
pub fn evaluate_key_viability_with_min_coverage(
    key_columns: Vec<Vec<u8>>,
    found_components_old: Vec<bool>,
    found_components_new: Vec<bool>,
    old_scan: Option<&KeyScan>,
    new_scan: Option<&KeyScan>,
    min_coverage: Option<f64>,
) -> KeyViabilityResult {
    let found_old = found_components_old.iter().all(|&f| f);
    let found_new = found_components_new.iter().all(|&f| f);
    let old_scan = old_scan.filter(|_| found_old);
    let new_scan = new_scan.filter(|_| found_new);

    let unique_old = old_scan.map(is_unique_and_non_empty);
    let unique_new = new_scan.map(is_unique_and_non_empty);
    let duplicate_values_old = old_scan.map(|scan| scan.duplicate_count);
    let duplicate_values_new = new_scan.map(|scan| scan.duplicate_count);
    let empty_values_old = old_scan.map(|scan| scan.empty_count);
    let empty_values_new = new_scan.map(|scan| scan.empty_count);
    let coverage = match (old_scan, new_scan) {
        (Some(old), Some(new)) => Some(calculate_coverage(old, new)),
        _ => None,
    };

    let coverage_is_viable = match (old_scan, new_scan, coverage) {
        (Some(old), Some(new), Some(coverage)) if has_rows(old) && has_rows(new) => {
            coverage > 0.0 && coverage >= min_coverage.unwrap_or(0.0)
        }
        // An empty table is rejected by the pipeline's E_EMPTY guard. Keep
        // direct check evaluation deterministic for the empty-table edge case
        // rather than treating its zero denominator as a domain mismatch.
        _ => true,
    };

    let status = if found_old
        && found_new
        && unique_old == Some(true)
        && unique_new == Some(true)
        && coverage_is_viable
    {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };

    let key_column = key_columns.first().cloned().unwrap_or_default();
    KeyViabilityResult {
        status,
        key_column,
        key_columns,
        found_old,
        found_new,
        found_components_old,
        found_components_new,
        unique_old,
        unique_new,
        duplicate_values_old,
        duplicate_values_new,
        empty_values_old,
        empty_values_new,
        coverage,
    }
}

fn has_rows(scan: &KeyScan) -> bool {
    !scan.values.is_empty() || scan.duplicate_count > 0 || scan.empty_count > 0
}

fn is_unique_and_non_empty(scan: &KeyScan) -> bool {
    scan.duplicate_count == 0 && scan.empty_count == 0
}

fn calculate_coverage(old: &KeyScan, new: &KeyScan) -> f64 {
    let overlap = old.values.intersection(&new.values).count() as u64;
    let max_keys = old.values.len().max(new.values.len()) as u64;
    if max_keys == 0 {
        0.0
    } else {
        overlap as f64 / max_keys as f64
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{evaluate_key_viability, evaluate_key_viability_with_min_coverage};
    use crate::checks::suite::CheckStatus;
    use crate::scan::KeyScan;

    fn key_scan(values: &[&[u8]], duplicate_count: u64, empty_count: u64) -> KeyScan {
        KeyScan {
            values: values
                .iter()
                .map(|value| vec![value.to_vec()])
                .collect::<HashSet<_>>(),
            duplicate_count,
            empty_count,
        }
    }

    #[test]
    fn passes_when_found_unique_and_non_empty_in_both_files() {
        let old = key_scan(&[b"A1", b"A2", b"A3"], 0, 0);
        let new = key_scan(&[b"A1", b"A2", b"A4"], 0, 0);

        let result = evaluate_key_viability(
            vec![b"loan_id".to_vec()],
            vec![true],
            vec![true],
            Some(&old),
            Some(&new),
        );

        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(result.key_column, b"loan_id".to_vec());
        assert!(result.found_old);
        assert!(result.found_new);
        assert_eq!(result.unique_old, Some(true));
        assert_eq!(result.unique_new, Some(true));
        assert_eq!(result.duplicate_values_old, Some(0));
        assert_eq!(result.duplicate_values_new, Some(0));
        assert_eq!(result.empty_values_old, Some(0));
        assert_eq!(result.empty_values_new, Some(0));
        assert_eq!(result.coverage, Some(2.0 / 3.0));
    }

    #[test]
    fn fails_when_key_missing_in_new_file() {
        let old = key_scan(&[b"A1", b"A2"], 0, 0);

        let result = evaluate_key_viability(
            vec![b"loan_id".to_vec()],
            vec![true],
            vec![false],
            Some(&old),
            None,
        );

        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.found_old);
        assert!(!result.found_new);
        assert_eq!(result.unique_old, Some(true));
        assert_eq!(result.unique_new, None);
        assert_eq!(result.duplicate_values_old, Some(0));
        assert_eq!(result.duplicate_values_new, None);
        assert_eq!(result.empty_values_old, Some(0));
        assert_eq!(result.empty_values_new, None);
        assert_eq!(result.coverage, None);
    }

    #[test]
    fn fails_when_duplicates_or_empty_values_exist() {
        let old = key_scan(&[b"A1", b"A2"], 1, 0);
        let new = key_scan(&[b"A1", b"A2"], 0, 2);

        let result = evaluate_key_viability(
            vec![b"loan_id".to_vec()],
            vec![true],
            vec![true],
            Some(&old),
            Some(&new),
        );

        assert_eq!(result.status, CheckStatus::Fail);
        assert_eq!(result.unique_old, Some(false));
        assert_eq!(result.unique_new, Some(false));
        assert_eq!(result.duplicate_values_old, Some(1));
        assert_eq!(result.duplicate_values_new, Some(0));
        assert_eq!(result.empty_values_old, Some(0));
        assert_eq!(result.empty_values_new, Some(2));
        assert_eq!(result.coverage, Some(1.0));
    }

    #[test]
    fn coverage_is_zero_when_both_key_sets_are_empty() {
        let old = key_scan(&[], 0, 0);
        let new = key_scan(&[], 0, 0);

        let result = evaluate_key_viability(
            vec![b"loan_id".to_vec()],
            vec![true],
            vec![true],
            Some(&old),
            Some(&new),
        );

        assert_eq!(result.coverage, Some(0.0));
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn fails_when_non_empty_key_sets_have_no_overlap() {
        let old = key_scan(&[b"OLD-1", b"OLD-2"], 0, 0);
        let new = key_scan(&[b"NEW-1", b"NEW-2"], 0, 0);

        let result = evaluate_key_viability(
            vec![b"loan_id".to_vec()],
            vec![true],
            vec![true],
            Some(&old),
            Some(&new),
        );

        assert_eq!(result.status, CheckStatus::Fail);
        assert_eq!(result.coverage, Some(0.0));
        assert_eq!(result.unique_old, Some(true));
        assert_eq!(result.unique_new, Some(true));
    }

    #[test]
    fn explicit_minimum_coverage_is_inclusive_and_zero_stays_incompatible() {
        let old = key_scan(&[b"A1", b"A2"], 0, 0);
        let new = key_scan(&[b"A1", b"A3"], 0, 0);

        let below = evaluate_key_viability_with_min_coverage(
            vec![b"loan_id".to_vec()],
            vec![true],
            vec![true],
            Some(&old),
            Some(&new),
            Some(0.6),
        );
        assert_eq!(below.status, CheckStatus::Fail);

        let at_floor = evaluate_key_viability_with_min_coverage(
            vec![b"loan_id".to_vec()],
            vec![true],
            vec![true],
            Some(&old),
            Some(&new),
            Some(0.5),
        );
        assert_eq!(at_floor.status, CheckStatus::Pass);

        let zero_floor = evaluate_key_viability_with_min_coverage(
            vec![b"loan_id".to_vec()],
            vec![true],
            vec![true],
            Some(&key_scan(&[b"OLD"], 0, 0)),
            Some(&key_scan(&[b"NEW"], 0, 0)),
            Some(0.0),
        );
        assert_eq!(zero_floor.status, CheckStatus::Fail);
    }

    #[test]
    fn composite_key_found_requires_all_components() {
        let scan = key_scan(&[b"A1"], 0, 0);

        let result = evaluate_key_viability(
            vec![b"loan_id".to_vec(), b"unit".to_vec()],
            vec![true, true],
            vec![true, false],
            Some(&scan),
            None,
        );

        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.found_old);
        assert!(!result.found_new);
        assert_eq!(result.found_components_new, vec![true, false]);
    }

    #[test]
    fn missing_composite_component_suppresses_scan_derived_metrics_for_that_file() {
        let old = key_scan(&[b"A1"], 0, 0);
        let new = key_scan(&[b"A1"], 0, 0);

        let result = evaluate_key_viability(
            vec![b"loan_id".to_vec(), b"unit".to_vec()],
            vec![true, true],
            vec![true, false],
            Some(&old),
            Some(&new),
        );

        assert_eq!(result.status, CheckStatus::Fail);
        assert_eq!(result.unique_old, Some(true));
        assert_eq!(result.unique_new, None);
        assert_eq!(result.duplicate_values_new, None);
        assert_eq!(result.empty_values_new, None);
        assert_eq!(result.coverage, None);
    }
}
