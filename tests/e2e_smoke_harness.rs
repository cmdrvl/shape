mod helpers;

use helpers::{ShapeInvocation, fixture_path, run_shape_with_fixtures};
use serde_json::Value;

const BASIC_OLD: &str = "basic_old.csv";
const BASIC_NEW: &str = "basic_new.csv";
const TYPE_SHIFT_OLD: &str = "type_shift_old.csv";
const TYPE_SHIFT_NEW: &str = "type_shift_new.csv";
const NO_HEADER: &str = "no_header.csv";
const KEY_LOAN_ID: &str = "loan_id";
const OUTCOME_COMPATIBLE: &str = "COMPATIBLE";
const OUTCOME_INCOMPATIBLE: &str = "INCOMPATIBLE";
const OUTCOME_REFUSAL: &str = "REFUSAL";

#[derive(Clone, Copy)]
struct Scenario {
    old: &'static str,
    new: &'static str,
    key: Option<&'static str>,
    expected_status: i32,
    expected_outcome: &'static str,
    expect_partial_context: bool,
}

fn compared_line(old_fixture: &str, new_fixture: &str) -> String {
    format!(
        "Compared: {} -> {}",
        fixture_path(old_fixture).display(),
        fixture_path(new_fixture).display()
    )
}

fn assert_contains_compared_line(output: &str, scenario: &Scenario, context: &str) {
    assert!(
        output.contains(&compared_line(scenario.old, scenario.new)),
        "{context}: {output}"
    );
}

fn scenario_args(scenario: &Scenario, json_mode: bool) -> Vec<&str> {
    let mut args = Vec::new();
    if json_mode {
        args.push("--json");
    }
    if let Some(key) = scenario.key {
        args.extend(["--key", key]);
    }
    args
}

fn run_scenario(scenario: &Scenario, json_mode: bool) -> ShapeInvocation {
    let args = scenario_args(scenario, json_mode);
    run_shape_with_fixtures(scenario.old, scenario.new, &args)
}

fn run_human_scenario(scenario: &Scenario) -> ShapeInvocation {
    run_scenario(scenario, false)
}

fn run_json_scenario(scenario: &Scenario) -> ShapeInvocation {
    run_scenario(scenario, true)
}

fn assert_human_outcome(scenario: &Scenario, human: &ShapeInvocation) {
    assert_eq!(human.status, scenario.expected_status);

    if scenario.expected_status == 2 {
        assert!(
            human.stdout.trim().is_empty(),
            "human refusal should not write stdout: {}",
            human.stdout
        );
        assert!(
            human.stderr.contains("SHAPE ERROR"),
            "human refusal should render refusal header: {}",
            human.stderr
        );
        assert_contains_compared_line(
            &human.stderr,
            scenario,
            "human refusal should echo compared file paths",
        );
        if scenario.expect_partial_context {
            assert!(human.stderr.contains("Dialect(old):"));
            assert!(!human.stderr.contains("Dialect(new):"));
        }
    } else {
        assert!(
            human.stderr.trim().is_empty(),
            "human non-refusal should not write stderr: {}",
            human.stderr
        );
        assert!(
            human.stdout.contains(scenario.expected_outcome),
            "human output should contain expected outcome {}: {}",
            scenario.expected_outcome,
            human.stdout
        );
        assert_contains_compared_line(
            &human.stdout,
            scenario,
            "human output should echo compared file paths",
        );
    }
}

fn parse_json_output(json: &ShapeInvocation) -> Value {
    serde_json::from_str(json.stdout_trimmed())
        .expect("json mode should emit valid shape.v0 payload")
}

fn assert_json_outcome(scenario: &Scenario, json: &ShapeInvocation) {
    assert_eq!(json.status, scenario.expected_status);
    assert!(
        json.stderr.trim().is_empty(),
        "json mode should not write stderr: {}",
        json.stderr
    );

    let payload = parse_json_output(json);
    assert_eq!(payload["outcome"], scenario.expected_outcome);
    assert_eq!(
        payload["files"]["old"],
        fixture_path(scenario.old).display().to_string()
    );
    assert_eq!(
        payload["files"]["new"],
        fixture_path(scenario.new).display().to_string()
    );

    if scenario.expect_partial_context {
        assert!(payload["dialect"]["old"].is_object());
        assert!(payload["dialect"]["new"].is_null());
    }
}

#[test]
fn e2e_cli_matrix_runs_human_and_json_through_shared_harness() {
    let scenarios = [
        Scenario {
            old: BASIC_OLD,
            new: BASIC_NEW,
            key: Some(KEY_LOAN_ID),
            expected_status: 0,
            expected_outcome: OUTCOME_COMPATIBLE,
            expect_partial_context: false,
        },
        Scenario {
            old: TYPE_SHIFT_OLD,
            new: TYPE_SHIFT_NEW,
            key: Some(KEY_LOAN_ID),
            expected_status: 1,
            expected_outcome: OUTCOME_INCOMPATIBLE,
            expect_partial_context: false,
        },
        Scenario {
            old: BASIC_OLD,
            new: NO_HEADER,
            key: None,
            expected_status: 2,
            expected_outcome: OUTCOME_REFUSAL,
            expect_partial_context: true,
        },
    ];

    for scenario in scenarios {
        let human = run_human_scenario(&scenario);
        assert_human_outcome(&scenario, &human);

        let json = run_json_scenario(&scenario);
        assert_json_outcome(&scenario, &json);
    }
}

#[test]
fn e2e_old_parse_refusal_omits_both_dialect_contexts() {
    let scenario = Scenario {
        old: NO_HEADER,
        new: BASIC_NEW,
        key: None,
        expected_status: 2,
        expected_outcome: OUTCOME_REFUSAL,
        expect_partial_context: false,
    };

    let human = run_human_scenario(&scenario);
    assert_eq!(human.status, 2);
    assert!(human.stdout.trim().is_empty());
    assert!(human.stderr.contains("SHAPE ERROR"));
    assert_contains_compared_line(
        &human.stderr,
        &scenario,
        "human refusal should echo compared file paths",
    );
    assert!(!human.stderr.contains("Dialect(old):"));
    assert!(!human.stderr.contains("Dialect(new):"));

    let json = run_json_scenario(&scenario);
    assert_eq!(json.status, 2);
    assert!(json.stderr.trim().is_empty());
    let payload = parse_json_output(&json);
    assert_eq!(payload["outcome"], OUTCOME_REFUSAL);
    assert_eq!(payload["refusal"]["code"], "E_HEADERS");
    assert!(payload["dialect"]["old"].is_null());
    assert!(payload["dialect"]["new"].is_null());
}
