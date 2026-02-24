use serde::{Deserialize, Serialize};

use super::hash::{hash_bytes, hash_self};
use crate::checks::suite::Outcome;
use crate::cli::args::Args;
use crate::cli::exit;
use crate::orchestrator::PipelineResult;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WitnessRecord {
    pub id: String,
    pub tool: String,
    pub version: String,
    pub binary_hash: String,
    pub inputs: Vec<WitnessInput>,
    pub params: serde_json::Value,
    pub outcome: String,
    pub exit_code: u8,
    pub output_hash: String,
    pub prev: Option<String>,
    pub ts: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WitnessInput {
    pub path: String,
    pub hash: String,
    pub bytes: u64,
}

/// Serialize a record as canonical, single-line JSON.
///
/// This is the single serialization path for witness records so ID computation
/// and ledger appends stay consistent.
pub fn canonical_json(record: &WitnessRecord) -> String {
    let value = serde_json::to_value(record).expect("WitnessRecord is serializable");
    serde_json::to_string(&value).expect("serde_json::Value is serializable")
}

impl WitnessRecord {
    pub fn from_run(
        args: &Args,
        result: &PipelineResult,
        old_bytes: &[u8],
        new_bytes: &[u8],
        old_path: &str,
        new_path: &str,
        prev: Option<String>,
    ) -> Self {
        let binary_hash = hash_self()
            .map(|value| format!("blake3:{value}"))
            .unwrap_or_default();

        let inputs = vec![
            WitnessInput {
                path: old_path.to_owned(),
                hash: format!("blake3:{}", hash_bytes(old_bytes)),
                bytes: old_bytes.len() as u64,
            },
            WitnessInput {
                path: new_path.to_owned(),
                hash: format!("blake3:{}", hash_bytes(new_bytes)),
                bytes: new_bytes.len() as u64,
            },
        ];

        let outcome = match result.outcome {
            Outcome::Compatible => "COMPATIBLE",
            Outcome::Incompatible => "INCOMPATIBLE",
            Outcome::Refusal => "REFUSAL",
        };

        let profile = args
            .profile
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        let lock: Vec<String> = args
            .lock
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let params = serde_json::json!({
            "key": args.key.clone(),
            "delimiter": args.delimiter.clone(),
            "json": args.json,
            "no_witness": args.no_witness,
            "profile": profile,
            "profile_id": args.profile_id.clone(),
            "lock": lock,
            "max_rows": args.max_rows,
            "max_bytes": args.max_bytes,
        });

        Self {
            id: String::new(),
            tool: "shape".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            binary_hash,
            inputs,
            params,
            outcome: outcome.to_owned(),
            exit_code: exit::exit_code(result.outcome),
            output_hash: format!("blake3:{}", hash_bytes(result.output.as_bytes())),
            prev,
            ts: current_utc_iso8601(),
        }
    }

    /// Compute the content-addressed id by hashing canonical JSON with
    /// `id` blanked.
    pub fn compute_id(&mut self) {
        self.id.clear();
        let canonical = canonical_json(self);
        self.id = format!("blake3:{}", hash_bytes(canonical.as_bytes()));
    }
}

fn current_utc_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_seconds = elapsed.as_secs();

    let days = total_seconds / 86_400;
    let time_of_day = total_seconds % 86_400;
    let hours = time_of_day / 3_600;
    let minutes = (time_of_day % 3_600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_date(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since UNIX epoch to Gregorian (year, month, day).
/// Algorithm: Howard Hinnant civil-from-days.
fn days_to_date(days_since_epoch: u64) -> (i64, u64, u64) {
    let z = days_since_epoch as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::WitnessRecord;
    use crate::checks::suite::Outcome;
    use crate::cli::args::Args;
    use crate::orchestrator::PipelineResult;
    use std::path::PathBuf;

    fn make_args() -> Args {
        Args {
            old: Some(PathBuf::from("old.csv")),
            new: Some(PathBuf::from("new.csv")),
            key: Some("loan_id".to_owned()),
            delimiter: Some("comma".to_owned()),
            json: true,
            no_witness: false,
            profile: None,
            profile_id: Some("profile.v0".to_owned()),
            lock: vec![PathBuf::from("shape.lock")],
            max_rows: Some(100),
            max_bytes: Some(2048),
            describe: false,
            command: None,
        }
    }

    fn make_result(outcome: Outcome) -> PipelineResult {
        PipelineResult {
            outcome,
            output: "shape output".to_owned(),
        }
    }

    #[test]
    fn from_run_populates_expected_fields() {
        let args = make_args();
        let result = make_result(Outcome::Compatible);

        let mut record = WitnessRecord::from_run(
            &args,
            &result,
            b"old-bytes",
            b"new-bytes",
            "old.csv",
            "new.csv",
            None,
        );
        record.compute_id();

        assert_eq!(record.tool, "shape");
        assert_eq!(record.outcome, "COMPATIBLE");
        assert_eq!(record.exit_code, 0);
        assert_eq!(record.inputs.len(), 2);
        assert_eq!(record.inputs[0].path, "old.csv");
        assert_eq!(record.inputs[1].path, "new.csv");
        assert!(record.id.starts_with("blake3:"));
        assert!(record.output_hash.starts_with("blake3:"));
        assert!(record.ts.ends_with('Z'));
    }

    #[test]
    fn from_run_maps_all_outcomes() {
        let args = make_args();

        let compatible = WitnessRecord::from_run(
            &args,
            &make_result(Outcome::Compatible),
            b"a",
            b"b",
            "a.csv",
            "b.csv",
            None,
        );
        assert_eq!(compatible.outcome, "COMPATIBLE");
        assert_eq!(compatible.exit_code, 0);

        let incompatible = WitnessRecord::from_run(
            &args,
            &make_result(Outcome::Incompatible),
            b"a",
            b"b",
            "a.csv",
            "b.csv",
            None,
        );
        assert_eq!(incompatible.outcome, "INCOMPATIBLE");
        assert_eq!(incompatible.exit_code, 1);

        let refusal = WitnessRecord::from_run(
            &args,
            &make_result(Outcome::Refusal),
            b"a",
            b"b",
            "a.csv",
            "b.csv",
            None,
        );
        assert_eq!(refusal.outcome, "REFUSAL");
        assert_eq!(refusal.exit_code, 2);
    }

    #[test]
    fn compute_id_is_deterministic_for_identical_record_content() {
        let args = make_args();
        let result = make_result(Outcome::Compatible);

        let mut first = WitnessRecord::from_run(
            &args,
            &result,
            b"a",
            b"b",
            "a.csv",
            "b.csv",
            Some("prev".to_owned()),
        );
        first.ts = "2026-01-01T00:00:00Z".to_owned();
        first.binary_hash = "blake3:fixed".to_owned();
        first.compute_id();

        let mut second = first.clone();
        second.id.clear();
        second.compute_id();

        assert_eq!(first.id, second.id);
    }
}
