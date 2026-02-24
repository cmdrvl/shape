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
) -> SchemaOverlapResult {
    let old_set: HashSet<&[u8]> = old_headers.iter().map(Vec::as_slice).collect();
    let new_set: HashSet<&[u8]> = new_headers.iter().map(Vec::as_slice).collect();

    let columns_common: Vec<Vec<u8>> = old_headers
        .iter()
        .filter(|header| new_set.contains(header.as_slice()))
        .cloned()
        .collect();

    let columns_old_only: Vec<Vec<u8>> = old_headers
        .iter()
        .filter(|header| !new_set.contains(header.as_slice()))
        .cloned()
        .collect();

    let columns_new_only: Vec<Vec<u8>> = new_headers
        .iter()
        .filter(|header| !old_set.contains(header.as_slice()))
        .cloned()
        .collect();

    let union_count = columns_common.len() + columns_old_only.len() + columns_new_only.len();
    let overlap_ratio = if union_count == 0 {
        0.0
    } else {
        columns_common.len() as f64 / union_count as f64
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
    use super::evaluate_schema_overlap;
    use crate::checks::suite::CheckStatus;

    #[test]
    fn evaluates_full_overlap_as_pass() {
        let old = vec![b"loan_id".to_vec(), b"amount".to_vec()];
        let new = vec![b"loan_id".to_vec(), b"amount".to_vec()];

        let result = evaluate_schema_overlap(&old, &new);
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

        let result = evaluate_schema_overlap(&old, &new);
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

        let result = evaluate_schema_overlap(&old, &new);
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.columns_common.is_empty());
        assert_eq!(result.columns_old_only, old);
        assert_eq!(result.columns_new_only, new);
        assert_eq!(result.overlap_ratio, 0.0);
    }
}
