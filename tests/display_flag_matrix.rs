mod helpers;

use helpers::{DisplayCase, assert_display_stdout_only_matrix};

const MISSING_OLD: &str = "missing-old.csv";
const MISSING_NEW: &str = "missing-new.csv";
const HELP_PREFIX: &str = "Usage: shape";
const VERSION_PREFIX: &str = "shape ";

fn help_case(args: &'static [&'static str]) -> DisplayCase<'static> {
    DisplayCase {
        args,
        expected_stdout: HELP_PREFIX,
    }
}

fn version_case(args: &'static [&'static str]) -> DisplayCase<'static> {
    DisplayCase {
        args,
        expected_stdout: VERSION_PREFIX,
    }
}

#[test]
fn display_flags_short_circuit_before_file_io_with_missing_paths() {
    let cases = [
        help_case(&[MISSING_OLD, MISSING_NEW, "--help"]),
        help_case(&[MISSING_OLD, MISSING_NEW, "--help", "--json"]),
        help_case(&[MISSING_OLD, MISSING_NEW, "-h"]),
        help_case(&[MISSING_OLD, MISSING_NEW, "-h", "--json"]),
        version_case(&[MISSING_OLD, MISSING_NEW, "--version"]),
        version_case(&[MISSING_OLD, MISSING_NEW, "--version", "--json"]),
        version_case(&[MISSING_OLD, MISSING_NEW, "-V"]),
        version_case(&[MISSING_OLD, MISSING_NEW, "-V", "--json"]),
    ];

    assert_display_stdout_only_matrix(&cases);
}
