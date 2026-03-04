use std::collections::HashSet;

use crate::checks::suite::CheckStatus;

#[derive(Debug, Clone, PartialEq)]
pub struct SchemaOverlapResult {
    pub status: CheckStatus,
    pub columns_common: Vec<Vec<u8>>,
    pub columns_old_only: Vec<Vec<u8>>,
    pub columns_new_only: Vec<Vec<u8>>,
    pub overlap_ratio: f64,
}

impl SchemaOverlapResult {
    pub fn columns_common_count(&self) -> u64 {
        self.columns_common.len() as u64
    }
}

pub fn evaluate_schema_overlap(
    old_headers: &[Vec<u8>],
    new_headers: &[Vec<u8>],
    include_set: Option<&HashSet<Vec<u8>>>,
) -> SchemaOverlapResult {
    // When a profile is active, restrict both header lists to profile columns only.
    let (effective_old, effective_new): (Vec<Vec<u8>>, Vec<Vec<u8>>) = match include_set {
        Some(set) => (
            old_headers
                .iter()
                .filter(|h| set.contains(h.as_slice()))
                .cloned()
                .collect(),
            new_headers
                .iter()
                .filter(|h| set.contains(h.as_slice()))
                .cloned()
                .collect(),
        ),
        None => (old_headers.to_vec(), new_headers.to_vec()),
    };

    let old_set: HashSet<&[u8]> = effective_old.iter().map(Vec::as_slice).collect();
    let new_set: HashSet<&[u8]> = effective_new.iter().map(Vec::as_slice).collect();

    let columns_common: Vec<Vec<u8>> = effective_old
        .iter()
        .filter(|header| new_set.contains(header.as_slice()))
        .cloned()
        .collect();

    let columns_old_only: Vec<Vec<u8>> = effective_old
        .iter()
        .filter(|header| !new_set.contains(header.as_slice()))
        .cloned()
        .collect();

    let columns_new_only: Vec<Vec<u8>> = effective_new
        .iter()
        .filter(|header| !old_set.contains(header.as_slice()))
        .cloned()
        .collect();

    // When a profile is active, denominator is include_set.len() — profile columns
    // absent from both files penalize overlap.
    let denominator = match include_set {
        Some(set) => set.len(),
        None => columns_common.len() + columns_old_only.len() + columns_new_only.len(),
    };
    let overlap_ratio = if denominator == 0 {
        0.0
    } else {
        columns_common.len() as f64 / denominator as f64
    };

    let status = if columns_common.is_empty() {
        CheckStatus::Fail
    } else {
        CheckStatus::Pass
    };

    SchemaOverlapResult {
        status,
        columns_common,
        columns_old_only,
        columns_new_only,
        overlap_ratio,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::evaluate_schema_overlap;
    use crate::checks::suite::CheckStatus;

    #[test]
    fn evaluates_full_overlap_as_pass() {
        let old = vec![b"loan_id".to_vec(), b"amount".to_vec()];
        let new = vec![b"loan_id".to_vec(), b"amount".to_vec()];

        let result = evaluate_schema_overlap(&old, &new, None);
        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(result.columns_common, old);
        assert!(result.columns_old_only.is_empty());
        assert!(result.columns_new_only.is_empty());
        assert_eq!(result.overlap_ratio, 1.0);
    }

    #[test]
    fn evaluates_partial_overlap_with_old_and_new_only_columns() {
        let old = vec![b"loan_id".to_vec(), b"amount".to_vec(), b"legacy".to_vec()];
        let new = vec![b"loan_id".to_vec(), b"amount".to_vec(), b"added".to_vec()];

        let result = evaluate_schema_overlap(&old, &new, None);
        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(
            result.columns_common,
            vec![b"loan_id".to_vec(), b"amount".to_vec()]
        );
        assert_eq!(result.columns_old_only, vec![b"legacy".to_vec()]);
        assert_eq!(result.columns_new_only, vec![b"added".to_vec()]);
        assert_eq!(result.overlap_ratio, 0.5);
    }

    #[test]
    fn zero_overlap_fails() {
        let old = vec![b"a".to_vec(), b"b".to_vec()];
        let new = vec![b"x".to_vec(), b"y".to_vec()];

        let result = evaluate_schema_overlap(&old, &new, None);
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.columns_common.is_empty());
        assert_eq!(result.columns_old_only, old);
        assert_eq!(result.columns_new_only, new);
        assert_eq!(result.overlap_ratio, 0.0);
    }

    // --- Profile-scoped overlap tests (bd-3k9r) ---

    #[test]
    fn overlap_with_profile_measures_only_include_columns() {
        // 50 shared columns, but profile cares about 4.
        let mut old: Vec<Vec<u8>> = (0..50).map(|i| format!("col_{i}").into_bytes()).collect();
        let mut new = old.clone();
        // Add extra columns unique to each side.
        old.push(b"legacy".to_vec());
        new.push(b"added".to_vec());

        let include: HashSet<Vec<u8>> = HashSet::from([
            b"col_0".to_vec(),
            b"col_1".to_vec(),
            b"col_2".to_vec(),
            b"col_3".to_vec(),
        ]);

        let result = evaluate_schema_overlap(&old, &new, Some(&include));
        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(result.columns_common.len(), 4);
        assert!(result.columns_old_only.is_empty());
        assert!(result.columns_new_only.is_empty());
        assert_eq!(result.overlap_ratio, 1.0);
    }

    #[test]
    fn overlap_without_profile_measures_all_columns() {
        let old = vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()];
        let new = vec![b"a".to_vec(), b"b".to_vec(), b"d".to_vec()];

        let result = evaluate_schema_overlap(&old, &new, None);
        assert_eq!(result.columns_common.len(), 2);
        assert_eq!(result.overlap_ratio, 0.5); // 2 / 4 (union)
    }

    #[test]
    fn profile_columns_not_in_either_file_reduce_overlap() {
        // Profile: [a, b, c, d], old: [a, b, x], new: [a, c, y]
        let old = vec![b"a".to_vec(), b"b".to_vec(), b"x".to_vec()];
        let new = vec![b"a".to_vec(), b"c".to_vec(), b"y".to_vec()];
        let include: HashSet<Vec<u8>> =
            HashSet::from([b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()]);

        let result = evaluate_schema_overlap(&old, &new, Some(&include));
        // Effective old: [a, b], effective new: [a, c]
        // common: [a], old_only: [b], new_only: [c], denominator: 4 (include_set)
        assert_eq!(result.columns_common, vec![b"a".to_vec()]);
        assert_eq!(result.columns_old_only, vec![b"b".to_vec()]);
        assert_eq!(result.columns_new_only, vec![b"c".to_vec()]);
        assert_eq!(result.overlap_ratio, 0.25); // 1 / 4
    }

    #[test]
    fn profile_with_all_columns_present_reports_full_overlap() {
        let old = vec![
            b"loan_id".to_vec(),
            b"balance".to_vec(),
            b"rate".to_vec(),
            b"extra".to_vec(),
        ];
        let new = vec![
            b"loan_id".to_vec(),
            b"balance".to_vec(),
            b"rate".to_vec(),
            b"other".to_vec(),
        ];
        let include: HashSet<Vec<u8>> =
            HashSet::from([b"loan_id".to_vec(), b"balance".to_vec(), b"rate".to_vec()]);

        let result = evaluate_schema_overlap(&old, &new, Some(&include));
        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(result.columns_common.len(), 3);
        assert_eq!(result.overlap_ratio, 1.0); // 3 / 3
    }

    #[test]
    fn profile_with_zero_matching_columns_reports_no_overlap() {
        let old = vec![b"x".to_vec(), b"y".to_vec()];
        let new = vec![b"x".to_vec(), b"y".to_vec()];
        let include: HashSet<Vec<u8>> =
            HashSet::from([b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);

        let result = evaluate_schema_overlap(&old, &new, Some(&include));
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.columns_common.is_empty());
        assert_eq!(result.overlap_ratio, 0.0); // 0 / 3
    }

    #[test]
    fn empty_profile_include_columns() {
        let old = vec![b"a".to_vec()];
        let new = vec![b"a".to_vec()];
        let include: HashSet<Vec<u8>> = HashSet::new();

        let result = evaluate_schema_overlap(&old, &new, Some(&include));
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.columns_common.is_empty());
        assert_eq!(result.overlap_ratio, 0.0); // 0 / 0 → 0.0
    }

    #[test]
    fn profile_with_nonexistent_columns_only() {
        let old = vec![b"x".to_vec(), b"y".to_vec()];
        let new = vec![b"p".to_vec(), b"q".to_vec()];
        let include: HashSet<Vec<u8>> = HashSet::from([b"ghost_a".to_vec(), b"ghost_b".to_vec()]);

        let result = evaluate_schema_overlap(&old, &new, Some(&include));
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.columns_common.is_empty());
        assert!(result.columns_old_only.is_empty());
        assert!(result.columns_new_only.is_empty());
        assert_eq!(result.overlap_ratio, 0.0); // 0 / 2
    }
}
