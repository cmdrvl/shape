use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

use super::record::{WitnessRecord, canonical_json};

/// Resolve witness ledger path:
/// 1) `EPISTEMIC_WITNESS` when set
/// 2) `~/.epistemic/witness.jsonl` fallback
pub(crate) fn resolve_ledger_path() -> io::Result<PathBuf> {
    if let Ok(path) = std::env::var("EPISTEMIC_WITNESS")
        && !path.trim().is_empty()
    {
        return Ok(PathBuf::from(path));
    }

    let home = home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "could not determine home directory; set EPISTEMIC_WITNESS",
        )
    })?;
    Ok(home.join(".epistemic").join("witness.jsonl"))
}

pub struct LedgerWriter {
    path: PathBuf,
}

impl LedgerWriter {
    pub fn open() -> io::Result<Self> {
        let path = resolve_ledger_path()?;
        Ok(Self { path })
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    /// Append a record as canonical JSONL.
    pub fn append(&self, record: &WitnessRecord) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let encoded = canonical_json(record);
        writeln!(file, "{encoded}")?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::super::record::WitnessRecord;
    use super::{LedgerWriter, resolve_ledger_path};
    use crate::checks::suite::Outcome;
    use crate::cli::args::Args;
    use crate::orchestrator::PipelineResult;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_record() -> WitnessRecord {
        let args = Args {
            old: Some(PathBuf::from("old.csv")),
            new: Some(PathBuf::from("new.csv")),
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
            explicit: false,
            schema: false,
            describe: false,
            command: None,
        };
        let result = PipelineResult {
            outcome: Outcome::Compatible,
            output: "ok".to_owned(),
            resolved_profile_id: None,
            resolved_profile_sha256: None,
        };
        let mut record =
            WitnessRecord::from_run(&args, &result, b"old", b"new", "old.csv", "new.csv");
        record.ts = "2026-01-01T00:00:00Z".to_owned();
        record.compute_id();
        record
    }

    fn temp_ledger_path() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "shape_witness_ledger_test_{}-{seq}-{nanos}",
                std::process::id()
            ))
            .join("witness.jsonl")
    }

    #[test]
    fn append_creates_file_and_single_line() {
        let path = temp_ledger_path();
        let writer = LedgerWriter::with_path(path.clone());
        let record = make_record();
        writer.append(&record).expect("append");

        let content = std::fs::read_to_string(&path).expect("read ledger");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);

        let parsed: serde_json::Value = serde_json::from_str(lines[0]).expect("parse json");
        assert_eq!(parsed["tool"], "shape");

        std::fs::remove_file(path.clone()).ok();
        std::fs::remove_dir(path.parent().expect("parent")).ok();
    }

    #[test]
    fn append_is_additive() {
        let path = temp_ledger_path();
        let writer = LedgerWriter::with_path(path.clone());

        let first = make_record();
        writer.append(&first).expect("append first");
        let mut second = make_record();
        second.outcome = "INCOMPATIBLE".to_owned();
        second.compute_id();
        writer.append(&second).expect("append second");

        let content = std::fs::read_to_string(&path).expect("read ledger");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        let parsed: serde_json::Value = serde_json::from_str(lines[1]).expect("parse second");
        assert_eq!(parsed["id"], second.id);
        assert_eq!(parsed["outcome"], "INCOMPATIBLE");

        std::fs::remove_file(path.clone()).ok();
        std::fs::remove_dir(path.parent().expect("parent")).ok();
    }

    #[test]
    fn resolve_ledger_path_default_shape() {
        if std::env::var("EPISTEMIC_WITNESS").is_ok() {
            return;
        }

        let resolved = resolve_ledger_path().expect("resolve default witness path");
        assert!(
            resolved.ends_with(".epistemic/witness.jsonl")
                || resolved.ends_with(".epistemic\\witness.jsonl")
        );
    }
}
