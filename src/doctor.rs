use std::path::Path;

use serde::Serialize;
use serde_json::json;

use crate::cli::args::{DoctorAction, DoctorArgs, RobotDocsAction};

const DOCTOR_SCHEMA_VERSION: &str = "shape.doctor.v1";
const DOCTOR_CONTRACT_VERSION: &str = "cmdrvl.read_only_doctor.v1";

pub fn run(args: &DoctorArgs, json_output: bool) -> Result<u8, Box<dyn std::error::Error>> {
    if args.fix {
        return fix_unavailable();
    }

    if args.robot_triage {
        return emit_robot_triage();
    }

    match &args.action {
        Some(DoctorAction::Health(health_args)) => health(health_args.json || json_output),
        Some(DoctorAction::Capabilities(capabilities_args)) => {
            emit_capabilities(capabilities_args.json || json_output)
        }
        Some(DoctorAction::RobotDocs) => emit_robot_docs(None),
        None if json_output => emit_robot_triage(),
        None => human_triage(),
    }
}

fn health(json: bool) -> Result<u8, Box<dyn std::error::Error>> {
    let report = build_report();
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "shape doctor {}: {} checks passed, {} findings",
            report.summary.status,
            report.summary.checks_passed,
            report.findings.len()
        );
    }
    Ok(report.exit_code)
}

fn human_triage() -> Result<u8, Box<dyn std::error::Error>> {
    let report = build_report();
    println!("SHAPE DOCTOR");
    println!();
    println!("Status: {}", report.summary.status);
    println!("Checks passed: {}", report.summary.checks_passed);
    println!("Findings: {}", report.findings.len());
    if !report.findings.is_empty() {
        println!();
        for finding in &report.findings {
            println!("- {}: {}", finding.id, finding.summary);
            println!("  next: {}", finding.next_step);
        }
    }
    println!();
    println!("Next: shape capabilities --json");
    Ok(report.exit_code)
}

pub fn emit_capabilities(json: bool) -> Result<u8, Box<dyn std::error::Error>> {
    let payload = build_capabilities();
    if json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("shape capabilities");
        println!("schema_version: {}", payload.schema_version);
        println!("contract_version: {}", payload.contract_version);
        println!("read_only: {}", payload.read_only);
        println!("json: shape capabilities --json");
    }
    Ok(0)
}

pub fn emit_robot_docs(action: Option<&RobotDocsAction>) -> Result<u8, Box<dyn std::error::Error>> {
    match action {
        Some(RobotDocsAction::Guide) | None => {}
    }

    println!("# shape robot-docs guide");
    println!();
    println!(
        "shape's agent discovery surface is read-only. The discovery commands never repair files, delete files, run network probes, write witness ledgers, or create capsules."
    );
    println!();
    println!("Commands:");
    println!("- shape --robot-triage");
    println!("- shape capabilities --json");
    println!("- shape robot-docs guide");
    println!("- shape --json <old.csv> <new.csv>");
    println!("- shape doctor health");
    println!("- shape doctor health --json");
    println!("- shape doctor capabilities --json");
    println!("- shape doctor robot-docs");
    println!("- shape doctor --robot-triage");
    println!("- shape doctor --fix");
    println!();
    println!("Exit codes:");
    println!("- 0: healthy");
    println!("- 1: findings present");
    println!("- 2: command-line usage error from clap");
    println!();
    println!("Composition:");
    println!(
        "- Run `shape <old.csv> <new.csv> --key <column> --json` before `rvl` when comparing two tabular datasets."
    );
    println!(
        "- Continue to `rvl <old.csv> <new.csv> --key <column> --json` only when shape reports COMPATIBLE."
    );
    println!(
        "- Feed shape reports to `assess` with the matching rvl/verify/benchmark artifacts, or seal them with `pack`."
    );
    println!();
    println!(
        "Repair policy: shape doctor --fix is unavailable and exits 2 without stdout. Use shape --robot-triage or shape capabilities --json for read-only diagnostics."
    );
    Ok(0)
}

pub fn emit_robot_triage() -> Result<u8, Box<dyn std::error::Error>> {
    let report = build_report();
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(report.exit_code)
}

fn fix_unavailable() -> Result<u8, Box<dyn std::error::Error>> {
    use std::io::Write;

    let mut stderr = std::io::stderr();
    writeln!(
        stderr,
        "shape doctor --fix is unavailable: diagnostics are read-only in this release."
    )?;
    writeln!(stderr, "Try --robot-triage: shape --robot-triage")?;
    writeln!(stderr, "Try capabilities --json: shape capabilities --json")?;
    writeln!(stderr, "Try robot-docs guide: shape robot-docs guide")?;
    stderr.flush()?;
    Ok(2)
}

fn build_report() -> DoctorReport {
    let capabilities = build_capabilities();
    let mut checks = vec![
        Check {
            id: "binary-metadata",
            status: CheckStatus::Pass,
            summary: format!("shape {} is runnable", env!("CARGO_PKG_VERSION")),
        },
        Check {
            id: "operator-manifest",
            status: operator_manifest_status(),
            summary: "compiled operator manifest is readable".to_string(),
        },
    ];

    if let Some(check) = source_checkout_gitignore_check() {
        checks.push(check);
    }

    let findings: Vec<Finding> = checks
        .iter()
        .filter(|check| check.status == CheckStatus::Fail)
        .map(|check| Finding {
            id: check.id,
            severity: "warning",
            summary: check.summary.clone(),
            next_step: match check.id {
                "source-gitignore-doctor" => "add .doctor/ to .gitignore",
                "operator-manifest" => "rebuild shape with a valid operator.json",
                _ => "inspect shape capabilities --json",
            },
        })
        .collect();

    let status = if findings.is_empty() {
        "healthy"
    } else {
        "findings_present"
    };
    let exit_code = if findings.is_empty() { 0 } else { 1 };
    let checks_passed = checks
        .iter()
        .filter(|check| check.status == CheckStatus::Pass)
        .count();

    DoctorReport {
        schema_version: DOCTOR_SCHEMA_VERSION,
        tool: "shape",
        version: env!("CARGO_PKG_VERSION"),
        contract_version: DOCTOR_CONTRACT_VERSION,
        read_only: true,
        summary: Summary {
            status,
            checks_passed,
            checks_total: checks.len(),
            findings_count: findings.len(),
        },
        findings,
        checks,
        actions_planned: Vec::new(),
        recommended_command: if status == "healthy" {
            "shape capabilities --json"
        } else {
            "shape --robot-triage"
        },
        capabilities_url: "command:shape capabilities --json",
        capabilities,
        exit_code,
    }
}

fn operator_manifest_status() -> CheckStatus {
    match serde_json::from_str::<serde_json::Value>(crate::OPERATOR_JSON) {
        Ok(value) if value.get("name").and_then(|name| name.as_str()) == Some("shape") => {
            CheckStatus::Pass
        }
        _ => CheckStatus::Fail,
    }
}

fn source_checkout_gitignore_check() -> Option<Check> {
    let cwd = std::env::current_dir().ok()?;
    if !looks_like_shape_source_checkout(&cwd) {
        return None;
    }

    let gitignore = cwd.join(".gitignore");
    let status = match std::fs::read_to_string(&gitignore) {
        Ok(contents) if contents.lines().any(|line| line.trim() == ".doctor/") => CheckStatus::Pass,
        _ => CheckStatus::Fail,
    };

    Some(Check {
        id: "source-gitignore-doctor",
        status,
        summary: ".doctor/ is ignored in this shape checkout".to_string(),
    })
}

fn looks_like_shape_source_checkout(path: &Path) -> bool {
    let cargo_toml = path.join("Cargo.toml");
    let operator_json = path.join("operator.json");
    match std::fs::read_to_string(cargo_toml) {
        Ok(contents) => {
            contents
                .lines()
                .any(|line| line.trim() == r#"name = "shape""#)
                && operator_json.exists()
        }
        Err(_) => false,
    }
}

fn build_capabilities() -> DoctorCapabilities {
    DoctorCapabilities {
        schema_version: "shape.doctor.capabilities.v1",
        tool: "shape",
        version: env!("CARGO_PKG_VERSION"),
        contract_version: DOCTOR_CONTRACT_VERSION,
        read_only: true,
        online_default: false,
        fix_mode: FixModeCapability {
            available: false,
            command: "shape doctor --fix",
            behavior: "exits 2, emits only stderr, and names read-only alternatives",
        },
        agent_surfaces: json!({
            "global_json": {
                "command": "shape --json <old.csv> <new.csv>",
                "output": "shape.v0 JSON compare report",
                "stdout": "single JSON object",
                "stderr": "process-level failures only"
            },
            "robot_triage": {
                "command": "shape --robot-triage",
                "output": "shape.doctor.v1 JSON diagnostic report",
                "mutates": false
            },
            "capabilities": {
                "command": "shape capabilities --json",
                "output": "shape.doctor.capabilities.v1 JSON capability contract",
                "mutates": false
            },
            "robot_docs": {
                "command": "shape robot-docs guide",
                "output": "agent-oriented markdown guide",
                "mutates": false
            },
            "doctor_namespace": {
                "commands": [
                    "shape doctor health",
                    "shape doctor health --json",
                    "shape doctor capabilities --json",
                    "shape doctor robot-docs",
                    "shape doctor --robot-triage",
                    "shape doctor --fix"
                ],
                "status": "available"
            }
        }),
        shape_capabilities: json!({
            "formats": ["csv"],
            "csv_only": true,
            "compare_report": true,
            "schema_describe": true,
            "operator_describe": true,
            "witness_query": true,
            "repro_capsules": true,
            "profile_scoping": true,
            "streaming": false
        }),
        composition: json!({
            "family": {
                "name": "cmdrvl-spine",
                "siblings": ["rvl", "verify", "benchmark", "assess", "pack"],
                "capabilities": {
                    "shape": "shape capabilities --json",
                    "rvl": "rvl capabilities --json",
                    "assess": "assess capabilities --json",
                    "pack": "pack capabilities --json"
                }
            },
            "role": "structural_comparability_gate",
            "position": "before_numeric_diff",
            "accepts": [
                {
                    "kind": "csv_pair",
                    "description": "Two CSV datasets that may be compared after schema, key, granularity, and type checks."
                }
            ],
            "produces": [
                {
                    "kind": "shape_report",
                    "schema": "shape.v0",
                    "command": "shape <old.csv> <new.csv> --key <column> --json"
                }
            ],
            "canonical_chains": [
                {
                    "name": "gate_numeric_diff",
                    "commands": [
                        "shape <old.csv> <new.csv> --key <column> --json > shape.json",
                        "jq -e '.outcome == \"COMPATIBLE\"' shape.json >/dev/null",
                        "rvl <old.csv> <new.csv> --key <column> --json > rvl.json"
                    ],
                    "stop_conditions": ["REFUSAL", "INCOMPATIBLE"],
                    "downstream_tools": ["rvl", "assess", "pack"]
                }
            ],
            "agent_rules": [
                "Run shape before rvl whenever the task is to compare two tabular datasets.",
                "Do not run rvl on the pair when shape reports REFUSAL or INCOMPATIBLE.",
                "Use the same key/profile choices for shape and the downstream rvl run."
            ]
        }),
        side_effects: json!({
            "shape --robot-triage": {
                "reads_stdin": false,
                "reads_input_files": false,
                "parses_csv": false,
                "writes_witness_ledger": false,
                "writes_capsules": false,
                "writes_doctor_artifacts": false,
                "changes_cwd": false,
                "uses_network": false
            },
            "shape capabilities --json": {
                "reads_stdin": false,
                "reads_input_files": false,
                "parses_csv": false,
                "writes_witness_ledger": false,
                "writes_capsules": false,
                "writes_doctor_artifacts": false,
                "changes_cwd": false,
                "uses_network": false
            },
            "shape robot-docs guide": {
                "reads_stdin": false,
                "reads_input_files": false,
                "parses_csv": false,
                "writes_witness_ledger": false,
                "writes_capsules": false,
                "writes_doctor_artifacts": false,
                "changes_cwd": false,
                "uses_network": false
            },
            "shape doctor --fix": {
                "reads_stdin": false,
                "reads_input_files": false,
                "parses_csv": false,
                "writes_witness_ledger": false,
                "writes_capsules": false,
                "writes_doctor_artifacts": false,
                "changes_cwd": false,
                "uses_network": false,
                "available": false
            }
        }),
        commands: vec![
            CommandCapability {
                command: "shape --robot-triage",
                output: "json",
                mutates: false,
            },
            CommandCapability {
                command: "shape capabilities --json",
                output: "json",
                mutates: false,
            },
            CommandCapability {
                command: "shape robot-docs guide",
                output: "markdown",
                mutates: false,
            },
            CommandCapability {
                command: "shape --json <old.csv> <new.csv>",
                output: "shape.v0 json",
                mutates: true,
            },
            CommandCapability {
                command: "shape doctor health",
                output: "one-line text",
                mutates: false,
            },
            CommandCapability {
                command: "shape doctor health --json",
                output: "json",
                mutates: false,
            },
            CommandCapability {
                command: "shape doctor capabilities --json",
                output: "json",
                mutates: false,
            },
            CommandCapability {
                command: "shape doctor robot-docs",
                output: "markdown",
                mutates: false,
            },
            CommandCapability {
                command: "shape doctor --robot-triage",
                output: "json",
                mutates: false,
            },
            CommandCapability {
                command: "shape doctor --fix",
                output: "stderr refusal",
                mutates: false,
            },
        ],
        detectors: vec![
            DetectorCapability {
                id: "binary-metadata",
                description: "Confirms the shape binary can report its compiled version.",
                online_required: false,
            },
            DetectorCapability {
                id: "operator-manifest",
                description: "Confirms the compiled operator manifest is present and names shape.",
                online_required: false,
            },
            DetectorCapability {
                id: "source-gitignore-doctor",
                description: "When run from the shape source checkout, confirms .doctor/ is ignored.",
                online_required: false,
            },
        ],
        fixers: Vec::new(),
        exit_codes: vec![
            ExitCodeCapability {
                code: 0,
                meaning: "healthy or display command succeeded",
            },
            ExitCodeCapability {
                code: 1,
                meaning: "doctor findings present",
            },
            ExitCodeCapability {
                code: 2,
                meaning: "command-line usage error or shape refusal/error",
            },
        ],
        env_vars: vec![
            EnvVarCapability {
                name: "EPISTEMIC_WITNESS",
                description: "Overrides the witness ledger path for compare runs; doctor commands do not write it.",
            },
            EnvVarCapability {
                name: "HOME",
                description: "Used to resolve ~/.cmdrvl default state and config paths; doctor commands do not write them.",
            },
        ],
        data_paths: vec![
            DataPathCapability {
                path: ".doctor/",
                purpose: "reserved and gitignored for future doctor run artifacts",
                mutates_in_this_release: false,
            },
            DataPathCapability {
                path: "~/.cmdrvl/state/witness/witness.jsonl",
                purpose: "compare-run witness ledger; not touched by doctor commands",
                mutates_in_this_release: false,
            },
            DataPathCapability {
                path: "~/.cmdrvl/config/profile/profiles",
                purpose: "default --profile-id search path; not touched by doctor commands",
                mutates_in_this_release: false,
            },
            DataPathCapability {
                path: "--capsule-dir <path>",
                purpose: "compare-run repro capsule output; not touched by doctor commands",
                mutates_in_this_release: false,
            },
        ],
    }
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    schema_version: &'static str,
    tool: &'static str,
    version: &'static str,
    contract_version: &'static str,
    read_only: bool,
    summary: Summary,
    findings: Vec<Finding>,
    checks: Vec<Check>,
    actions_planned: Vec<String>,
    recommended_command: &'static str,
    capabilities_url: &'static str,
    capabilities: DoctorCapabilities,
    #[serde(skip)]
    exit_code: u8,
}

#[derive(Debug, Serialize)]
struct Summary {
    status: &'static str,
    checks_passed: usize,
    checks_total: usize,
    findings_count: usize,
}

#[derive(Debug, Serialize)]
struct Finding {
    id: &'static str,
    severity: &'static str,
    summary: String,
    next_step: &'static str,
}

#[derive(Debug, Serialize)]
struct Check {
    id: &'static str,
    status: CheckStatus,
    summary: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CheckStatus {
    Pass,
    Fail,
}

#[derive(Debug, Serialize)]
struct DoctorCapabilities {
    schema_version: &'static str,
    tool: &'static str,
    version: &'static str,
    contract_version: &'static str,
    read_only: bool,
    online_default: bool,
    fix_mode: FixModeCapability,
    agent_surfaces: serde_json::Value,
    shape_capabilities: serde_json::Value,
    composition: serde_json::Value,
    side_effects: serde_json::Value,
    commands: Vec<CommandCapability>,
    detectors: Vec<DetectorCapability>,
    fixers: Vec<String>,
    exit_codes: Vec<ExitCodeCapability>,
    env_vars: Vec<EnvVarCapability>,
    data_paths: Vec<DataPathCapability>,
}

#[derive(Debug, Serialize)]
struct FixModeCapability {
    available: bool,
    command: &'static str,
    behavior: &'static str,
}

#[derive(Debug, Serialize)]
struct CommandCapability {
    command: &'static str,
    output: &'static str,
    mutates: bool,
}

#[derive(Debug, Serialize)]
struct DetectorCapability {
    id: &'static str,
    description: &'static str,
    online_required: bool,
}

#[derive(Debug, Serialize)]
struct ExitCodeCapability {
    code: u8,
    meaning: &'static str,
}

#[derive(Debug, Serialize)]
struct EnvVarCapability {
    name: &'static str,
    description: &'static str,
}

#[derive(Debug, Serialize)]
struct DataPathCapability {
    path: &'static str,
    purpose: &'static str,
    mutates_in_this_release: bool,
}
