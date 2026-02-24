mod helpers;

use helpers::{
    DisplayCase, ParseFailureCase, assert_display_stdout_only_matrix,
    assert_parse_failure_routes_to_stderr_matrix,
};

const OLD_PATH: &str = "old.csv";
const NEW_PATH: &str = "new.csv";
const JSON_FLAG: &str = "--json";
const DELIMITER_FLAG: &str = "--delimiter";
const INVALID_DELIMITER: &str = "bad";
const INVALID_DELIMITER_FRAGMENT: &str = "invalid --delimiter value";
const MAX_ROWS_FLAG: &str = "--max-rows";
const INVALID_MAX_ROWS: &str = "abc";
const PROFILE_FLAG: &str = "--profile";
const PROFILE_PATH: &str = "profile.toml";
const PROFILE_ID_FLAG: &str = "--profile-id";
const PROFILE_ID: &str = "monthly";
const MISSING_POSITIONALS_FRAGMENT: &str = "<old.csv>";

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
fn display_flag_smoke_cases_stay_stdout_only() {
    let cases = [
        help_case(&["--help"]),
        help_case(&["-h", "--json"]),
        version_case(&["--version"]),
        version_case(&["-V", "--json"]),
    ];

    assert_display_stdout_only_matrix(&cases);
}

#[test]
fn json_process_level_parse_errors_keep_stdout_empty_and_write_stderr() {
    let cases = [
        ParseFailureCase {
            args: &[JSON_FLAG],
            expected_stderr_fragment: MISSING_POSITIONALS_FRAGMENT,
        },
        ParseFailureCase {
            args: &[
                OLD_PATH,
                NEW_PATH,
                JSON_FLAG,
                DELIMITER_FLAG,
                INVALID_DELIMITER,
            ],
            expected_stderr_fragment: INVALID_DELIMITER_FRAGMENT,
        },
        ParseFailureCase {
            args: &[
                OLD_PATH,
                NEW_PATH,
                JSON_FLAG,
                MAX_ROWS_FLAG,
                INVALID_MAX_ROWS,
            ],
            expected_stderr_fragment: MAX_ROWS_FLAG,
        },
        ParseFailureCase {
            args: &[
                OLD_PATH,
                NEW_PATH,
                JSON_FLAG,
                PROFILE_FLAG,
                PROFILE_PATH,
                PROFILE_ID_FLAG,
                PROFILE_ID,
            ],
            expected_stderr_fragment: PROFILE_ID_FLAG,
        },
    ];

    assert_parse_failure_routes_to_stderr_matrix(&cases);
}
