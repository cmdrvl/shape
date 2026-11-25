use std::ffi::OsString;
use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

/// CLI arguments for shape.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "shape",
    version,
    about = "Structural comparability gate for CSV datasets",
    override_usage = "shape <old.csv> <new.csv> [OPTIONS]\n       shape --robot-triage\n       shape capabilities --json\n       shape robot-docs guide\n       shape witness <query|last|count> [OPTIONS]\n       shape doctor [health|capabilities|triage|robot-docs] [--json]",
    after_help = "Column names and schema detail are redacted by default: counts, ratios and statuses only.\nPass --explicit to show column names and other identifying metadata (schema old_only/new_only, type-shift columns).\nUse it for local, non-sensitive runs; prefer the default for logged or shared output.\nNOT yet redacted, with or without --explicit: file paths, the --key column you pass, and free-text `reasons` (known gap: bd-pitd). Do not treat default output as zero-retention safe yet.",
    subcommand_negates_reqs = true
)]
pub struct Args {
    #[arg(value_name = "old.csv", required_unless_present_any = ["describe", "schema", "robot_triage"])]
    pub old: Option<PathBuf>,

    #[arg(value_name = "new.csv", required_unless_present_any = ["describe", "schema", "robot_triage"])]
    pub new: Option<PathBuf>,

    #[arg(long, value_name = "column")]
    pub key: Option<String>,

    /// Require this inclusive key-overlap ratio when both inputs have rows.
    #[arg(long = "min-coverage", value_name = "ratio", value_parser = parse_min_coverage)]
    pub min_coverage: Option<f64>,

    #[arg(long, value_name = "delim")]
    pub delimiter: Option<String>,

    #[arg(long)]
    pub json: bool,

    #[arg(long)]
    pub no_witness: bool,

    #[arg(long = "capsule-dir", value_name = "path")]
    pub capsule_dir: Option<PathBuf>,

    #[arg(long, value_name = "path")]
    pub profile: Option<PathBuf>,

    #[arg(long = "profile-id", value_name = "id")]
    pub profile_id: Option<String>,

    #[arg(long, value_name = "lockfile", action = ArgAction::Append)]
    pub lock: Vec<PathBuf>,

    #[arg(long = "max-rows", value_name = "n")]
    pub max_rows: Option<u64>,

    #[arg(long = "max-bytes", value_name = "n")]
    pub max_bytes: Option<u64>,

    /// Show column names and other identifying metadata in output (default: column names redacted; free-text reasons, file paths and the --key column are not yet redacted, see bd-pitd).
    #[arg(long)]
    pub explicit: bool,

    /// Print JSON Schema for shape.v0 output format and exit 0.
    #[arg(long)]
    pub schema: bool,

    #[arg(long)]
    pub describe: bool,

    /// Emit one-call machine triage for headless agents.
    #[arg(long = "robot-triage")]
    pub robot_triage: bool,

    #[command(subcommand)]
    pub command: Option<ShapeCommand>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ShapeCommand {
    Witness {
        #[command(subcommand)]
        action: WitnessAction,
    },
    /// Print the machine-readable shape capability contract.
    Capabilities(TopLevelCapabilitiesArgs),
    /// Print paste-ready operating notes for agents.
    RobotDocs {
        #[command(subcommand)]
        action: Option<RobotDocsAction>,
    },
    /// Inspect shape's read-only diagnostic surface.
    Doctor(DoctorArgs),
}

#[derive(Debug, Clone, clap::Args)]
pub struct TopLevelCapabilitiesArgs {
    /// Emit JSON output.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum RobotDocsAction {
    /// Print the agent operating guide.
    Guide,
}

#[derive(Debug, Clone, clap::Args)]
pub struct DoctorArgs {
    /// Emit one-call machine triage for headless agents.
    #[arg(long = "robot-triage")]
    pub robot_triage: bool,

    /// Emit the doctor report as JSON (cmdrvl.doctor.v1, mode health).
    #[arg(long)]
    pub json: bool,

    /// Refuse safely; repair mode is not available in this release.
    #[arg(long, hide = true)]
    pub fix: bool,

    #[command(subcommand)]
    pub action: Option<DoctorAction>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum DoctorAction {
    /// Print a cheap one-line health summary.
    Health(DoctorHealthArgs),
    /// Print the doctor report carrying the operator contract (cmdrvl.doctor.v1, mode capabilities).
    Capabilities(DoctorCapabilitiesArgs),
    /// Print the doctor report with next actions (cmdrvl.doctor.v1, mode triage).
    Triage(DoctorTriageArgs),
    /// Print paste-ready operating notes for agents.
    RobotDocs,
}

#[derive(Debug, Clone, clap::Args)]
pub struct DoctorTriageArgs {
    /// Emit JSON output.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, clap::Args)]
pub struct DoctorHealthArgs {
    /// Emit JSON output.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, clap::Args)]
pub struct DoctorCapabilitiesArgs {
    /// Emit JSON output.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum WitnessAction {
    Query(WitnessQueryArgs),
    Last(WitnessLastArgs),
    Count(WitnessCountArgs),
}

#[derive(Debug, Clone, clap::Args)]
pub struct WitnessQueryArgs {
    #[arg(long)]
    pub tool: Option<String>,

    #[arg(long)]
    pub since: Option<String>,

    #[arg(long)]
    pub until: Option<String>,

    #[arg(long)]
    pub outcome: Option<String>,

    #[arg(long = "input-hash")]
    pub input_hash: Option<String>,

    #[arg(long, default_value_t = 20)]
    pub limit: usize,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, clap::Args)]
pub struct WitnessLastArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, clap::Args)]
pub struct WitnessCountArgs {
    #[arg(long)]
    pub tool: Option<String>,

    #[arg(long)]
    pub since: Option<String>,

    #[arg(long)]
    pub until: Option<String>,

    #[arg(long)]
    pub outcome: Option<String>,

    #[arg(long = "input-hash")]
    pub input_hash: Option<String>,

    #[arg(long)]
    pub json: bool,
}

impl Args {
    /// Parse CLI arguments using clap and return clap's rich error on failure.
    pub fn parse() -> Result<Self, clap::Error> {
        Self::parse_from(std::env::args_os())
    }

    /// Parse CLI arguments from an iterator.
    pub fn parse_from<I, T>(itr: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        use clap::{CommandFactory, FromArgMatches};

        let mut command = Self::command();
        if color_disabled_by_env(
            std::env::var_os("NO_COLOR").as_deref(),
            std::env::var_os("TERM").as_deref(),
        ) {
            // clap/anstream already strips ANSI for non-TTY and NO_COLOR, but a forced-color
            // variable (CLICOLOR_FORCE) would still win over TERM=dumb; make "never" explicit.
            command = command.color(clap::ColorChoice::Never);
        }
        let matches = command.try_get_matches_from(itr)?;
        Self::from_arg_matches(&matches)
    }
}

/// NO_COLOR (any non-empty value) or TERM=dumb means no ANSI styling.
pub(crate) fn color_disabled_by_env(
    no_color: Option<&std::ffi::OsStr>,
    term: Option<&std::ffi::OsStr>,
) -> bool {
    no_color.is_some_and(|value| !value.is_empty()) || term.is_some_and(|value| value == "dumb")
}

fn parse_min_coverage(value: &str) -> Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| "minimum coverage must be a number between 0.0 and 1.0".to_owned())?;
    if parsed.is_finite() && (0.0..=1.0).contains(&parsed) {
        Ok(parsed)
    } else {
        Err("minimum coverage must be a number between 0.0 and 1.0".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::{Args, DoctorAction, RobotDocsAction, ShapeCommand, WitnessAction};

    #[test]
    fn parse_describe_without_positionals() {
        let args = Args::parse_from(["shape", "--describe"]).expect("expected parse success");

        assert!(args.describe);
        assert!(args.old.is_none());
        assert!(args.new.is_none());
        assert!(args.command.is_none());
    }

    #[test]
    fn parse_schema_without_positionals() {
        let args = Args::parse_from(["shape", "--schema"]).expect("expected parse success");

        assert!(args.schema);
        assert!(args.old.is_none());
        assert!(args.new.is_none());
        assert!(args.command.is_none());
    }

    #[test]
    fn parse_robot_triage_without_positionals() {
        let args = Args::parse_from(["shape", "--robot-triage"])
            .expect("expected robot triage parse success");

        assert!(args.robot_triage);
        assert!(args.old.is_none());
        assert!(args.new.is_none());
        assert!(args.command.is_none());
    }

    #[test]
    fn parse_requires_positionals_without_describe() {
        let err = Args::parse_from(["shape"]).expect_err("expected parse failure");
        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn parse_version_without_positionals() {
        let err = Args::parse_from(["shape", "--version"]).expect_err("expected version display");
        assert_eq!(err.kind(), ErrorKind::DisplayVersion);
    }

    #[test]
    fn parse_accepts_ambiguous_profile_selectors_for_runtime_refusal() {
        let args = Args::parse_from([
            "shape",
            "old.csv",
            "new.csv",
            "--profile",
            "profile.toml",
            "--profile-id",
            "monthly",
        ])
        .expect("expected parse success");

        assert_eq!(
            args.profile.as_deref().and_then(|path| path.to_str()),
            Some("profile.toml")
        );
        assert_eq!(args.profile_id.as_deref(), Some("monthly"));
    }

    #[test]
    fn parse_rejects_malformed_max_rows() {
        let err = Args::parse_from(["shape", "old.csv", "new.csv", "--max-rows", "abc"])
            .expect_err("expected parse failure");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(err.to_string().contains("--max-rows"));
    }

    #[test]
    fn parse_rejects_min_coverage_outside_inclusive_unit_interval() {
        let err = Args::parse_from(["shape", "old.csv", "new.csv", "--min-coverage", "1.01"])
            .expect_err("expected minimum coverage validation failure");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(err.to_string().contains("--min-coverage"));
        assert!(err.to_string().contains("between 0.0 and 1.0"));
    }

    #[test]
    fn parse_accepts_core_and_deferred_flags() {
        let args = Args::parse_from([
            "shape",
            "old.csv",
            "new.csv",
            "--key",
            "loan_id",
            "--min-coverage",
            "0.75",
            "--delimiter",
            "comma",
            "--json",
            "--no-witness",
            "--capsule-dir",
            "capsules/run-001",
            "--profile-id",
            "monthly-profile",
            "--lock",
            "a.lock",
            "--lock",
            "b.lock",
            "--max-rows",
            "100",
            "--max-bytes",
            "2048",
        ])
        .expect("expected parse success");

        assert_eq!(
            args.old.as_deref().and_then(|p| p.to_str()),
            Some("old.csv")
        );
        assert_eq!(
            args.new.as_deref().and_then(|p| p.to_str()),
            Some("new.csv")
        );
        assert_eq!(args.key.as_deref(), Some("loan_id"));
        assert_eq!(args.min_coverage, Some(0.75));
        assert_eq!(args.delimiter.as_deref(), Some("comma"));
        assert!(args.json);
        assert!(args.no_witness);
        assert_eq!(
            args.capsule_dir.as_deref().and_then(|p| p.to_str()),
            Some("capsules/run-001")
        );
        assert_eq!(args.profile_id.as_deref(), Some("monthly-profile"));
        assert_eq!(args.lock.len(), 2);
        assert_eq!(args.max_rows, Some(100));
        assert_eq!(args.max_bytes, Some(2048));
        assert!(args.command.is_none());
    }

    #[test]
    fn parse_witness_query_without_positionals() {
        let args = Args::parse_from([
            "shape",
            "witness",
            "query",
            "--tool",
            "shape",
            "--since",
            "2026-01-01T00:00:00Z",
            "--until",
            "2026-01-02T00:00:00Z",
            "--outcome",
            "COMPATIBLE",
            "--input-hash",
            "abc123",
            "--limit",
            "5",
            "--json",
        ])
        .expect("expected witness query parse success");

        assert!(args.old.is_none());
        assert!(args.new.is_none());

        let command = args.command.expect("witness command expected");
        assert!(matches!(command, ShapeCommand::Witness { .. }));
        let ShapeCommand::Witness { action } = command else {
            return;
        };
        assert!(
            matches!(action, WitnessAction::Query(_)),
            "unexpected witness action: {action:?}"
        );
        let WitnessAction::Query(query) = action else {
            return;
        };
        assert_eq!(query.tool.as_deref(), Some("shape"));
        assert_eq!(query.since.as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(query.until.as_deref(), Some("2026-01-02T00:00:00Z"));
        assert_eq!(query.outcome.as_deref(), Some("COMPATIBLE"));
        assert_eq!(query.input_hash.as_deref(), Some("abc123"));
        assert_eq!(query.limit, 5);
        assert!(query.json);
    }

    #[test]
    fn parse_witness_last_without_positionals() {
        let args = Args::parse_from(["shape", "witness", "last", "--json"])
            .expect("expected witness last parse success");

        assert!(args.old.is_none());
        assert!(args.new.is_none());

        let command = args.command.expect("witness command expected");
        assert!(matches!(command, ShapeCommand::Witness { .. }));
        let ShapeCommand::Witness { action } = command else {
            return;
        };
        assert!(
            matches!(action, WitnessAction::Last(_)),
            "unexpected witness action: {action:?}"
        );
        let WitnessAction::Last(last) = action else {
            return;
        };
        assert!(last.json);
    }

    #[test]
    fn parse_witness_count_without_limit() {
        let args = Args::parse_from([
            "shape",
            "witness",
            "count",
            "--tool",
            "shape",
            "--since",
            "2026-01-01T00:00:00Z",
            "--until",
            "2026-01-02T00:00:00Z",
            "--outcome",
            "INCOMPATIBLE",
            "--input-hash",
            "feedbeef",
            "--json",
        ])
        .expect("expected witness count parse success");

        let command = args.command.expect("witness command expected");
        assert!(matches!(command, ShapeCommand::Witness { .. }));
        let ShapeCommand::Witness { action } = command else {
            return;
        };
        assert!(
            matches!(action, WitnessAction::Count(_)),
            "unexpected witness action: {action:?}"
        );
        let WitnessAction::Count(count) = action else {
            return;
        };
        assert_eq!(count.tool.as_deref(), Some("shape"));
        assert_eq!(count.since.as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(count.until.as_deref(), Some("2026-01-02T00:00:00Z"));
        assert_eq!(count.outcome.as_deref(), Some("INCOMPATIBLE"));
        assert_eq!(count.input_hash.as_deref(), Some("feedbeef"));
        assert!(count.json);
    }

    #[test]
    fn parse_doctor_health_without_positionals() {
        let args = Args::parse_from(["shape", "doctor", "health"])
            .expect("expected doctor health parse success");

        assert!(args.old.is_none());
        assert!(args.new.is_none());

        let command = args.command.expect("doctor command expected");
        assert!(matches!(command, ShapeCommand::Doctor(_)));
        let ShapeCommand::Doctor(doctor) = command else {
            return;
        };
        assert!(!doctor.robot_triage);
        match doctor.action {
            Some(DoctorAction::Health(health)) => assert!(!health.json),
            other => assert!(
                matches!(other, Some(DoctorAction::Health(_))),
                "unexpected doctor action: {other:?}"
            ),
        }
    }

    #[test]
    fn parse_doctor_health_json_without_positionals() {
        let args = Args::parse_from(["shape", "doctor", "health", "--json"])
            .expect("expected doctor health --json parse success");

        assert!(args.old.is_none());
        assert!(args.new.is_none());

        let command = args.command.expect("doctor command expected");
        assert!(matches!(command, ShapeCommand::Doctor(_)));
        let ShapeCommand::Doctor(doctor) = command else {
            return;
        };
        assert!(!doctor.robot_triage);
        match doctor.action {
            Some(DoctorAction::Health(health)) => assert!(health.json),
            other => assert!(
                matches!(other, Some(DoctorAction::Health(_))),
                "unexpected doctor action: {other:?}"
            ),
        }
    }

    #[test]
    fn parse_doctor_robot_triage_without_positionals() {
        let args = Args::parse_from(["shape", "doctor", "--robot-triage"])
            .expect("expected doctor robot triage parse success");

        assert!(args.old.is_none());
        assert!(args.new.is_none());

        let command = args.command.expect("doctor command expected");
        assert!(matches!(command, ShapeCommand::Doctor(_)));
        let ShapeCommand::Doctor(doctor) = command else {
            return;
        };
        assert!(doctor.robot_triage);
        assert!(!doctor.fix);
        assert!(doctor.action.is_none());
    }

    #[test]
    fn parse_capabilities_without_positionals() {
        let args = Args::parse_from(["shape", "capabilities", "--json"])
            .expect("expected capabilities parse success");

        assert!(args.old.is_none());
        assert!(args.new.is_none());

        let command = args.command.expect("capabilities command expected");
        match command {
            ShapeCommand::Capabilities(capabilities) => assert!(capabilities.json),
            other => assert!(
                matches!(other, ShapeCommand::Capabilities(_)),
                "unexpected command"
            ),
        }
    }

    #[test]
    fn parse_robot_docs_guide_without_positionals() {
        let args = Args::parse_from(["shape", "robot-docs", "guide"])
            .expect("expected robot-docs guide parse success");

        assert!(args.old.is_none());
        assert!(args.new.is_none());

        let command = args.command.expect("robot-docs command expected");
        match command {
            ShapeCommand::RobotDocs { action } => {
                assert!(matches!(action, Some(RobotDocsAction::Guide)));
            }
            other => assert!(
                matches!(other, ShapeCommand::RobotDocs { .. }),
                "unexpected command"
            ),
        }
    }

    #[test]
    fn parse_doctor_fix_without_positionals() {
        let args =
            Args::parse_from(["shape", "doctor", "--fix"]).expect("expected doctor --fix parse");

        let command = args.command.expect("doctor command expected");
        match command {
            ShapeCommand::Doctor(doctor) => {
                assert!(doctor.fix);
                assert!(!doctor.robot_triage);
                assert!(doctor.action.is_none());
            }
            other => assert!(
                matches!(other, ShapeCommand::Doctor(_)),
                "unexpected command"
            ),
        }
    }

    #[test]
    fn parse_rejects_invalid_witness_query_limit() {
        let err = Args::parse_from(["shape", "witness", "query", "--limit", "abc"])
            .expect_err("expected witness limit parse failure");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(err.to_string().contains("--limit"));
    }

    #[test]
    fn parse_rejects_witness_count_limit() {
        let err = Args::parse_from(["shape", "witness", "count", "--limit", "1"])
            .expect_err("expected parse failure for unsupported count limit");

        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
        assert!(err.to_string().contains("--limit"));
    }
}
