use crate::checks::suite::CheckStatus;
use crate::scan::KeyScan;

#[derive(Debug, Clone, PartialEq)]
pub struct KeyViabilityResult {
    pub status: CheckStatus,
    pub key_column: Vec<u8>,
    pub found_old: bool,
    pub found_new: bool,
    pub unique_old: Option<bool>,
    pub unique_new: Option<bool>,
    pub duplicate_values_old: Option<u64>,
    pub duplicate_values_new: Option<u64>,
    pub empty_values_old: Option<u64>,
    pub empty_values_new: Option<u64>,
    pub coverage: Option<f64>,
}

pub fn evaluate_key_viability(
    key_column: Vec<u8>,
    found_old: bool,
    found_new: bool,
    old_scan: Option<&KeyScan>,
    new_scan: Option<&KeyScan>,
) -> KeyViabilityResult {
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

    let status = if found_old && found_new && unique_old == Some(true) && unique_new == Some(true) {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };

    KeyViabilityResult {
        status,
        key_column,
        found_old,
        found_new,
        unique_old,
        unique_new,
        duplicate_values_old,
        duplicate_values_new,
        empty_values_old,
        empty_values_new,
        coverage,
    }
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

    use super::evaluate_key_viability;
    use crate::checks::suite::CheckStatus;
    use crate::scan::KeyScan;

    fn key_scan(values: &[&[u8]], duplicate_count: u64, empty_count: u64) -> KeyScan {
        KeyScan {
            values: values.iter().map(|v| v.to_vec()).collect::<HashSet<_>>(),
            duplicate_count,
            empty_count,
        }
    }

    #[test]
    fn passes_when_found_unique_and_non_empty_in_both_files() {
        let old = key_scan(&[b"A1", b"A2", b"A3"], 0, 0);
        let new = key_scan(&[b"A1", b"A2", b"A4"], 0, 0);

        let result =
            evaluate_key_viability(b"loan_id".to_vec(), true, true, Some(&old), Some(&new));

        assert_eq!(result.status, CheckStatus::Pass);
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

        let result = evaluate_key_viability(b"loan_id".to_vec(), true, false, Some(&old), None);

        assert_eq!(result.status, CheckStatus::Fail);
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

        let result =
            evaluate_key_viability(b"loan_id".to_vec(), true, true, Some(&old), Some(&new));

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

        let result =
            evaluate_key_viability(b"loan_id".to_vec(), true, true, Some(&old), Some(&new));

        assert_eq!(result.coverage, Some(0.0));
    }
}
