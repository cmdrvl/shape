pub mod hash;
pub mod ledger;
pub mod record;

use crate::checks::suite::Outcome;
use crate::cli::args::Args;
use crate::orchestrator::PipelineResult;

/// Build and append a witness record for a completed shape run.
///
/// Witness failures are intentionally non-fatal and never alter the domain
/// outcome/exit behavior of the comparison pipeline.
pub fn record_run(args: &Args, result: &PipelineResult) {
    let writer = match ledger::LedgerWriter::open() {
        Ok(writer) => writer,
        Err(error) => {
            if should_report_witness_error(result.outcome) {
                eprintln!("shape: witness: {error}");
            }
            return;
        }
    };

    if let Err(error) = record_run_with_writer(args, result, &writer)
        && should_report_witness_error(result.outcome)
    {
        eprintln!("shape: witness: {error}");
    }
}

fn should_report_witness_error(outcome: Outcome) -> bool {
    outcome != Outcome::Refusal
}

fn record_run_with_writer(
    args: &Args,
    result: &PipelineResult,
    writer: &ledger::LedgerWriter,
) -> Result<(), Box<dyn std::error::Error>> {
    let old_path = args.old.as_ref().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing old input path")
    })?;
    let new_path = args.new.as_ref().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing new input path")
    })?;

    let old_bytes = std::fs::read(old_path)?;
    let new_bytes = std::fs::read(new_path)?;

    let old_path_str = old_path.to_string_lossy().into_owned();
    let new_path_str = new_path.to_string_lossy().into_owned();
    let prev = writer.read_prev();

    let mut record = record::WitnessRecord::from_run(
        args,
        result,
        &old_bytes,
        &new_bytes,
        &old_path_str,
        &new_path_str,
        prev,
    );
    record.compute_id();
    writer.append(&record)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ledger::LedgerWriter;
    use super::record_run_with_writer;
    use crate::checks::suite::Outcome;
    use crate::cli::args::Args;
    use crate::orchestrator::PipelineResult;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "shape_witness_mod_test_{}-{nanos}-{seq}",
            std::process::id()
        ))
    }

    fn write_csv(dir: &PathBuf, name: &str, contents: &str) -> PathBuf {
        std::fs::create_dir_all(dir).expect("create temp dir");
        let path = dir.join(name);
        std::fs::write(&path, contents).expect("write csv");
        path
    }

    fn make_args(old: PathBuf, new: PathBuf) -> Args {
        Args {
            old: Some(old),
            new: Some(new),
            key: Some("id".to_owned()),
            delimiter: None,
            json: false,
            no_witness: false,
            capsule_dir: None,
            profile: None,
            profile_id: None,
            lock: Vec::new(),
            max_rows: None,
            max_bytes: None,
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
    fn record_run_with_writer_writes_one_record() {
        let dir = temp_dir();
        let old = write_csv(&dir, "old.csv", "id,value\nA,1\n");
        let new = write_csv(&dir, "new.csv", "id,value\nA,2\n");
        let ledger_path = dir.join("witness.jsonl");
        let writer = LedgerWriter::with_path(ledger_path.clone());

        let args = make_args(old, new);
        let result = make_result(Outcome::Compatible);
        record_run_with_writer(&args, &result, &writer).expect("record run");

        let content = std::fs::read_to_string(&ledger_path).expect("read ledger");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);

        let parsed: serde_json::Value = serde_json::from_str(lines[0]).expect("parse json");
        assert_eq!(parsed["tool"], "shape");
        assert_eq!(parsed["outcome"], "COMPATIBLE");
        assert_eq!(parsed["exit_code"], 0);

        std::fs::remove_file(ledger_path).ok();
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn record_run_with_writer_chains_prev_values() {
        let dir = temp_dir();
        let old = write_csv(&dir, "old.csv", "id,value\nA,1\n");
        let new = write_csv(&dir, "new.csv", "id,value\nA,2\n");
        let ledger_path = dir.join("witness.jsonl");
        let writer = LedgerWriter::with_path(ledger_path.clone());

        let args = make_args(old, new);
        record_run_with_writer(&args, &make_result(Outcome::Compatible), &writer)
            .expect("first run");
        record_run_with_writer(&args, &make_result(Outcome::Incompatible), &writer)
            .expect("second run");

        let content = std::fs::read_to_string(&ledger_path).expect("read ledger");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let first: serde_json::Value = serde_json::from_str(lines[0]).expect("parse first");
        let second: serde_json::Value = serde_json::from_str(lines[1]).expect("parse second");
        assert_eq!(second["prev"], first["id"]);

        std::fs::remove_file(ledger_path).ok();
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn record_run_with_writer_errors_when_paths_missing() {
        let writer = LedgerWriter::with_path(temp_dir().join("witness.jsonl"));
        let args = Args {
            old: None,
            new: None,
            key: None,
            delimiter: None,
            json: false,
            no_witness: false,
            capsule_dir: None,
            profile: None,
            profile_id: None,
            lock: Vec::new(),
            max_rows: None,
            max_bytes: None,
            describe: false,
            command: None,
        };

        let result = make_result(Outcome::Refusal);
        let err = record_run_with_writer(&args, &result, &writer).expect_err("missing paths");
        assert!(err.to_string().contains("missing old input path"));
    }
}
