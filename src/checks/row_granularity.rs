use std::collections::HashSet;

use crate::checks::suite::CheckStatus;
use crate::scan::KeyScan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowGranularityResult {
    pub status: CheckStatus,
    pub rows_old: u64,
    pub rows_new: u64,
    pub key_overlap: Option<u64>,
    pub keys_old_only: Option<u64>,
    pub keys_new_only: Option<u64>,
}

pub fn evaluate_row_granularity(
    rows_old: u64,
    rows_new: u64,
    key_metrics: Option<(u64, u64, u64)>,
) -> RowGranularityResult {
    let (key_overlap, keys_old_only, keys_new_only) = match key_metrics {
        Some((overlap, old_only, new_only)) => (Some(overlap), Some(old_only), Some(new_only)),
        None => (None, None, None),
    };

    RowGranularityResult {
        status: CheckStatus::Pass,
        rows_old,
        rows_new,
        key_overlap,
        keys_old_only,
        keys_new_only,
    }
}

pub fn compute_key_overlap_metrics(old: &KeyScan, new: &KeyScan) -> (u64, u64, u64) {
    let old_values: HashSet<&[u8]> = old.values.iter().map(Vec::as_slice).collect();
    let new_values: HashSet<&[u8]> = new.values.iter().map(Vec::as_slice).collect();

    let overlap = old_values.intersection(&new_values).count() as u64;
    let old_only = old_values.len() as u64 - overlap;
    let new_only = new_values.len() as u64 - overlap;
    (overlap, old_only, new_only)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{compute_key_overlap_metrics, evaluate_row_granularity};
    use crate::checks::suite::CheckStatus;
    use crate::scan::KeyScan;

    #[test]
    fn row_granularity_always_passes_without_key_metrics() {
        let result = evaluate_row_granularity(10, 12, None);

        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(result.rows_old, 10);
        assert_eq!(result.rows_new, 12);
        assert_eq!(result.key_overlap, None);
        assert_eq!(result.keys_old_only, None);
        assert_eq!(result.keys_new_only, None);
    }

    #[test]
    fn row_granularity_reports_key_metrics_when_present() {
        let result = evaluate_row_granularity(5, 6, Some((4, 1, 2)));

        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(result.rows_old, 5);
        assert_eq!(result.rows_new, 6);
        assert_eq!(result.key_overlap, Some(4));
        assert_eq!(result.keys_old_only, Some(1));
        assert_eq!(result.keys_new_only, Some(2));
    }

    #[test]
    fn computes_key_overlap_old_only_and_new_only_counts() {
        let old = KeyScan {
            values: HashSet::from([b"A".to_vec(), b"B".to_vec(), b"C".to_vec()]),
            duplicate_count: 0,
            empty_count: 0,
        };
        let new = KeyScan {
            values: HashSet::from([b"B".to_vec(), b"D".to_vec()]),
            duplicate_count: 0,
            empty_count: 0,
        };

        let (overlap, old_only, new_only) = compute_key_overlap_metrics(&old, &new);
        assert_eq!(overlap, 1);
        assert_eq!(old_only, 2);
        assert_eq!(new_only, 1);
    }
}
