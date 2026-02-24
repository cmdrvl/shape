mod helpers;

use helpers::{DisplayCase, assert_display_stdout_only_matrix};

const MISSING_OLD: &str = "missing-old.csv";
const MISSING_NEW: &str = "missing-new.csv";

fn help_case(args: &'static [&'static str]) -> DisplayCase<'static> {
    DisplayCase {
        args,
        expected_stdout: "Usage: shape",
    }
}

fn version_case(args: &'static [&'static str]) -> DisplayCase<'static> {
    DisplayCase {
        args,
        expected_stdout: "shape ",
    }
}

#[test]
fn display_flags_precede_positional_and_unknown_flag_validation() {
    let cases = [
        version_case(&[MISSING_OLD, MISSING_NEW, "-V"]),
        help_case(&["--help", "--bogus-flag"]),
    ];

    assert_display_stdout_only_matrix(&cases);
}
