mod helpers;

use helpers::{DisplayCase, assert_display_stdout_only_matrix};

const HELP_PREFIX: &str = "Usage: shape";
const VERSION_PREFIX: &str = "shape ";

fn help_case<'a>(args: &'a [&'a str]) -> DisplayCase<'a> {
    DisplayCase {
        args,
        expected_stdout: HELP_PREFIX,
    }
}

fn version_case<'a>(args: &'a [&'a str]) -> DisplayCase<'a> {
    DisplayCase {
        args,
        expected_stdout: VERSION_PREFIX,
    }
}

#[test]
fn mixed_display_flags_follow_argument_order_with_stdout_only_output() {
    let cases = [
        help_case(&["--help", "--version"]),
        version_case(&["--version", "--help"]),
        help_case(&["-h", "-V"]),
        version_case(&["-V", "-h"]),
    ];

    assert_display_stdout_only_matrix(&cases);
}
