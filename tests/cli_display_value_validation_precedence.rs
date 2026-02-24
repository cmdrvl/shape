mod helpers;

use helpers::{DisplayCase, assert_display_stdout_only_matrix};

const INVALID_MAX_BYTES: &str = "oops";
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
fn display_flags_precede_value_and_runtime_validation_errors() {
    let cases = [
        help_case(&["-h", "--max-bytes", INVALID_MAX_BYTES]),
        version_case(&["--version", "--json", "--max-bytes", INVALID_MAX_BYTES]),
    ];

    assert_display_stdout_only_matrix(&cases);
}
