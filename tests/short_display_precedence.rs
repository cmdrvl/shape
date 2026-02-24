mod helpers;

use helpers::{DisplayCase, assert_display_stdout_only_matrix};

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
fn short_display_flags_precede_unknown_flag_validation_with_json() {
    let cases = [
        help_case(&["-h", "--json", "--bogus-flag"]),
        version_case(&["-V", "--json", "--bogus-flag"]),
    ];

    assert_display_stdout_only_matrix(&cases);
}
