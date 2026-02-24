mod helpers;

use helpers::{DisplayMatchCase, StdoutMatch, assert_display_stdout_only_with_match_matrix};

const OLD_PATH: &str = "old.csv";
const NEW_PATH: &str = "new.csv";
const INVALID_DELIMITER: &str = "bad";
const INVALID_MAX_ROWS: &str = "abc";
const INVALID_MAX_BYTES: &str = "oops";
const BOGUS_FLAG: &str = "--bogus-flag";
const PROFILE_PATH: &str = "a.toml";
const PROFILE_ID: &str = "monthly";
const HELP_PREFIX: &str = "Usage: shape";
const VERSION_PREFIX: &str = "shape ";

enum DisplayKind {
    Help,
    Version,
}

struct Case<'a> {
    args: &'a [&'a str],
    expected: DisplayKind,
}

fn help_case<'a>(args: &'a [&'a str]) -> Case<'a> {
    Case {
        args,
        expected: DisplayKind::Help,
    }
}

fn version_case<'a>(args: &'a [&'a str]) -> Case<'a> {
    Case {
        args,
        expected: DisplayKind::Version,
    }
}

#[test]
fn display_mode_matrix_is_stdout_only_and_exits_zero() {
    let cases = [
        // Baseline long/short forms (with/without --json).
        help_case(&["--help"]),
        help_case(&["--help", "--json"]),
        help_case(&["-h"]),
        help_case(&["-h", "--json"]),
        version_case(&["--version"]),
        version_case(&["--version", "--json"]),
        version_case(&["-V"]),
        version_case(&["-V", "--json"]),
        // Precedence over runtime delimiter validation.
        help_case(&["--help", "--delimiter", INVALID_DELIMITER]),
        version_case(&["--version", "--delimiter", INVALID_DELIMITER]),
        help_case(&["-h", "--delimiter", INVALID_DELIMITER]),
        version_case(&["-V", "--delimiter", INVALID_DELIMITER]),
        // Precedence when positionals are present with invalid delimiter.
        help_case(&[
            OLD_PATH,
            NEW_PATH,
            "--help",
            "--delimiter",
            INVALID_DELIMITER,
        ]),
        version_case(&[
            OLD_PATH,
            NEW_PATH,
            "--version",
            "--delimiter",
            INVALID_DELIMITER,
        ]),
        help_case(&[
            OLD_PATH,
            NEW_PATH,
            "-h",
            "--json",
            "--delimiter",
            INVALID_DELIMITER,
        ]),
        version_case(&[
            OLD_PATH,
            NEW_PATH,
            "--delimiter",
            INVALID_DELIMITER,
            "-V",
            "--json",
        ]),
        // Precedence over malformed typed values.
        help_case(&["--help", "--max-rows", INVALID_MAX_ROWS]),
        version_case(&["--version", "--max-bytes", INVALID_MAX_BYTES]),
        help_case(&["-h", "--max-bytes", INVALID_MAX_BYTES]),
        version_case(&["-V", "--max-rows", INVALID_MAX_ROWS]),
        // Precedence over parse-level unknown flags.
        help_case(&["--help", BOGUS_FLAG]),
        version_case(&["--version", BOGUS_FLAG]),
        help_case(&["-h", "--json", BOGUS_FLAG]),
        version_case(&["-V", "--json", BOGUS_FLAG]),
        // Precedence over argument conflict errors.
        help_case(&[
            "--help",
            "--profile",
            PROFILE_PATH,
            "--profile-id",
            PROFILE_ID,
        ]),
        version_case(&[
            "--version",
            "--profile",
            PROFILE_PATH,
            "--profile-id",
            PROFILE_ID,
        ]),
        // Ordered mixed display flags.
        help_case(&["--help", "--version"]),
        version_case(&["--version", "--help"]),
        help_case(&["-h", "-V"]),
        version_case(&["-V", "-h"]),
    ];

    let match_cases: Vec<DisplayMatchCase<'_>> = cases
        .iter()
        .map(|case| DisplayMatchCase {
            args: case.args,
            expected_stdout: match case.expected {
                DisplayKind::Help => StdoutMatch::Contains(HELP_PREFIX),
                DisplayKind::Version => StdoutMatch::StartsWith(VERSION_PREFIX),
            },
        })
        .collect();

    assert_display_stdout_only_with_match_matrix(&match_cases);
}
