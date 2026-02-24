mod helpers;

use helpers::{
    assert_parse_failure_routes_to_stderr, assert_parse_failure_with_fixtures_routes_to_stderr,
};

const BASIC_OLD: &str = "basic_old.csv";
const BASIC_NEW: &str = "basic_new.csv";
const JSON_FLAG: &str = "--json";
const INVALID_DELIMITER: &str = "bad";
const INVALID_MAX_ROWS: &str = "abc";
const USAGE_PREFIX: &str = "Usage: shape";

struct FixtureParseFailureCase {
    args: &'static [&'static str],
    expected_stderr_fragments: &'static [&'static str],
}

fn assert_fixture_parse_failure_cases(cases: &[FixtureParseFailureCase]) {
    for case in cases {
        assert_parse_failure_with_fixtures_routes_to_stderr(
            BASIC_OLD,
            BASIC_NEW,
            case.args,
            case.expected_stderr_fragments,
        );
    }
}

#[test]
fn json_mode_fixture_parse_failures_route_process_errors_to_stderr() {
    let cases = [
        FixtureParseFailureCase {
            args: &[JSON_FLAG, "--delimiter", INVALID_DELIMITER],
            expected_stderr_fragments: &[
                "invalid --delimiter value",
                "unsupported delimiter value: bad",
            ],
        },
        FixtureParseFailureCase {
            args: &[JSON_FLAG, "--max-rows", INVALID_MAX_ROWS],
            expected_stderr_fragments: &["--max-rows"],
        },
    ];

    assert_fixture_parse_failure_cases(&cases);
}

#[test]
fn json_mode_missing_positionals_routes_process_error_to_stderr() {
    assert_parse_failure_routes_to_stderr(&[JSON_FLAG], USAGE_PREFIX);
}
