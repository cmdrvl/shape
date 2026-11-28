//! JSONL, Parquet and XLSX/XLS inputs via the shared `cmdrvl-tabular` reader.
//!
//! shape's four checks are built around CSV bytes (dialect handling, record widths, type
//! inference, keys). A non-CSV input is read through tabular and re-rendered as canonical CSV
//! bytes IN MEMORY (comma, `\n` line ends, quotes only where needed), then handed to the
//! unchanged CSV path. Consequences:
//!
//! - the same data in any container gives identical bytes after this step, so verdicts match
//!   across containers by construction (golden tests in `tests/formats.rs`);
//! - `.csv`, `.tsv`, `.txt` and extensionless files never come through here, so existing
//!   output cannot change (TSV keeps shape's own dialect detection);
//! - nothing is written to disk, and refusals carry coordinates, sheet names and type names,
//!   never cell values.

#[cfg(feature = "cli")]
use std::path::Path;

#[cfg(all(feature = "cli", feature = "tabular"))]
use cmdrvl_tabular::{Format, RefusalCode as TabularCode, open};
#[cfg(feature = "tabular")]
use cmdrvl_tabular::{OpenOptions, SheetSel, open_bytes};

#[cfg(any(feature = "cli", feature = "tabular"))]
use crate::refusal::payload::RefusalPayload;

/// Container formats that go through tabular, by extension.
pub fn is_tabular_name(name: &str) -> bool {
    name.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .is_some_and(|extension| {
            matches!(
                extension.as_str(),
                "jsonl" | "ndjson" | "parquet" | "xlsx" | "xlsm" | "xls"
            )
        })
}

/// Container formats that go through tabular, by extension.
#[cfg(feature = "cli")]
pub fn is_tabular_path(path: &Path) -> bool {
    #[cfg(feature = "tabular")]
    {
        path.to_str().is_some_and(is_tabular_name)
    }
    #[cfg(not(feature = "tabular"))]
    {
        let _ = path;
        false
    }
}

/// Read `path` through tabular and render it as CSV bytes (comma delimiter).
#[cfg(all(feature = "cli", feature = "tabular"))]
pub fn render_csv(path: &Path, sheet: Option<&str>) -> Result<Vec<u8>, RefusalPayload> {
    let shown = path.to_string_lossy().into_owned();
    let to_refusal = |refusal: cmdrvl_tabular::Refusal| -> RefusalPayload {
        if refusal.code == TabularCode::EIo {
            return RefusalPayload::io(shown.clone(), refusal.detail);
        }
        let context = serde_json::to_value(&refusal.context)
            .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
        let sheet_problem = matches!(
            refusal.code,
            TabularCode::EAmbiguousSheet | TabularCode::ESheetNotFound
        );
        let payload = RefusalPayload::input_format(
            shown.clone(),
            refusal.code.as_str(),
            refusal.detail,
            context,
        );
        if sheet_problem {
            payload.with_next_command(format!(
                "shape <old> <new> --sheet <name|index> (or --old-sheet / --new-sheet) to pick the worksheet, then rerun; input: {shown}"
            ))
        } else {
            payload
        }
    };
    let render_failure =
        || RefusalPayload::io(shown.clone(), "could not render the input as CSV in memory");

    let opts = OpenOptions {
        sheet: sheet.map(SheetSel::parse),
        ..OpenOptions::default()
    };
    let mut table = open(path, &opts).map_err(to_refusal)?;
    debug_assert!(matches!(
        table.format(),
        Format::Jsonl | Format::Parquet | Format::Xlsx
    ));

    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Vec::new());
    writer
        .write_record(table.headers())
        .map_err(|_| render_failure())?;
    for row in table.rows() {
        let row = row.map_err(to_refusal)?;
        writer
            .write_record(row.cells.iter().map(|cell| cell.as_bytes()))
            .map_err(|_| render_failure())?;
    }
    writer.into_inner().map_err(|_| render_failure())
}

#[cfg(all(feature = "cli", not(feature = "tabular")))]
pub fn render_csv(path: &Path, _sheet: Option<&str>) -> Result<Vec<u8>, RefusalPayload> {
    Err(RefusalPayload::input_format(
        path.to_string_lossy(),
        "E_FORMAT",
        "tabular input support is disabled",
        serde_json::json!({ "feature": "tabular" }),
    ))
}

/// Read an in-memory tabular container through the shared reader and render its cells as the
/// canonical CSV bytes consumed by shape's existing checks. The source bytes stay in memory;
/// no path, temporary file, environment variable, clock or witness ledger is consulted.
#[cfg(feature = "tabular")]
pub fn render_bytes_csv(
    bytes: &[u8],
    name_hint: Option<&str>,
    sheet: Option<&str>,
) -> Result<Vec<u8>, RefusalPayload> {
    let shown = name_hint.unwrap_or("<bytes>").to_owned();
    let to_refusal = |refusal: cmdrvl_tabular::Refusal| -> RefusalPayload {
        let context = serde_json::to_value(&refusal.context)
            .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
        RefusalPayload::input_format(
            shown.clone(),
            refusal.code.as_str(),
            refusal.detail,
            context,
        )
    };
    let render_failure = || {
        RefusalPayload::input_format(
            shown.clone(),
            "E_RENDER",
            "could not render the input as CSV in memory",
            serde_json::json!({}),
        )
    };
    let opts = OpenOptions {
        sheet: sheet.map(SheetSel::parse),
        ..OpenOptions::default()
    };
    let mut table = open_bytes(bytes.to_vec(), name_hint, &opts).map_err(to_refusal)?;
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Vec::new());
    writer
        .write_record(table.headers())
        .map_err(|_| render_failure())?;
    for row in table.rows() {
        let row = row.map_err(to_refusal)?;
        writer
            .write_record(row.cells.iter().map(|cell| cell.as_bytes()))
            .map_err(|_| render_failure())?;
    }
    writer.into_inner().map_err(|_| render_failure())
}

/// The bytes shape's CSV pipeline sees for `path`: the file itself for CSV-like inputs, the
/// tabular CSV rendering for JSONL / Parquet / XLSX.
#[cfg(all(feature = "cli", feature = "tabular"))]
pub fn effective_csv_bytes(path: &Path, sheet: Option<&str>) -> Result<Vec<u8>, RefusalPayload> {
    if is_tabular_path(path) {
        render_csv(path, sheet)
    } else {
        std::fs::read(path)
            .map_err(|error| RefusalPayload::io(path.to_string_lossy(), error.to_string()))
    }
}

#[cfg(all(feature = "cli", not(feature = "tabular")))]
pub fn effective_csv_bytes(path: &Path, _sheet: Option<&str>) -> Result<Vec<u8>, RefusalPayload> {
    std::fs::read(path)
        .map_err(|error| RefusalPayload::io(path.to_string_lossy(), error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_routing() {
        for yes in [
            "a.jsonl",
            "a.NDJSON",
            "a.parquet",
            "a.xlsx",
            "a.XLSM",
            "a.xls",
        ] {
            assert!(is_tabular_name(yes), "{yes}");
        }
        for no in ["a.csv", "a.tsv", "a.txt", "a", "a.csv.gz"] {
            assert!(!is_tabular_name(no), "{no}");
        }
    }
}
