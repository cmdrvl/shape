use std::path::Path;

use serde_json::{Value, json};

use crate::cli::args::{DoctorAction, DoctorArgs, RobotDocsAction};

/// Schema id shared by every spine tool's doctor reports (spine-rules `cmdrvl.doctor.v1`).
const DOCTOR_SCHEMA_VERSION: &str = "cmdrvl.doctor.v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DoctorMode {
    Health,
    Capabilities,
    Triage,
}

impl DoctorMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Health => "health",
            Self::Capabilities => "capabilities",
            Self::Triage => "triage",
        }
    }
}

pub fn run(args: &DoctorArgs, json_output: bool) -> Result<u8, Box<dyn std::error::Error>> {
    if args.fix {
        return fix_unavailable();
    }

    let json = args.json || json_output;

    if args.robot_triage {
        return emit_report(DoctorMode::Triage);
    }

    match &args.action {
        Some(DoctorAction::Health(health_args)) => health(health_args.json || json),
        Some(DoctorAction::Capabilities(capabilities_args)) => {
            if capabilities_args.json || json {
                emit_report(DoctorMode::Capabilities)
            } else {
                emit_capabilities(false)
            }
        }
        Some(DoctorAction::Triage(triage_args)) => {
            if triage_args.json || json {
                emit_report(DoctorMode::Triage)
            } else {
                human_triage()
            }
        }
        Some(DoctorAction::RobotDocs) => emit_robot_docs(None),
        None if json => emit_report(DoctorMode::Health),
        None => human_triage(),
    }
}

fn health(json: bool) -> Result<u8, Box<dyn std::error::Error>> {
    if json {
        return emit_report(DoctorMode::Health);
    }
    let report = build_report(DoctorMode::Health);
    println!(
        "shape doctor {}: {} checks passed, {} findings",
        report.status(),
        report.checks_passed,
        report.error_count()
    );
    Ok(report.exit_code())
}

fn human_triage() -> Result<u8, Box<dyn std::error::Error>> {
    let report = build_report(DoctorMode::Triage);
    println!("SHAPE DOCTOR");
    println!();
    println!("Status: {}", report.status());
    println!("Checks passed: {}", report.checks_passed);
    println!("Findings: {}", report.error_count());
    if report.error_count() > 0 {
        println!();
        for check in report.checks.iter().filter(|check| !check.pass) {
            println!("- {}: {}", check.id, check.message);
            println!("  next: {}", check.fix);
        }
    }
    println!();
    println!("Next: shape capabilities --json");
    Ok(report.exit_code())
}

/// `shape capabilities --json` is the operator.v1 contract, byte-identical to `--describe`.
pub fn emit_capabilities(json: bool) -> Result<u8, Box<dyn std::error::Error>> {
    if json {
        // `--describe` writes the manifest followed by a newline; stay identical.
        println!("{}", crate::OPERATOR_JSON);
    } else {
        let contract = operator_contract();
        println!("shape capabilities");
        println!(
            "schema_version: {}",
            contract["schema_version"].as_str().unwrap_or("?")
        );
        println!(
            "contract_version: {}",
            contract["contract_version"].as_str().unwrap_or("?")
        );
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
    println!("- shape capabilities --json");
    println!("- shape robot-docs guide");
    println!("- shape --json <old.csv> <new.csv>");
    println!("- shape doctor [--json]");
    println!("- shape doctor capabilities --json");
    println!("- shape doctor triage --json");
    println!("- shape doctor robot-docs");
    println!("- shape doctor --fix");
    println!();
    println!("Deprecated aliases (still work; same cmdrvl.doctor.v1 output):");
    println!("- shape --robot-triage  ->  shape doctor triage --json");
    println!("- shape doctor --robot-triage  ->  shape doctor triage --json");
    println!("- shape doctor health [--json]  ->  shape doctor [--json]");
    println!();
    println!("Exit codes:");
    println!("- 0: COMPATIBLE (doctor: healthy)");
    println!("- 1: INCOMPATIBLE, a halting verdict (doctor: unhealthy, an error finding)");
    println!("- 2: refusal or command-line usage error");
    println!();
    println!("Redaction (--explicit):");
    println!(
        "- Column names and schema detail are redacted by default: counts, ratios and statuses only (schema old_only/new_only lists are empty, type-shift columns show [REDACTED])."
    );
    println!(
        "- Pass `--explicit` to show column names and other identifying metadata, e.g. `shape <old.csv> <new.csv> --key <column> --json --explicit`."
    );
    println!(
        "- NOT yet redacted, with or without `--explicit`: file paths, the `--key` column you passed, and the free-text `reasons` strings (which can name columns). Known gap: bd-pitd. Do not treat default output as zero-retention safe until it is fixed."
    );
    println!(
        "- Use `--explicit` for local non-sensitive runs; prefer the default for logged, shared, or veil/airlock workflows, keeping the gap above in mind."
    );
    println!();
    println!("Composition:");
    println!(
        "- Run `shape <old.csv> <new.csv> --key <column> --json` before `rvl` when comparing two tabular datasets; keyed compatibility requires positive overlap."
    );
    println!(
        "- Continue to `rvl <old.csv> <new.csv> --key <column> --key-mode overlap --json` only when shape reports COMPATIBLE; zero-overlap pairs are INCOMPATIBLE and must stop."
    );
    println!(
        "- Feed shape reports to `assess` with the matching rvl/verify/benchmark artifacts, or seal them with `pack`."
    );
    println!();
    println!(
        "Repair policy: shape doctor --fix is unavailable and exits 2 without stdout. Use shape doctor triage --json or shape capabilities --json for read-only diagnostics."
    );
    Ok(0)
}

/// Deprecated `shape --robot-triage` alias: the triage-mode cmdrvl.doctor.v1 report.
pub fn emit_robot_triage() -> Result<u8, Box<dyn std::error::Error>> {
    emit_report(DoctorMode::Triage)
}

fn emit_report(mode: DoctorMode) -> Result<u8, Box<dyn std::error::Error>> {
    let report = build_report(mode);
    println!("{}", serde_json::to_string_pretty(&report.to_json())?);
    Ok(report.exit_code())
}

fn fix_unavailable() -> Result<u8, Box<dyn std::error::Error>> {
    use std::io::Write;

    let mut stderr = std::io::stderr();
    writeln!(
        stderr,
        "shape doctor --fix is unavailable: diagnostics are read-only in this release."
    )?;
    writeln!(stderr, "Try triage: shape doctor triage --json")?;
    writeln!(stderr, "Try capabilities --json: shape capabilities --json")?;
    writeln!(stderr, "Try robot-docs guide: shape robot-docs guide")?;
    stderr.flush()?;
    Ok(2)
}

fn operator_contract() -> Value {
    serde_json::from_str(crate::OPERATOR_JSON).unwrap_or(Value::Null)
}

struct Check {
    id: &'static str,
    pass: bool,
    message: String,
    fix: &'static str,
}

struct Report {
    mode: DoctorMode,
    checks: Vec<Check>,
    checks_passed: usize,
}

impl Report {
    fn error_count(&self) -> usize {
        self.checks.iter().filter(|check| !check.pass).count()
    }

    fn healthy(&self) -> bool {
        self.error_count() == 0
    }

    fn status(&self) -> &'static str {
        if self.healthy() {
            "healthy"
        } else {
            "unhealthy"
        }
    }

    /// Doctor contract: 0 healthy, 1 unhealthy (an error finding), never 2.
    fn exit_code(&self) -> u8 {
        if self.healthy() { 0 } else { 1 }
    }

    fn to_json(&self) -> Value {
        let findings: Vec<Value> = self
            .checks
            .iter()
            .map(|check| {
                let mut finding = json!({
                    "id": check.id,
                    "severity": if check.pass { "info" } else { "error" },
                    "message": check.message,
                });
                if !check.pass {
                    finding["fix"] = json!(check.fix);
                }
                finding
            })
            .collect();

        let mut report = json!({
            "schema_version": DOCTOR_SCHEMA_VERSION,
            "mode": self.mode.as_str(),
            "tool": "shape",
            "tool_version": env!("CARGO_PKG_VERSION"),
            "healthy": self.healthy(),
            "findings": findings,
        });

        match self.mode {
            DoctorMode::Health => {}
            DoctorMode::Capabilities => {
                report["contract"] = operator_contract();
            }
            DoctorMode::Triage => {
                let mut next_actions: Vec<String> = self
                    .checks
                    .iter()
                    .filter(|check| !check.pass)
                    .map(|check| check.fix.to_string())
                    .collect();
                if next_actions.is_empty() {
                    next_actions.push("shape capabilities --json".to_string());
                    next_actions
                        .push("shape <old.csv> <new.csv> --key <column> --json (requires positive key overlap)".to_string());
                } else {
                    next_actions.push("shape doctor capabilities --json".to_string());
                }
                report["next_actions"] = json!(next_actions);
            }
        }
        report
    }
}

fn build_report(mode: DoctorMode) -> Report {
    let mut checks = vec![
        Check {
            id: "binary-metadata",
            pass: true,
            message: format!("shape {} is runnable", env!("CARGO_PKG_VERSION")),
            fix: "reinstall shape",
        },
        Check {
            id: "operator-manifest",
            pass: operator_manifest_ok(),
            message: "compiled operator manifest is readable".to_string(),
            fix: "rebuild shape with a valid operator.json",
        },
    ];

    if let Some(check) = source_checkout_gitignore_check() {
        checks.push(check);
    }

    let checks_passed = checks.iter().filter(|check| check.pass).count();
    Report {
        mode,
        checks,
        checks_passed,
    }
}

fn operator_manifest_ok() -> bool {
    operator_contract().get("name").and_then(Value::as_str) == Some("shape")
}

fn source_checkout_gitignore_check() -> Option<Check> {
    let cwd = std::env::current_dir().ok()?;
    if !looks_like_shape_source_checkout(&cwd) {
        return None;
    }

    let gitignore = cwd.join(".gitignore");
    let pass = match std::fs::read_to_string(&gitignore) {
        Ok(contents) => contents.lines().any(|line| line.trim() == ".doctor/"),
        Err(_) => false,
    };

    Some(Check {
        id: "source-gitignore-doctor",
        pass,
        message: ".doctor/ is ignored in this shape checkout".to_string(),
        fix: "add .doctor/ to .gitignore",
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
