use crate::checks::suite::CheckStatus;
use crate::scan::ColumnClassification;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeShift {
    pub column: Vec<u8>,
    pub old_type: ColumnClassification,
    pub new_type: ColumnClassification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeConsistencyResult {
    pub status: CheckStatus,
    pub numeric_columns: u64,
    pub type_shifts: Vec<TypeShift>,
}

pub fn evaluate_type_consistency(
    columns_common: &[Vec<u8>],
    old_types: &[ColumnClassification],
    new_types: &[ColumnClassification],
) -> TypeConsistencyResult {
    debug_assert_eq!(columns_common.len(), old_types.len());
    debug_assert_eq!(columns_common.len(), new_types.len());

    let mut numeric_columns = 0u64;
    let mut type_shifts = Vec::new();

    for ((column, old_type), new_type) in columns_common.iter().zip(old_types).zip(new_types) {
        if *old_type == ColumnClassification::Numeric && *new_type == ColumnClassification::Numeric
        {
            numeric_columns += 1;
        }

        if is_type_shift(*old_type, *new_type) {
            type_shifts.push(TypeShift {
                column: column.clone(),
                old_type: *old_type,
                new_type: *new_type,
            });
        }
    }

    let status = if type_shifts.is_empty() {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };

    TypeConsistencyResult {
        status,
        numeric_columns,
        type_shifts,
    }
}

fn is_type_shift(old_type: ColumnClassification, new_type: ColumnClassification) -> bool {
    matches!(
        (old_type, new_type),
        (
            ColumnClassification::Numeric,
            ColumnClassification::NonNumeric
        ) | (
            ColumnClassification::NonNumeric,
            ColumnClassification::Numeric
        )
    )
}

#[cfg(test)]
mod tests {
    use super::evaluate_type_consistency;
    use crate::checks::suite::CheckStatus;
    use crate::scan::ColumnClassification;

    #[test]
    fn passes_when_no_numeric_non_numeric_shift_exists() {
        let columns = vec![b"loan_id".to_vec(), b"amount".to_vec(), b"notes".to_vec()];
        let old = vec![
            ColumnClassification::AllMissing,
            ColumnClassification::Numeric,
            ColumnClassification::NonNumeric,
        ];
        let new = vec![
            ColumnClassification::NonNumeric,
            ColumnClassification::Numeric,
            ColumnClassification::AllMissing,
        ];

        let result = evaluate_type_consistency(&columns, &old, &new);
        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(result.numeric_columns, 1);
        assert!(result.type_shifts.is_empty());
    }

    #[test]
    fn fails_for_numeric_to_non_numeric_and_reverse_shifts() {
        let columns = vec![b"balance".to_vec(), b"grade".to_vec(), b"zip".to_vec()];
        let old = vec![
            ColumnClassification::Numeric,
            ColumnClassification::NonNumeric,
            ColumnClassification::Numeric,
        ];
        let new = vec![
            ColumnClassification::NonNumeric,
            ColumnClassification::Numeric,
            ColumnClassification::Numeric,
        ];

        let result = evaluate_type_consistency(&columns, &old, &new);
        assert_eq!(result.status, CheckStatus::Fail);
        assert_eq!(result.numeric_columns, 1);
        assert_eq!(result.type_shifts.len(), 2);

        assert_eq!(result.type_shifts[0].column, b"balance".to_vec());
        assert_eq!(
            result.type_shifts[0].old_type,
            ColumnClassification::Numeric
        );
        assert_eq!(
            result.type_shifts[0].new_type,
            ColumnClassification::NonNumeric
        );

        assert_eq!(result.type_shifts[1].column, b"grade".to_vec());
        assert_eq!(
            result.type_shifts[1].old_type,
            ColumnClassification::NonNumeric
        );
        assert_eq!(
            result.type_shifts[1].new_type,
            ColumnClassification::Numeric
        );
    }
}
