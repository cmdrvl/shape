mod helpers;

use helpers::{DisplayCase, assert_display_stdout_only_matrix};

const OLD_PATH: &str = "old.csv";
const NEW_PATH: &str = "new.csv";
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
fn short_display_flags_with_positionals_and_invalid_delimiter_still_display() {
    let cases = [
        help_case(&[OLD_PATH, NEW_PATH, "-h", "--json", "--delimiter", "bad"]),
        version_case(&[OLD_PATH, NEW_PATH, "--delimiter", "bad", "-V", "--json"]),
    ];

    assert_display_stdout_only_matrix(&cases);
}
