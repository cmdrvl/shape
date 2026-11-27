//! JSONL, Parquet and XLSX inputs through the shared tabular reader. The same loan datasets
//! exist as CSV, TSV, JSONL, Parquet and XLSX (independent writers, see
//! tests/fixtures/formats/make_format_fixtures.py). Every one of the 25 old x new pairings
//! must give the same verdict and the same checks as CSV vs CSV, for a COMPATIBLE pair
//! (old/new) and for an INCOMPATIBLE one (old/dup: the key is repeated in dup).

use serde_json::{Value, json};
use std::fs;
use std::process::{Command, Output};

const EXTS: [&str; 5] = ["csv", "tsv", "jsonl", "parquet", "xlsx"];

fn fx(name: &str) -> String {
    format!(
        "{}/tests/fixtures/formats/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn shape(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shape"))
        .args(args)
        .arg("--no-witness")
        .output()
        .expect("run shape")
}

fn report(old: &str, new: &str, extra: &[&str]) -> (Option<i32>, Value) {
    let mut args = vec![old, new, "--key", "loan_id", "--json"];
    args.extend_from_slice(extra);
    let out = shape(&args);
    let value = serde_json::from_slice::<Value>(&out.stdout);
    assert!(
        value.is_ok(),
        "stdout is not JSON: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let value = value.unwrap_or(Value::Null);
    (out.status.code(), value)
}

/// Drop what legitimately differs by container: the file paths, the dialect receipt (non-CSV
/// inputs are rendered with a comma, TSV is tab) and the verdict's human `reasons` wording is
/// kept (it must match too).
fn comparison(mut v: Value) -> Value {
    let obj = v.as_object_mut().unwrap();
    obj.remove("files");
    obj.remove("dialect");
    v
}

#[test]
fn all_25_pairings_give_the_csv_verdict_compatible_and_incompatible() {
    for (new_name, want_code, want_outcome) in
        [("new", 0, "COMPATIBLE"), ("dup", 1, "INCOMPATIBLE")]
    {
        let (code, baseline) = report(&fx("old.csv"), &fx(&format!("{new_name}.csv")), &[]);
        assert_eq!(code, Some(want_code), "{new_name}: {baseline}");
        assert_eq!(baseline["outcome"], want_outcome);
        let baseline = comparison(baseline);
        let mut checked = 0;
        for o in EXTS {
            for n in EXTS {
                let (code, got) = report(
                    &fx(&format!("old.{o}")),
                    &fx(&format!("{new_name}.{n}")),
                    &[],
                );
                assert_eq!(code, Some(want_code), "old.{o} vs {new_name}.{n}");
                assert_eq!(comparison(got), baseline, "old.{o} vs {new_name}.{n}");
                checked += 1;
            }
        }
        assert_eq!(checked, 25);
    }
}

#[test]
fn the_incompatible_verdict_names_the_repeated_key_in_every_format() {
    for ext in EXTS {
        let (_, got) = report(&fx("old.csv"), &fx(&format!("dup.{ext}")), &["--explicit"]);
        assert_eq!(got["checks"]["key_viability"]["status"], "fail", "{ext}");
        assert_eq!(got["checks"]["key_viability"]["unique_new"], false, "{ext}");
    }
}

// ---- worksheets ------------------------------------------------------------

#[test]
fn multi_sheet_workbook_without_a_sheet_refuses_and_names_the_sheets() {
    let (code, got) = report(&fx("old.csv"), &fx("new_two_sheets.xlsx"), &[]);
    assert_eq!(code, Some(2));
    assert_eq!(got["refusal"]["code"], "E_INPUT_FORMAT");
    let detail = &got["refusal"]["detail"];
    assert_eq!(detail["tabular_code"], "E_AMBIGUOUS_SHEET");
    assert_eq!(detail["context"]["sheets"], json!(["notes", "loans"]));
    assert!(
        got["refusal"]["next_command"]
            .as_str()
            .is_some_and(|next| next.contains("--sheet")),
        "{got}"
    );
}

#[test]
fn sheet_flags_select_by_name_index_and_per_side() {
    let baseline = comparison(report(&fx("old.csv"), &fx("new.csv"), &[]).1);
    for flags in [
        &["--sheet", "loans"][..],
        &["--new-sheet", "loans"][..],
        &["--sheet", "nope", "--new-sheet", "loans"][..], // per-side overrides --sheet
        &["--new-sheet", "1"][..],                        // 0-based index
    ] {
        let (code, got) = report(&fx("old.csv"), &fx("new_two_sheets.xlsx"), flags);
        assert_eq!(code, Some(0), "{flags:?}: {got}");
        assert_eq!(comparison(got), baseline, "{flags:?}");
    }
}

#[test]
fn unknown_sheet_refuses_with_the_listing() {
    let (code, got) = report(
        &fx("old.csv"),
        &fx("new_two_sheets.xlsx"),
        &["--new-sheet", "missing"],
    );
    assert_eq!(code, Some(2));
    assert_eq!(
        got["refusal"]["detail"]["tabular_code"],
        "E_SHEET_NOT_FOUND"
    );
    assert_eq!(
        got["refusal"]["detail"]["context"]["sheets"],
        json!(["notes", "loans"])
    );
}

// ---- refusals --------------------------------------------------------------

#[test]
fn unsupported_parquet_value_is_refused_without_the_value() {
    let (code, got) = report(&fx("old.csv"), &fx("binary.parquet"), &[]);
    assert_eq!(code, Some(2));
    assert_eq!(got["refusal"]["code"], "E_INPUT_FORMAT");
    assert_eq!(
        got["refusal"]["detail"]["tabular_code"],
        "E_UNSUPPORTED_VALUE"
    );
    assert!(!got.to_string().contains("secret"), "{got}");
}

#[test]
fn extension_that_disagrees_with_the_content_is_refused() {
    let (code, got) = report(&fx("old.csv"), &fx("not_really.parquet"), &[]);
    assert_eq!(code, Some(2));
    assert_eq!(got["refusal"]["detail"]["tabular_code"], "E_FORMAT");
}

#[test]
fn missing_non_csv_file_is_the_ordinary_io_refusal() {
    let (code, got) = report(&fx("old.csv"), &fx("does_not_exist.parquet"), &[]);
    assert_eq!(code, Some(2));
    assert_eq!(got["refusal"]["code"], "E_IO");
}

#[test]
fn csv_and_tsv_still_use_shapes_own_dialect_detection() {
    let (_, tsv) = report(&fx("old.tsv"), &fx("new.tsv"), &[]);
    assert_eq!(tsv["dialect"]["old"]["delimiter"], json!("\\t")); // shape shows a tab escaped
    for ext in ["jsonl", "parquet", "xlsx"] {
        let (_, report) = report(&fx(&format!("old.{ext}")), &fx(&format!("new.{ext}")), &[]);
        assert!(report["dialect"]["old"].is_null(), "{ext}: {report}");
        assert!(report["dialect"]["new"].is_null(), "{ext}: {report}");
    }
}

// ---- capsule and witness -----------------------------------------------------

#[test]
fn capsule_for_a_workbook_input_holds_the_csv_shape_saw_and_replays_without_a_sheet() {
    let dir = std::env::temp_dir().join(format!("shape-formats-capsule-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let capsule = dir.join("capsule");
    let capsule_arg = capsule.to_str().unwrap();
    let (code, got) = report(
        &fx("old.csv"),
        &fx("new_two_sheets.xlsx"),
        &["--new-sheet", "loans", "--capsule-dir", capsule_arg],
    );
    assert_eq!(code, Some(0), "{got}");

    // The captured new input is CSV text (not workbook bytes). Capsules hold a minimal repro
    // (header plus the rows the verdict needs), so only the header and a data row are checked.
    let captured = fs::read_to_string(capsule.join("inputs/new.csv")).expect("captured new input");
    assert!(
        captured.starts_with("loan_id,region,balance,rate\n"),
        "captured: {captured}"
    );
    assert!(
        captured.contains("L1,East,1000.5,0.05"),
        "captured: {captured}"
    );

    // Replay: the capsule's own inputs, no --sheet needed, same verdict.
    let replay = shape(&[
        capsule.join("inputs/old.csv").to_str().unwrap(),
        capsule.join("inputs/new.csv").to_str().unwrap(),
        "--key",
        "loan_id",
        "--json",
    ]);
    let replayed: Value = serde_json::from_slice(&replay.stdout).unwrap();
    assert_eq!(replay.status.code(), Some(0));
    assert_eq!(replayed["outcome"], got["outcome"]);
    let _ = fs::remove_dir_all(&dir);
}
