use std::ffi::OsString;
use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

/// CLI arguments for shape.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "shape",
    version,
    about = "Structural comparability gate for CSV datasets",
    override_usage = "shape <old.csv> <new.csv> [OPTIONS]\n       shape witness <query|last|count> [OPTIONS]",
    subcommand_negates_reqs = true
)]
pub struct Args {
    #[arg(value_name = "old.csv", required_unless_present_any = ["describe", "schema"])]
    pub old: Option<PathBuf>,

    #[arg(value_name = "new.csv", required_unless_present_any = ["describe", "schema"])]
    pub new: Option<PathBuf>,

    #[arg(long, value_name = "column")]
    pub key: Option<String>,

    #[arg(long, value_name = "delim")]
    pub delimiter: Option<String>,

    #[arg(long)]
    pub json: bool,

    #[arg(long)]
    pub no_witness: bool,

    #[arg(long = "capsule-dir", value_name = "path")]
    pub capsule_dir: Option<PathBuf>,

    #[arg(long, value_name = "path", conflicts_with = "profile_id")]
    pub profile: Option<PathBuf>,

    #[arg(long = "profile-id", value_name = "id", conflicts_with = "profile")]
    pub profile_id: Option<String>,

    #[arg(long, value_name = "lockfile", action = ArgAction::Append)]
    pub lock: Vec<PathBuf>,

    #[arg(long = "max-rows", value_name = "n")]
    pub max_rows: Option<u64>,

    #[arg(long = "max-bytes", value_name = "n")]
    pub max_bytes: Option<u64>,

    /// Show column names and other identifying metadata in output (default: redacted for zero-retention safety).
    #[arg(long)]
    pub explicit: bool,

    /// Print JSON Schema for shape.v0 output format and exit 0.
    #[arg(long)]
    pub schema: bool,

    #[arg(long)]
    pub describe: bool,

    #[command(subcommand)]
    pub command: Option<ShapeCommand>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ShapeCommand {
    Witness {
        #[command(subcommand)]
        action: WitnessAction,
    },
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
        Self::try_parse()
    }

    /// Parse CLI arguments from an iterator.
    pub fn parse_from<I, T>(itr: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        Self::try_parse_from(itr)
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::{Args, ShapeCommand, WitnessAction};

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
    fn parse_rejects_ambiguous_profile_selectors() {
        let err = Args::parse_from([
            "shape",
            "old.csv",
            "new.csv",
            "--profile",
            "profile.toml",
            "--profile-id",
            "monthly",
        ])
        .expect_err("expected parse failure");

        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn parse_rejects_malformed_max_rows() {
        let err = Args::parse_from(["shape", "old.csv", "new.csv", "--max-rows", "abc"])
            .expect_err("expected parse failure");

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
        assert!(err.to_string().contains("--max-rows"));
    }

    #[test]
    fn parse_accepts_core_and_deferred_flags() {
        let args = Args::parse_from([
            "shape",
            "old.csv",
            "new.csv",
            "--key",
            "loan_id",
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
        match command {
            ShapeCommand::Witness { action } => match action {
                WitnessAction::Query(query) => {
                    assert_eq!(query.tool.as_deref(), Some("shape"));
                    assert_eq!(query.since.as_deref(), Some("2026-01-01T00:00:00Z"));
                    assert_eq!(query.until.as_deref(), Some("2026-01-02T00:00:00Z"));
                    assert_eq!(query.outcome.as_deref(), Some("COMPATIBLE"));
                    assert_eq!(query.input_hash.as_deref(), Some("abc123"));
                    assert_eq!(query.limit, 5);
                    assert!(query.json);
                }
                _ => panic!("expected witness query action"),
            },
        }
    }

    #[test]
    fn parse_witness_last_without_positionals() {
        let args = Args::parse_from(["shape", "witness", "last", "--json"])
            .expect("expected witness last parse success");

        assert!(args.old.is_none());
        assert!(args.new.is_none());

        let command = args.command.expect("witness command expected");
        match command {
            ShapeCommand::Witness { action } => match action {
                WitnessAction::Last(last) => {
                    assert!(last.json);
                }
                _ => panic!("expected witness last action"),
            },
        }
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
        match command {
            ShapeCommand::Witness { action } => match action {
                WitnessAction::Count(count) => {
                    assert_eq!(count.tool.as_deref(), Some("shape"));
                    assert_eq!(count.since.as_deref(), Some("2026-01-01T00:00:00Z"));
                    assert_eq!(count.until.as_deref(), Some("2026-01-02T00:00:00Z"));
                    assert_eq!(count.outcome.as_deref(), Some("INCOMPATIBLE"));
                    assert_eq!(count.input_hash.as_deref(), Some("feedbeef"));
                    assert!(count.json);
                }
                _ => panic!("expected witness count action"),
            },
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
