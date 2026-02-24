use crate::checks::suite::Outcome;
use crate::checks::suite::{CheckStatus, CheckSuite};
use crate::checks::type_consistency::TypeShift;
use crate::csv::dialect::{Dialect, EscapeMode};
use crate::refusal::codes::RefusalCode;
use crate::refusal::payload::RefusalPayload;
use crate::scan::ColumnClassification;
use crate::{checks::key_viability::KeyViabilityResult, format::numbers};

pub fn render_outcome_header(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Compatible => "COMPATIBLE",
        Outcome::Incompatible => "INCOMPATIBLE",
        Outcome::Refusal => "REFUSAL",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefusalRenderContext<'a> {
    pub old_path: &'a str,
    pub new_path: &'a str,
    pub dialect_old: Option<Dialect>,
    pub dialect_new: Option<Dialect>,
}

pub fn render_refusal(refusal: &RefusalPayload) -> String {
    let mut out = String::new();
    out.push_str(&format!("SHAPE ERROR ({})\n\n", refusal.code.as_str()));
    out.push_str(&refusal.message);
    out.push('\n');
    out.push_str(&format!("Next: {}\n", refusal_next_step(refusal)));
    out
}

pub fn render_refusal_with_context(
    refusal: &RefusalPayload,
    context: &RefusalRenderContext<'_>,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("SHAPE ERROR ({})\n\n", refusal.code.as_str()));
    out.push_str(&format!(
        "Compared: {} -> {}\n",
        context.old_path, context.new_path
    ));
    if let Some(dialect) = context.dialect_old {
        out.push_str(&format!(
            "Dialect(old): {}\n",
            format_dialect_display(dialect)
        ));
    }
    if let Some(dialect) = context.dialect_new {
        out.push_str(&format!(
            "Dialect(new): {}\n",
            format_dialect_display(dialect)
        ));
    }
    out.push('\n');
    out.push_str(&refusal.message);
    out.push('\n');
    out.push_str(&format!("Next: {}\n", refusal_next_step(refusal)));
    out
}

pub fn format_dialect_display(dialect: Dialect) -> String {
    let escape = match dialect.escape {
        EscapeMode::None => "none",
        EscapeMode::Backslash => "backslash",
    };
    format!(
        "delimiter={} quote=\" escape={}",
        dialect.delimiter_display(),
        escape
    )
}

fn refusal_next_step(refusal: &RefusalPayload) -> &str {
    refusal
        .next_command
        .as_deref()
        .unwrap_or(default_next_step(refusal.code))
}

fn default_next_step(code: RefusalCode) -> &'static str {
    match code {
        RefusalCode::EIo => "check file path and permissions.",
        RefusalCode::EEncoding => "convert/re-export as UTF-8.",
        RefusalCode::ECsvParse => "re-export as standard RFC4180 CSV.",
        RefusalCode::EEmpty => "provide non-empty datasets.",
        RefusalCode::EHeaders => "fix headers or re-export.",
        RefusalCode::EDialect => "use --delimiter <delim>.",
        RefusalCode::EAmbiguousProfile => "provide exactly one profile selector.",
        RefusalCode::EInputNotLocked => "re-run with correct --lock or lock inputs first.",
        RefusalCode::EInputDrift => "use the locked file; regenerate lock if expected.",
        RefusalCode::ETooLarge => "increase limit or split input.",
    }
}

pub fn render_compatible(
    old_path: &str,
    new_path: &str,
    old_dialect: Dialect,
    new_dialect: Dialect,
    suite: &CheckSuite,
) -> String {
    render_structural_outcome(
        Outcome::Compatible,
        old_path,
        new_path,
        old_dialect,
        new_dialect,
        suite,
        &[],
    )
}

pub fn render_incompatible(
    old_path: &str,
    new_path: &str,
    old_dialect: Dialect,
    new_dialect: Dialect,
    suite: &CheckSuite,
    reasons: &[String],
) -> String {
    render_structural_outcome(
        Outcome::Incompatible,
        old_path,
        new_path,
        old_dialect,
        new_dialect,
        suite,
        reasons,
    )
}

fn render_structural_outcome(
    outcome: Outcome,
    old_path: &str,
    new_path: &str,
    old_dialect: Dialect,
    new_dialect: Dialect,
    suite: &CheckSuite,
    reasons: &[String],
) -> String {
    let mut out = String::new();
    out.push_str("SHAPE\n\n");
    out.push_str(render_outcome_header(outcome));
    out.push_str("\n\n");
    out.push_str(&format!("Compared: {old_path} -> {new_path}\n"));

    if let Some(key) = suite.key_viability.as_ref() {
        out.push_str(&format!("Key: {}\n", render_key_header_line(key)));
    }
    out.push_str(&format!(
        "Dialect(old): {}\n",
        format_dialect_display(old_dialect)
    ));
    out.push_str(&format!(
        "Dialect(new): {}\n",
        format_dialect_display(new_dialect)
    ));
    out.push('\n');

    out.push_str(&render_schema_block(suite));
    if let Some(key) = suite.key_viability.as_ref() {
        out.push_str(&format!("Key:       {}\n", render_key_detail_line(key)));
    }
    out.push_str(&format!("Rows:      {}\n", render_rows_line(suite)));
    out.push_str(&format!(
        "Types:     {}\n",
        render_types_summary_line(suite)
    ));

    for shift in &suite.type_consistency.type_shifts {
        out.push_str(&format!("           {}\n", render_type_shift_line(shift)));
    }

    if outcome == Outcome::Incompatible && !reasons.is_empty() {
        out.push_str("\nReasons:\n");
        for (index, reason) in reasons.iter().enumerate() {
            out.push_str(&format!("  {}. {}\n", index + 1, reason));
        }
    }

    out
}

fn render_schema_block(suite: &CheckSuite) -> String {
    let common = suite.schema_overlap.columns_common.len() as u64;
    let old_only = suite.schema_overlap.columns_old_only.len() as u64;
    let new_only = suite.schema_overlap.columns_new_only.len() as u64;
    let total = common + old_only + new_only;

    let mut out = String::new();
    out.push_str(&format!(
        "Schema:    {} common / {} total ({} overlap)\n",
        numbers::format_count(common),
        numbers::format_count(total),
        numbers::format_ratio_as_percent(suite.schema_overlap.overlap_ratio)
    ));

    if !suite.schema_overlap.columns_old_only.is_empty() {
        out.push_str(&format!(
            "           old_only: [{}]\n",
            join_human_identifiers(&suite.schema_overlap.columns_old_only)
        ));
    }
    if !suite.schema_overlap.columns_new_only.is_empty() {
        out.push_str(&format!(
            "           new_only: [{}]\n",
            join_human_identifiers(&suite.schema_overlap.columns_new_only)
        ));
    }

    out
}

fn render_key_header_line(key: &KeyViabilityResult) -> String {
    let column = String::from_utf8_lossy(&key.key_column);
    if key.status == CheckStatus::Pass {
        return format!("{column} (unique in both files)");
    }

    if !key.found_old && !key.found_new {
        return format!("{column} (NOT FOUND in old and new files)");
    }
    if !key.found_old {
        return format!("{column} (NOT FOUND in old file)");
    }
    if !key.found_new {
        return format!("{column} (NOT FOUND in new file)");
    }

    let issue_summary = key_viability_issue_fragments(key);
    if issue_summary.is_empty() {
        format!("{column} (NOT VIABLE)")
    } else {
        format!("{column} (NOT VIABLE — {})", issue_summary.join(", "))
    }
}

fn render_key_detail_line(key: &KeyViabilityResult) -> String {
    let column = String::from_utf8_lossy(&key.key_column);
    if !key.found_old && !key.found_new {
        return format!("{column} — not found in old and new files");
    }
    if !key.found_old {
        return format!("{column} — not found in old file");
    }
    if !key.found_new {
        return format!("{column} — not found in new file");
    }

    let uniqueness = if key.unique_old == Some(true) && key.unique_new == Some(true) {
        "unique in both".to_owned()
    } else {
        let issue_summary = key_viability_issue_fragments(key);
        if issue_summary.is_empty() {
            "NOT VIABLE".to_owned()
        } else {
            issue_summary.join(", ")
        }
    };

    match key.coverage {
        Some(coverage) => format!(
            "{column} — {uniqueness}, coverage={}",
            numbers::format_coverage(coverage)
        ),
        None => format!("{column} — {uniqueness}"),
    }
}

fn key_viability_issue_fragments(key: &KeyViabilityResult) -> Vec<String> {
    let mut issues = Vec::new();
    append_key_viability_issues(
        &mut issues,
        "old",
        key.duplicate_values_old,
        key.empty_values_old,
    );
    append_key_viability_issues(
        &mut issues,
        "new",
        key.duplicate_values_new,
        key.empty_values_new,
    );
    issues
}

fn append_key_viability_issues(
    issues: &mut Vec<String>,
    file_label: &str,
    duplicate_values: Option<u64>,
    empty_values: Option<u64>,
) {
    if let Some(duplicates) = duplicate_values.filter(|count| *count > 0) {
        let noun = if duplicates == 1 {
            "duplicate"
        } else {
            "duplicates"
        };
        issues.push(format!(
            "{} {noun} in {file_label}",
            numbers::format_count(duplicates),
        ));
    }
    if let Some(empty) = empty_values.filter(|count| *count > 0) {
        let noun = if empty == 1 {
            "empty value"
        } else {
            "empty values"
        };
        issues.push(format!(
            "{} {noun} in {file_label}",
            numbers::format_count(empty),
        ));
    }
}

fn render_rows_line(suite: &CheckSuite) -> String {
    let rows_old = numbers::format_count(suite.row_granularity.rows_old);
    let rows_new = numbers::format_count(suite.row_granularity.rows_new);

    match (
        suite.row_granularity.key_overlap,
        suite.row_granularity.keys_old_only,
        suite.row_granularity.keys_new_only,
    ) {
        (Some(overlap), Some(old_only), Some(new_only)) => format!(
            "{rows_old} old / {rows_new} new ({} removed, {} added, {} overlap)",
            numbers::format_count(old_only),
            numbers::format_count(new_only),
            numbers::format_count(overlap)
        ),
        _ => format!("{rows_old} old / {rows_new} new"),
    }
}

fn render_types_summary_line(suite: &CheckSuite) -> String {
    let type_shift_count = suite.type_consistency.type_shifts.len();
    let noun = if type_shift_count == 1 {
        "type shift"
    } else {
        "type shifts"
    };
    format!(
        "{} numeric columns, {} {}",
        suite.type_consistency.numeric_columns, type_shift_count, noun
    )
}

fn render_type_shift_line(shift: &TypeShift) -> String {
    format!(
        "{}: {} -> {}",
        String::from_utf8_lossy(&shift.column),
        classification_label(shift.old_type),
        classification_label(shift.new_type)
    )
}

fn classification_label(classification: ColumnClassification) -> &'static str {
    match classification {
        ColumnClassification::Numeric => "numeric",
        ColumnClassification::NonNumeric => "non-numeric",
        ColumnClassification::AllMissing => "all-missing",
    }
}

fn join_human_identifiers(columns: &[Vec<u8>]) -> String {
    columns
        .iter()
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::{
        RefusalRenderContext, format_dialect_display, render_compatible, render_incompatible,
        render_refusal, render_refusal_with_context,
    };
    use crate::checks::key_viability::KeyViabilityResult;
    use crate::checks::row_granularity::RowGranularityResult;
    use crate::checks::schema_overlap::SchemaOverlapResult;
    use crate::checks::suite::{CheckStatus, CheckSuite};
    use crate::checks::type_consistency::TypeConsistencyResult;
    use crate::csv::dialect::{Dialect, EscapeMode};
    use crate::refusal::payload::RefusalPayload;
    use crate::scan::ColumnClassification;

    #[test]
    fn refusal_render_uses_default_next_step_when_next_command_is_null() {
        let refusal = RefusalPayload::empty("new.csv", 0);
        let rendered = render_refusal(&refusal);
        assert!(rendered.contains("SHAPE ERROR (E_EMPTY)"));
        assert!(rendered.contains("One or both files empty"));
        assert!(rendered.contains("Next: provide non-empty datasets."));
    }

    #[test]
    fn refusal_render_prefers_next_command_when_available() {
        let refusal = RefusalPayload::dialect(
            "old.csv",
            vec!["0x2c".to_string(), "0x09".to_string()],
            Some("shape old.csv new.csv --delimiter tab --json".to_string()),
        );
        let rendered = render_refusal(&refusal);
        assert!(rendered.contains("Next: shape old.csv new.csv --delimiter tab --json"));
    }

    #[test]
    fn refusal_render_with_context_includes_paths_and_known_dialects() {
        let refusal = RefusalPayload::empty("new.csv", 0);
        let context = RefusalRenderContext {
            old_path: "old.csv",
            new_path: "new.csv",
            dialect_old: Some(Dialect {
                delimiter: b',',
                quote: b'"',
                escape: EscapeMode::None,
            }),
            dialect_new: Some(Dialect {
                delimiter: b'\t',
                quote: b'"',
                escape: EscapeMode::Backslash,
            }),
        };
        let rendered = render_refusal_with_context(&refusal, &context);

        assert!(rendered.contains("Compared: old.csv -> new.csv"));
        assert!(rendered.contains("Dialect(old): delimiter=, quote=\" escape=none"));
        assert!(rendered.contains("Dialect(new): delimiter=\\t quote=\" escape=backslash"));
    }

    #[test]
    fn dialect_display_uses_plan_escape_and_tab_rules() {
        let comma = Dialect {
            delimiter: b',',
            quote: b'"',
            escape: EscapeMode::None,
        };
        let tab = Dialect {
            delimiter: b'\t',
            quote: b'"',
            escape: EscapeMode::Backslash,
        };
        let equals = Dialect {
            delimiter: b'=',
            quote: b'"',
            escape: EscapeMode::None,
        };

        assert_eq!(
            format_dialect_display(comma),
            "delimiter=, quote=\" escape=none"
        );
        assert_eq!(
            format_dialect_display(tab),
            "delimiter=\\t quote=\" escape=backslash"
        );
        assert_eq!(
            format_dialect_display(equals),
            "delimiter== quote=\" escape=none"
        );
    }

    #[test]
    fn compatible_renderer_includes_expected_fixed_sections() {
        let suite = base_suite();
        let rendered = render_compatible(
            "old.csv",
            "new.csv",
            Dialect::default(),
            Dialect::default(),
            &suite,
        );

        assert!(rendered.contains("SHAPE\n\nCOMPATIBLE"));
        assert!(rendered.contains("Compared: old.csv -> new.csv"));
        assert!(rendered.contains("Dialect(old): delimiter=, quote=\" escape=none"));
        assert!(rendered.contains("Schema:    2 common / 2 total (100% overlap)"));
        assert!(rendered.contains("Rows:      3 old / 2 new (1 removed, 0 added, 2 overlap)"));
        assert!(rendered.contains("Types:     1 numeric columns, 0 type shifts"));
    }

    #[test]
    fn compatible_with_key_matches_exact_layout_snapshot() {
        let rendered = render_compatible(
            "old.csv",
            "new.csv",
            Dialect::default(),
            Dialect::default(),
            &base_suite(),
        );

        let expected = concat!(
            "SHAPE\n\n",
            "COMPATIBLE\n\n",
            "Compared: old.csv -> new.csv\n",
            "Key: loan_id (unique in both files)\n",
            "Dialect(old): delimiter=, quote=\" escape=none\n",
            "Dialect(new): delimiter=, quote=\" escape=none\n\n",
            "Schema:    2 common / 2 total (100% overlap)\n",
            "Key:       loan_id — unique in both, coverage=1.0\n",
            "Rows:      3 old / 2 new (1 removed, 0 added, 2 overlap)\n",
            "Types:     1 numeric columns, 0 type shifts\n",
        );

        assert_eq!(rendered, expected);
    }

    #[test]
    fn compatible_without_key_matches_exact_layout_snapshot() {
        let mut suite = base_suite();
        suite.key_viability = None;
        suite.row_granularity.key_overlap = None;
        suite.row_granularity.keys_old_only = None;
        suite.row_granularity.keys_new_only = None;

        let rendered = render_compatible(
            "old.csv",
            "new.csv",
            Dialect::default(),
            Dialect::default(),
            &suite,
        );

        let expected = concat!(
            "SHAPE\n\n",
            "COMPATIBLE\n\n",
            "Compared: old.csv -> new.csv\n",
            "Dialect(old): delimiter=, quote=\" escape=none\n",
            "Dialect(new): delimiter=, quote=\" escape=none\n\n",
            "Schema:    2 common / 2 total (100% overlap)\n",
            "Rows:      3 old / 2 new\n",
            "Types:     1 numeric columns, 0 type shifts\n",
        );

        assert_eq!(rendered, expected);
    }

    #[test]
    fn incompatible_renderer_includes_reasons_block_when_provided() {
        let mut suite = base_suite();
        suite.type_consistency.status = CheckStatus::Fail;
        suite.type_consistency.type_shifts = vec![crate::checks::type_consistency::TypeShift {
            column: b"balance".to_vec(),
            old_type: ColumnClassification::Numeric,
            new_type: ColumnClassification::NonNumeric,
        }];
        let reasons = vec!["Type shift: balance changed from numeric to non-numeric".to_string()];

        let rendered = render_incompatible(
            "old.csv",
            "new.csv",
            Dialect::default(),
            Dialect::default(),
            &suite,
            &reasons,
        );

        assert!(rendered.contains("INCOMPATIBLE"));
        assert!(rendered.contains("balance: numeric -> non-numeric"));
        assert!(
            rendered
                .contains("Reasons:\n  1. Type shift: balance changed from numeric to non-numeric")
        );
    }

    #[test]
    fn key_detail_line_mentions_both_files_when_key_missing_in_both() {
        let mut suite = base_suite();
        suite.key_viability = Some(KeyViabilityResult {
            status: CheckStatus::Fail,
            key_column: b"loan_id".to_vec(),
            found_old: false,
            found_new: false,
            unique_old: None,
            unique_new: None,
            duplicate_values_old: None,
            duplicate_values_new: None,
            empty_values_old: None,
            empty_values_new: None,
            coverage: None,
        });

        let rendered = render_incompatible(
            "old.csv",
            "new.csv",
            Dialect::default(),
            Dialect::default(),
            &suite,
            &["Key viability: loan_id not found in old file".to_string()],
        );

        assert!(rendered.contains("Key: loan_id (NOT FOUND in old and new files)"));
        assert!(rendered.contains("Key:       loan_id — not found in old and new files"));
    }

    #[test]
    fn key_detail_line_includes_duplicate_and_empty_counts_when_not_viable() {
        let mut suite = base_suite();
        suite.key_viability = Some(KeyViabilityResult {
            status: CheckStatus::Fail,
            key_column: b"loan_id".to_vec(),
            found_old: true,
            found_new: true,
            unique_old: Some(false),
            unique_new: Some(false),
            duplicate_values_old: Some(42),
            duplicate_values_new: Some(0),
            empty_values_old: Some(0),
            empty_values_new: Some(3),
            coverage: Some(0.95),
        });

        let rendered = render_incompatible(
            "old.csv",
            "new.csv",
            Dialect::default(),
            Dialect::default(),
            &suite,
            &[
                "Key viability: loan_id has 42 duplicate values in old file".to_string(),
                "Key viability: loan_id has 3 empty values in new file".to_string(),
            ],
        );

        assert!(
            rendered.contains(
                "Key: loan_id (NOT VIABLE — 42 duplicates in old, 3 empty values in new)"
            )
        );
        assert!(rendered.contains(
            "Key:       loan_id — 42 duplicates in old, 3 empty values in new, coverage=0.95"
        ));
    }

    #[test]
    fn incompatible_type_shift_matches_exact_layout_snapshot() {
        let mut suite = base_suite();
        suite.schema_overlap.columns_old_only = vec![b"retired_field".to_vec()];
        suite.schema_overlap.columns_new_only = vec![b"new_field".to_vec()];
        suite.schema_overlap.overlap_ratio = 0.5;
        suite.row_granularity.rows_old = 4_183;
        suite.row_granularity.rows_new = 4_201;
        suite.row_granularity.key_overlap = Some(4_150);
        suite.row_granularity.keys_old_only = Some(33);
        suite.row_granularity.keys_new_only = Some(51);
        suite.type_consistency.status = CheckStatus::Fail;
        suite.type_consistency.numeric_columns = 12;
        suite.type_consistency.type_shifts = vec![crate::checks::type_consistency::TypeShift {
            column: b"balance".to_vec(),
            old_type: ColumnClassification::Numeric,
            new_type: ColumnClassification::NonNumeric,
        }];
        if let Some(key) = suite.key_viability.as_mut() {
            key.coverage = Some(0.99);
        }
        let reasons = vec!["Type shift: balance changed from numeric to non-numeric".to_string()];

        let rendered = render_incompatible(
            "nov.csv",
            "dec.csv",
            Dialect::default(),
            Dialect::default(),
            &suite,
            &reasons,
        );

        let expected = concat!(
            "SHAPE\n\n",
            "INCOMPATIBLE\n\n",
            "Compared: nov.csv -> dec.csv\n",
            "Key: loan_id (unique in both files)\n",
            "Dialect(old): delimiter=, quote=\" escape=none\n",
            "Dialect(new): delimiter=, quote=\" escape=none\n\n",
            "Schema:    2 common / 4 total (50% overlap)\n",
            "           old_only: [retired_field]\n",
            "           new_only: [new_field]\n",
            "Key:       loan_id — unique in both, coverage=0.99\n",
            "Rows:      4,183 old / 4,201 new (33 removed, 51 added, 4,150 overlap)\n",
            "Types:     12 numeric columns, 1 type shift\n",
            "           balance: numeric -> non-numeric\n",
            "\n",
            "Reasons:\n",
            "  1. Type shift: balance changed from numeric to non-numeric\n",
        );

        assert_eq!(rendered, expected);
    }

    #[test]
    fn refusal_empty_matches_exact_layout_snapshot() {
        let refusal = RefusalPayload::empty("new.csv", 0);
        let context = RefusalRenderContext {
            old_path: "old.csv",
            new_path: "new.csv",
            dialect_old: Some(Dialect::default()),
            dialect_new: Some(Dialect::default()),
        };

        let rendered = render_refusal_with_context(&refusal, &context);

        let expected = concat!(
            "SHAPE ERROR (E_EMPTY)\n\n",
            "Compared: old.csv -> new.csv\n",
            "Dialect(old): delimiter=, quote=\" escape=none\n",
            "Dialect(new): delimiter=, quote=\" escape=none\n",
            "\n",
            "One or both files empty (no data rows after header)\n",
            "Next: provide non-empty datasets.\n",
        );

        assert_eq!(rendered, expected);
    }

    #[test]
    fn refusal_dialect_matches_exact_layout_snapshot() {
        let refusal = RefusalPayload::dialect(
            "new.csv",
            vec!["0x2c".to_string(), "0x09".to_string()],
            Some("shape old.csv new.csv --delimiter tab --json".to_string()),
        );
        let context = RefusalRenderContext {
            old_path: "old.csv",
            new_path: "new.csv",
            dialect_old: Some(Dialect::default()),
            dialect_new: None,
        };

        let rendered = render_refusal_with_context(&refusal, &context);

        let expected = concat!(
            "SHAPE ERROR (E_DIALECT)\n\n",
            "Compared: old.csv -> new.csv\n",
            "Dialect(old): delimiter=, quote=\" escape=none\n",
            "\n",
            "Delimiter ambiguous or undetectable\n",
            "Next: shape old.csv new.csv --delimiter tab --json\n",
        );

        assert_eq!(rendered, expected);
    }

    #[test]
    fn refusal_early_file_error_matches_exact_layout_snapshot() {
        let refusal = RefusalPayload::io("old.csv", "No such file");
        let context = RefusalRenderContext {
            old_path: "old.csv",
            new_path: "new.csv",
            dialect_old: None,
            dialect_new: None,
        };

        let rendered = render_refusal_with_context(&refusal, &context);

        let expected = concat!(
            "SHAPE ERROR (E_IO)\n\n",
            "Compared: old.csv -> new.csv\n",
            "\n",
            "Can't read file\n",
            "Next: check file path and permissions.\n",
        );

        assert_eq!(rendered, expected);
    }

    fn base_suite() -> CheckSuite {
        CheckSuite {
            schema_overlap: SchemaOverlapResult {
                status: CheckStatus::Pass,
                columns_common: vec![b"loan_id".to_vec(), b"balance".to_vec()],
                columns_old_only: vec![],
                columns_new_only: vec![],
                overlap_ratio: 1.0,
            },
            key_viability: Some(KeyViabilityResult {
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
            }),
            row_granularity: RowGranularityResult {
                status: CheckStatus::Pass,
                rows_old: 3,
                rows_new: 2,
                key_overlap: Some(2),
                keys_old_only: Some(1),
                keys_new_only: Some(0),
            },
            type_consistency: TypeConsistencyResult {
                status: CheckStatus::Pass,
                numeric_columns: 1,
                type_shifts: vec![],
            },
        }
    }
}
