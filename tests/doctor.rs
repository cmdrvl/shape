mod helpers;

use helpers::run_shape;

#[test]
fn help_routes_exit_success() {
    for args in [
        &["--help"][..],
        &["witness", "--help"][..],
        &["doctor", "--help"][..],
        &["doctor", "health", "--help"][..],
        &["doctor", "capabilities", "--help"][..],
    ] {
        let result = run_shape(args);
        assert_eq!(result.status, 0, "help route should exit 0: {args:?}");
        assert!(
            result.stderr.trim().is_empty(),
            "help should not write stderr: {}",
            result.stderr
        );
        assert!(!result.stdout.trim().is_empty(), "help should print stdout");
    }
}

#[test]
fn doctor_health_is_read_only_and_successful() {
    let result = run_shape(["doctor", "health"]);

    assert_eq!(result.status, 0);
    assert!(result.stdout.contains("shape doctor healthy"));
    assert!(
        result.stderr.trim().is_empty(),
        "doctor health should not write stderr: {}",
        result.stderr
    );
}

#[test]
fn doctor_health_json_is_read_only_and_successful() {
    let result = run_shape(["doctor", "health", "--json"]);

    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "doctor health json should not write stderr: {}",
        result.stderr
    );
    let value: serde_json::Value =
        serde_json::from_str(&result.stdout).expect("health should be JSON");

    assert_eq!(value["schema_version"], "shape.doctor.v1");
    assert_eq!(value["tool"], "shape");
    assert_eq!(value["summary"]["status"], "healthy");
    assert_eq!(value["read_only"], true);
}

#[test]
fn doctor_capabilities_json_declares_read_only_contract() {
    let result = run_shape(["doctor", "capabilities", "--json"]);

    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "doctor capabilities should not write stderr: {}",
        result.stderr
    );
    let value: serde_json::Value =
        serde_json::from_str(&result.stdout).expect("capabilities should be JSON");

    assert_eq!(value["schema_version"], "shape.doctor.capabilities.v1");
    assert_eq!(value["tool"], "shape");
    assert_eq!(value["read_only"], true);
    assert_eq!(
        value["fixers"]
            .as_array()
            .expect("fixers should be an array")
            .len(),
        0
    );

    let commands = value["commands"]
        .as_array()
        .expect("commands should be an array");
    for expected in [
        "shape doctor health",
        "shape doctor health --json",
        "shape doctor capabilities --json",
        "shape doctor robot-docs",
        "shape doctor --robot-triage",
    ] {
        assert!(
            commands
                .iter()
                .any(|command| command["command"].as_str() == Some(expected)),
            "missing command capability {expected}"
        );
    }
}

#[test]
fn describe_includes_doctor_surface() {
    let result = run_shape(["--describe"]);

    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "describe should not write stderr: {}",
        result.stderr
    );
    let value: serde_json::Value =
        serde_json::from_str(&result.stdout).expect("describe should be JSON");
    let subcommands = value["subcommands"]
        .as_array()
        .expect("subcommands should be an array");
    let doctor = subcommands
        .iter()
        .find(|command| command["name"].as_str() == Some("doctor"))
        .expect("operator.json should describe doctor");

    assert_eq!(doctor["current_runtime_behavior"]["read_only"], true);
    assert_eq!(
        doctor["current_runtime_behavior"]["fix_mode"],
        "not_available"
    );
    assert_eq!(doctor["current_runtime_behavior"]["writes_witness"], false);
    assert_eq!(doctor["current_runtime_behavior"]["writes_capsules"], false);
}

#[test]
fn doctor_robot_docs_names_agent_surface() {
    let result = run_shape(["doctor", "robot-docs"]);

    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "robot-docs should not write stderr: {}",
        result.stderr
    );
    assert!(result.stdout.contains("shape doctor health"));
    assert!(result.stdout.contains("shape doctor health --json"));
    assert!(result.stdout.contains("shape doctor capabilities --json"));
    assert!(result.stdout.contains("no doctor --fix surface exists yet"));
}

#[test]
fn doctor_robot_triage_is_single_call_json() {
    let result = run_shape(["doctor", "--robot-triage"]);

    assert_eq!(result.status, 0);
    assert!(
        result.stderr.trim().is_empty(),
        "robot triage should not write stderr: {}",
        result.stderr
    );
    let value: serde_json::Value =
        serde_json::from_str(&result.stdout).expect("robot triage should be JSON");

    assert_eq!(value["schema_version"], "shape.doctor.v1");
    assert_eq!(value["summary"]["status"], "healthy");
    assert_eq!(value["read_only"], true);
    assert_eq!(value["actions_planned"].as_array().unwrap().len(), 0);
    assert_eq!(
        value["capabilities_url"],
        "command:shape doctor capabilities --json"
    );
}

#[test]
fn doctor_fix_surface_is_not_present() {
    let result = run_shape(["doctor", "--fix"]);

    assert_eq!(result.status, 2);
    assert!(
        result.stdout.trim().is_empty(),
        "usage errors should not emit stdout"
    );
    assert!(result.stderr.contains("unexpected argument '--fix'"));
}

#[test]
fn doctor_runtime_artifacts_are_gitignored() {
    let gitignore = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/.gitignore"))
        .expect(".gitignore should be readable");

    assert!(gitignore.lines().any(|line| line.trim() == ".doctor/"));
}
