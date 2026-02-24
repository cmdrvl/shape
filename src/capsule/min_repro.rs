use std::collections::BTreeSet;

use crate::checks::suite::Outcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReproLimits {
    pub max_rows: usize,
    pub max_bytes: usize,
}

impl ReproLimits {
    pub fn new(max_rows: usize, max_bytes: usize) -> Self {
        Self {
            max_rows,
            max_bytes,
        }
    }

    pub fn from_optional(max_rows: Option<u64>, max_bytes: Option<u64>) -> Self {
        let defaults = Self::default();
        Self {
            max_rows: max_rows
                .map(|value| value.min(usize::MAX as u64) as usize)
                .unwrap_or(defaults.max_rows),
            max_bytes: max_bytes
                .map(|value| value.min(usize::MAX as u64) as usize)
                .unwrap_or(defaults.max_bytes),
        }
    }
}

impl Default for ReproLimits {
    fn default() -> Self {
        Self::new(128, 64 * 1024)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReproSlice {
    pub row_indices: Vec<usize>,
    pub estimated_bytes: usize,
    pub truncated_by_rows: bool,
    pub truncated_by_bytes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedRepro {
    pub bytes: Vec<u8>,
    pub kept_data_rows: usize,
    pub truncated_by_rows: bool,
    pub truncated_by_bytes: bool,
}

pub fn extract_minimal_repro(
    source: &[u8],
    outcome: Outcome,
    refusal_code: Option<&str>,
    limits: ReproLimits,
) -> ExtractedRepro {
    if source.is_empty() {
        return ExtractedRepro {
            bytes: Vec::new(),
            kept_data_rows: 0,
            truncated_by_rows: false,
            truncated_by_bytes: false,
        };
    }

    let lines = split_lines(source);
    if lines.is_empty() {
        return ExtractedRepro {
            bytes: source.to_vec(),
            kept_data_rows: 0,
            truncated_by_rows: false,
            truncated_by_bytes: false,
        };
    }

    let prefix_count = prefix_line_count(&lines);
    let effective_prefix = prefix_count.min(lines.len());
    let (prefix_lines, data_lines) = lines.split_at(effective_prefix);

    let target_rows = target_data_rows(outcome, refusal_code, data_lines.len(), limits.max_rows);
    let data_row_sizes: Vec<usize> = data_lines.iter().map(|line| line.len()).collect();
    let required_row_indices: Vec<usize> = (0..target_rows).collect();

    let prefix_bytes = prefix_lines.iter().map(|line| line.len()).sum::<usize>();
    let available_data_bytes = limits.max_bytes.saturating_sub(prefix_bytes);
    let slice = if target_rows == 0 || data_lines.is_empty() {
        ReproSlice {
            row_indices: Vec::new(),
            estimated_bytes: 0,
            truncated_by_rows: data_lines.len() > target_rows,
            truncated_by_bytes: false,
        }
    } else {
        build_minimal_slice(
            data_lines.len(),
            &required_row_indices,
            &data_row_sizes,
            ReproLimits::new(target_rows, available_data_bytes),
        )
    };

    let mut bytes = Vec::with_capacity(prefix_bytes + slice.estimated_bytes);
    for line in prefix_lines {
        bytes.extend_from_slice(line);
    }
    for index in &slice.row_indices {
        bytes.extend_from_slice(data_lines[*index]);
    }

    if bytes.is_empty() {
        bytes.extend_from_slice(source);
    }

    ExtractedRepro {
        bytes,
        kept_data_rows: slice.row_indices.len(),
        truncated_by_rows: slice.truncated_by_rows || data_lines.len() > target_rows,
        truncated_by_bytes: slice.truncated_by_bytes,
    }
}

pub fn build_minimal_slice(
    total_rows: usize,
    required_row_indices: &[usize],
    estimated_row_sizes: &[usize],
    limits: ReproLimits,
) -> ReproSlice {
    let capped_total_rows = total_rows.min(estimated_row_sizes.len());
    if capped_total_rows == 0 || limits.max_rows == 0 {
        return ReproSlice {
            row_indices: Vec::new(),
            estimated_bytes: 0,
            truncated_by_rows: total_rows > 0,
            truncated_by_bytes: false,
        };
    }

    let mut selected = BTreeSet::new();
    for &index in required_row_indices {
        if index < capped_total_rows {
            selected.insert(index);
        }
    }

    if selected.is_empty() {
        selected.insert(0);
    }

    let mut ordered: Vec<usize> = selected.into_iter().collect();
    let mut truncated_by_rows = false;
    if ordered.len() > limits.max_rows {
        ordered.truncate(limits.max_rows);
        truncated_by_rows = true;
    }

    let mut estimated_bytes = sum_sizes(&ordered, estimated_row_sizes);
    let mut truncated_by_bytes = false;
    while estimated_bytes > limits.max_bytes && ordered.len() > 1 {
        ordered.pop();
        estimated_bytes = sum_sizes(&ordered, estimated_row_sizes);
        truncated_by_bytes = true;
    }

    ReproSlice {
        row_indices: ordered,
        estimated_bytes,
        truncated_by_rows,
        truncated_by_bytes,
    }
}

fn split_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut start = 0usize;

    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            lines.push(&bytes[start..=index]);
            start = index + 1;
        }
    }

    if start < bytes.len() {
        lines.push(&bytes[start..]);
    }

    lines
}

fn prefix_line_count(lines: &[&[u8]]) -> usize {
    if lines.is_empty() {
        return 0;
    }

    if is_sep_directive(lines[0]) && lines.len() >= 2 {
        2
    } else {
        1
    }
}

fn is_sep_directive(line: &[u8]) -> bool {
    let mut trimmed = line;
    if trimmed.last().copied() == Some(b'\n') {
        trimmed = &trimmed[..trimmed.len() - 1];
    }
    if trimmed.last().copied() == Some(b'\r') {
        trimmed = &trimmed[..trimmed.len() - 1];
    }
    trimmed.len() == 5 && trimmed.starts_with(b"sep=")
}

fn target_data_rows(
    outcome: Outcome,
    refusal_code: Option<&str>,
    available_rows: usize,
    max_rows: usize,
) -> usize {
    if available_rows == 0 || max_rows == 0 {
        return 0;
    }

    let desired = match outcome {
        Outcome::Compatible => 1,
        Outcome::Incompatible => 4,
        Outcome::Refusal => match refusal_code {
            Some("E_EMPTY") => 0,
            Some("E_HEADERS") => 0,
            Some("E_DIALECT") => 3,
            Some("E_CSV_PARSE") => 3,
            _ => 1,
        },
    };

    desired.min(available_rows).min(max_rows)
}

fn sum_sizes(row_indices: &[usize], estimated_row_sizes: &[usize]) -> usize {
    row_indices
        .iter()
        .map(|index| estimated_row_sizes[*index])
        .sum()
}

#[cfg(test)]
mod tests {
    use crate::checks::suite::Outcome;

    use super::{ReproLimits, build_minimal_slice, extract_minimal_repro};

    #[test]
    fn compatible_keeps_header_and_one_data_row() {
        let source = b"loan_id,balance\nA1,100\nA2,150\nA3,175\n";
        let extracted =
            extract_minimal_repro(source, Outcome::Compatible, None, ReproLimits::default());

        assert_eq!(extracted.bytes, b"loan_id,balance\nA1,100\n");
        assert_eq!(extracted.kept_data_rows, 1);
        assert!(extracted.truncated_by_rows);
        assert!(!extracted.truncated_by_bytes);
    }

    #[test]
    fn incompatible_respects_row_guardrail() {
        let source = b"loan_id,balance\nA1,100\nA2,150\nA3,175\nA4,200\nA5,250\n";
        let extracted = extract_minimal_repro(
            source,
            Outcome::Incompatible,
            None,
            ReproLimits::new(2, 4096),
        );

        assert_eq!(extracted.bytes, b"loan_id,balance\nA1,100\nA2,150\n");
        assert_eq!(extracted.kept_data_rows, 2);
        assert!(extracted.truncated_by_rows);
    }

    #[test]
    fn refusal_empty_keeps_only_prefix_lines() {
        let source = b"sep=;\nloan_id;balance\nA1;100\nA2;150\n";
        let extracted = extract_minimal_repro(
            source,
            Outcome::Refusal,
            Some("E_EMPTY"),
            ReproLimits::default(),
        );

        assert_eq!(extracted.bytes, b"sep=;\nloan_id;balance\n");
        assert_eq!(extracted.kept_data_rows, 0);
        assert!(extracted.truncated_by_rows);
    }

    #[test]
    fn byte_guardrail_drops_trailing_rows() {
        let source = b"loan_id,balance\nA1,1000000000\nA2,2000000000\nA3,3000000000\n";
        let extracted =
            extract_minimal_repro(source, Outcome::Incompatible, None, ReproLimits::new(4, 34));

        assert_eq!(extracted.bytes, b"loan_id,balance\nA1,1000000000\n");
        assert_eq!(extracted.kept_data_rows, 1);
        assert!(extracted.truncated_by_bytes);
    }

    #[test]
    fn defaults_to_first_row_when_no_required_indices() {
        let slice = build_minimal_slice(3, &[], &[12, 8, 9], ReproLimits::new(10, 1024));

        assert_eq!(slice.row_indices, vec![0]);
        assert_eq!(slice.estimated_bytes, 12);
        assert!(!slice.truncated_by_rows);
        assert!(!slice.truncated_by_bytes);
    }

    #[test]
    fn keeps_required_rows_in_sorted_deterministic_order() {
        let slice = build_minimal_slice(
            6,
            &[4, 2, 4, 1, 5],
            &[3, 5, 7, 11, 13, 17],
            ReproLimits::new(10, 1024),
        );

        assert_eq!(slice.row_indices, vec![1, 2, 4, 5]);
        assert_eq!(slice.estimated_bytes, 42);
    }

    #[test]
    fn truncates_by_row_limit() {
        let slice = build_minimal_slice(
            6,
            &[0, 1, 2, 3, 4],
            &[9, 9, 9, 9, 9, 9],
            ReproLimits::new(3, 1024),
        );

        assert_eq!(slice.row_indices, vec![0, 1, 2]);
        assert_eq!(slice.estimated_bytes, 27);
        assert!(slice.truncated_by_rows);
        assert!(!slice.truncated_by_bytes);
    }

    #[test]
    fn truncates_by_byte_limit_but_keeps_at_least_one_row() {
        let slice = build_minimal_slice(
            5,
            &[0, 1, 2, 3],
            &[20, 20, 20, 20, 20],
            ReproLimits::new(5, 35),
        );

        assert_eq!(slice.row_indices, vec![0]);
        assert_eq!(slice.estimated_bytes, 20);
        assert!(!slice.truncated_by_rows);
        assert!(slice.truncated_by_bytes);
    }
}
