use std::fs::File;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cli::args::{WitnessAction, WitnessCountArgs, WitnessLastArgs, WitnessQueryArgs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessResponse {
    pub exit_code: u8,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

impl WitnessResponse {
    fn success_human(output: String) -> Self {
        Self {
            exit_code: 0,
            stdout: Some(format!("{output}\n")),
            stderr: None,
        }
    }

    fn success_json<T: Serialize>(value: &T) -> Self {
        match serde_json::to_string(value) {
            Ok(encoded) => Self {
                exit_code: 0,
                stdout: Some(format!("{encoded}\n")),
                stderr: None,
            },
            Err(error) => Self::internal_error(format!(
                "shape: witness: failed to serialize JSON output: {error}"
            )),
        }
    }

    fn no_match_human(message: &str) -> Self {
        Self {
            exit_code: 1,
            stdout: None,
            stderr: Some(format!("{message}\n")),
        }
    }

    fn no_match_json<T: Serialize>(value: &T, message: &str) -> Self {
        match serde_json::to_string(value) {
            Ok(encoded) => Self {
                exit_code: 1,
                stdout: Some(format!("{encoded}\n")),
                stderr: Some(format!("{message}\n")),
            },
            Err(error) => Self::internal_error(format!(
                "shape: witness: failed to serialize JSON output: {error}"
            )),
        }
    }

    fn internal_error(message: String) -> Self {
        Self {
            exit_code: 2,
            stdout: None,
            stderr: Some(format!("{message}\n")),
        }
    }
}

pub fn execute(action: &WitnessAction) -> WitnessResponse {
    let records = match load_records() {
        Ok(records) => records,
        Err(error) => {
            return WitnessResponse::internal_error(format!(
                "shape: witness: failed to read ledger: {error}"
            ));
        }
    };

    execute_with_records(action, &records)
}

fn execute_with_records(action: &WitnessAction, records: &[WitnessRecord]) -> WitnessResponse {
    match action {
        WitnessAction::Last(args) => execute_last(args, records),
        WitnessAction::Query(args) => execute_query(args, records),
        WitnessAction::Count(args) => execute_count(args, records),
    }
}

fn execute_last(args: &WitnessLastArgs, records: &[WitnessRecord]) -> WitnessResponse {
    match records.last() {
        Some(record) => {
            if args.json {
                WitnessResponse::success_json(record)
            } else {
                WitnessResponse::success_human(format_record_human(record))
            }
        }
        None => {
            if args.json {
                WitnessResponse::no_match_json(
                    &serde_json::Value::Null,
                    "shape: witness ledger is empty",
                )
            } else {
                WitnessResponse::no_match_human("shape: witness ledger is empty")
            }
        }
    }
}

fn execute_query(args: &WitnessQueryArgs, records: &[WitnessRecord]) -> WitnessResponse {
    let filter = QueryFilter::for_query(args);
    let matched: Vec<WitnessRecord> = filter.apply(records.iter().cloned());

    if matched.is_empty() {
        if args.json {
            return WitnessResponse::no_match_json(
                &Vec::<WitnessRecord>::new(),
                "shape: no matching witness records",
            );
        }
        return WitnessResponse::no_match_human("shape: no matching witness records");
    }

    if args.json {
        WitnessResponse::success_json(&matched)
    } else {
        WitnessResponse::success_human(format_records_human(&matched))
    }
}

fn execute_count(args: &WitnessCountArgs, records: &[WitnessRecord]) -> WitnessResponse {
    let filter = QueryFilter::for_count(args);
    let count = records
        .iter()
        .filter(|record| filter.matches(record))
        .count();

    if count == 0 {
        if args.json {
            return WitnessResponse::no_match_json(
                &serde_json::json!({ "count": 0usize }),
                "shape: no matching witness records",
            );
        }
        return WitnessResponse::no_match_human("shape: no matching witness records");
    }

    if args.json {
        WitnessResponse::success_json(&serde_json::json!({ "count": count }))
    } else {
        WitnessResponse::success_human(count.to_string())
    }
}

#[derive(Debug, Clone, Default)]
struct QueryFilter {
    tool: Option<String>,
    since: Option<String>,
    until: Option<String>,
    outcome: Option<String>,
    input_hash: Option<String>,
    limit: Option<usize>,
}

impl QueryFilter {
    fn for_query(args: &WitnessQueryArgs) -> Self {
        Self {
            tool: args.tool.clone(),
            since: args.since.clone(),
            until: args.until.clone(),
            outcome: args.outcome.clone(),
            input_hash: args.input_hash.clone(),
            limit: Some(args.limit),
        }
    }

    fn for_count(args: &WitnessCountArgs) -> Self {
        Self {
            tool: args.tool.clone(),
            since: args.since.clone(),
            until: args.until.clone(),
            outcome: args.outcome.clone(),
            input_hash: args.input_hash.clone(),
            limit: None,
        }
    }

    fn apply<I>(&self, records: I) -> Vec<WitnessRecord>
    where
        I: Iterator<Item = WitnessRecord>,
    {
        let iter = records.filter(|record| self.matches(record));
        match self.limit {
            Some(limit) => iter.take(limit).collect(),
            None => iter.collect(),
        }
    }

    fn matches(&self, record: &WitnessRecord) -> bool {
        if let Some(tool) = self.tool.as_deref()
            && record.tool != tool
        {
            return false;
        }

        if let Some(since) = self.since.as_deref()
            && record.ts.as_str() < since
        {
            return false;
        }

        if let Some(until) = self.until.as_deref()
            && record.ts.as_str() > until
        {
            return false;
        }

        if let Some(outcome) = self.outcome.as_deref()
            && !record.outcome.eq_ignore_ascii_case(outcome)
        {
            return false;
        }

        if let Some(input_hash) = self.input_hash.as_deref()
            && !record
                .inputs
                .iter()
                .any(|input| input.hash.contains(input_hash))
        {
            return false;
        }

        true
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
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
    pub ts: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WitnessInput {
    pub path: String,
    pub hash: String,
    pub bytes: u64,
}

fn format_record_human(record: &WitnessRecord) -> String {
    let mut lines = Vec::new();
    lines.push(format!("id:       {}", record.id));
    lines.push(format!("ts:       {}", record.ts));
    lines.push(format!("tool:     {}", record.tool));
    lines.push(format!("version:  {}", record.version));
    lines.push(format!("outcome:  {}", record.outcome));
    lines.push(format!("exit:     {}", record.exit_code));
    for (index, input) in record.inputs.iter().enumerate() {
        lines.push(format!(
            "input[{index}]: {} ({} bytes, {})",
            input.path, input.bytes, input.hash
        ));
    }
    lines.join("\n")
}

fn format_records_human(records: &[WitnessRecord]) -> String {
    records
        .iter()
        .map(format_record_human)
        .collect::<Vec<_>>()
        .join("\n---\n")
}

fn load_records() -> io::Result<Vec<WitnessRecord>> {
    let ledger_path = resolve_ledger_path()?;
    load_records_from_path(&ledger_path)
}

fn load_records_from_path(path: &Path) -> io::Result<Vec<WitnessRecord>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let reader = io::BufReader::new(file);

    let mut records = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(record) = serde_json::from_str::<WitnessRecord>(trimmed) {
            records.push(record);
        }
    }

    Ok(records)
}

fn resolve_ledger_path() -> io::Result<PathBuf> {
    if let Ok(path) = std::env::var("EPISTEMIC_WITNESS") {
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

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        WitnessInput, WitnessRecord, execute_with_records, load_records_from_path,
        resolve_ledger_path,
    };
    use crate::cli::args::{WitnessAction, WitnessCountArgs, WitnessLastArgs, WitnessQueryArgs};

    fn sample_record(
        id: &str,
        ts: &str,
        outcome: &str,
        input_hash: &str,
        exit_code: u8,
    ) -> WitnessRecord {
        WitnessRecord {
            id: id.to_owned(),
            tool: "shape".to_owned(),
            version: "0.1.0".to_owned(),
            binary_hash: "blake3:binary".to_owned(),
            inputs: vec![WitnessInput {
                path: "old.csv".to_owned(),
                hash: input_hash.to_owned(),
                bytes: 100,
            }],
            params: serde_json::json!({"json": false}),
            outcome: outcome.to_owned(),
            exit_code,
            output_hash: "blake3:output".to_owned(),
            ts: ts.to_owned(),
        }
    }

    fn query_action(
        tool: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        outcome: Option<&str>,
        input_hash: Option<&str>,
        limit: usize,
        json: bool,
    ) -> WitnessAction {
        WitnessAction::Query(WitnessQueryArgs {
            tool: tool.map(str::to_owned),
            since: since.map(str::to_owned),
            until: until.map(str::to_owned),
            outcome: outcome.map(str::to_owned),
            input_hash: input_hash.map(str::to_owned),
            limit,
            json,
        })
    }

    fn count_action(
        tool: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        outcome: Option<&str>,
        input_hash: Option<&str>,
        json: bool,
    ) -> WitnessAction {
        WitnessAction::Count(WitnessCountArgs {
            tool: tool.map(str::to_owned),
            since: since.map(str::to_owned),
            until: until.map(str::to_owned),
            outcome: outcome.map(str::to_owned),
            input_hash: input_hash.map(str::to_owned),
            json,
        })
    }

    fn last_action(json: bool) -> WitnessAction {
        WitnessAction::Last(WitnessLastArgs { json })
    }

    fn temp_ledger_path() -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "shape-witness-query-test-{}-{nanos}-{counter}.jsonl",
            std::process::id()
        ))
    }

    #[test]
    fn query_filters_and_applies_limit() {
        let records = vec![
            sample_record(
                "blake3:1",
                "2026-02-01T00:00:00Z",
                "COMPATIBLE",
                "blake3:aaa111",
                0,
            ),
            sample_record(
                "blake3:2",
                "2026-02-02T00:00:00Z",
                "INCOMPATIBLE",
                "blake3:bbb222",
                1,
            ),
            sample_record(
                "blake3:3",
                "2026-02-03T00:00:00Z",
                "COMPATIBLE",
                "blake3:ccc333",
                0,
            ),
        ];

        let action = query_action(
            Some("shape"),
            Some("2026-02-01T00:00:00Z"),
            Some("2026-02-10T00:00:00Z"),
            Some("compatible"),
            None,
            1,
            true,
        );
        let response = execute_with_records(&action, &records);

        assert_eq!(response.exit_code, 0);
        assert!(response.stderr.is_none());
        let payload = response.stdout.expect("query JSON output");
        let parsed: serde_json::Value =
            serde_json::from_str(payload.trim()).expect("valid query JSON");
        let rows = parsed.as_array().expect("query should return array");
        assert_eq!(rows.len(), 1, "limit should be respected for query");
        assert_eq!(rows[0]["id"], "blake3:1");
    }

    #[test]
    fn query_no_matches_returns_exit_1_and_json_array() {
        let records = vec![sample_record(
            "blake3:1",
            "2026-02-01T00:00:00Z",
            "COMPATIBLE",
            "blake3:aaa111",
            0,
        )];
        let action = query_action(None, None, None, Some("REFUSAL"), None, 20, true);

        let response = execute_with_records(&action, &records);
        assert_eq!(response.exit_code, 1);
        assert!(
            response
                .stderr
                .as_deref()
                .is_some_and(|msg| msg.contains("no matching witness records"))
        );

        let payload = response.stdout.expect("query JSON output");
        let parsed: serde_json::Value =
            serde_json::from_str(payload.trim()).expect("valid query JSON");
        assert_eq!(parsed, serde_json::json!([]));
    }

    #[test]
    fn last_returns_latest_record() {
        let records = vec![
            sample_record(
                "blake3:older",
                "2026-01-01T00:00:00Z",
                "COMPATIBLE",
                "blake3:old",
                0,
            ),
            sample_record(
                "blake3:newer",
                "2026-01-02T00:00:00Z",
                "INCOMPATIBLE",
                "blake3:new",
                1,
            ),
        ];

        let response = execute_with_records(&last_action(true), &records);
        assert_eq!(response.exit_code, 0);
        let payload = response.stdout.expect("last JSON output");
        let parsed: serde_json::Value =
            serde_json::from_str(payload.trim()).expect("valid last JSON");
        assert_eq!(parsed["id"], "blake3:newer");
    }

    #[test]
    fn last_on_empty_ledger_returns_exit_1() {
        let response = execute_with_records(&last_action(false), &[]);
        assert_eq!(response.exit_code, 1);
        assert!(response.stdout.is_none());
        assert!(
            response
                .stderr
                .as_deref()
                .is_some_and(|message| message.contains("ledger is empty"))
        );
    }

    #[test]
    fn count_returns_full_match_count() {
        let records = vec![
            sample_record(
                "blake3:1",
                "2026-02-01T00:00:00Z",
                "COMPATIBLE",
                "blake3:aaa111",
                0,
            ),
            sample_record(
                "blake3:2",
                "2026-02-02T00:00:00Z",
                "COMPATIBLE",
                "blake3:bbb222",
                0,
            ),
            sample_record(
                "blake3:3",
                "2026-02-03T00:00:00Z",
                "INCOMPATIBLE",
                "blake3:ccc333",
                1,
            ),
        ];

        let action = count_action(None, None, None, Some("COMPATIBLE"), None, true);
        let response = execute_with_records(&action, &records);
        assert_eq!(response.exit_code, 0);
        let payload = response.stdout.expect("count JSON output");
        let parsed: serde_json::Value =
            serde_json::from_str(payload.trim()).expect("valid count JSON");
        assert_eq!(parsed["count"], 2);
    }

    #[test]
    fn count_with_zero_matches_returns_exit_1() {
        let records = vec![sample_record(
            "blake3:1",
            "2026-02-01T00:00:00Z",
            "COMPATIBLE",
            "blake3:aaa111",
            0,
        )];
        let action = count_action(None, None, None, Some("REFUSAL"), None, false);
        let response = execute_with_records(&action, &records);
        assert_eq!(response.exit_code, 1);
        assert_eq!(response.stdout, None);
        assert!(
            response
                .stderr
                .as_deref()
                .is_some_and(|message| message.contains("no matching witness records"))
        );
    }

    #[test]
    fn load_records_skips_malformed_lines_and_blank_lines() {
        let path = temp_ledger_path();
        let valid = sample_record(
            "blake3:ok",
            "2026-02-01T00:00:00Z",
            "COMPATIBLE",
            "blake3:abc",
            0,
        );
        let valid_line = serde_json::to_string(&valid).expect("valid witness record");
        let partial_line = r#"{"id":"blake3:partial"}"#;
        fs::write(&path, format!("{valid_line}\nnot-json\n{partial_line}\n\n"))
            .expect("write ledger fixture");

        let records = load_records_from_path(&path).expect("load witness records");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "blake3:ok");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn load_records_returns_empty_for_missing_ledger() {
        let path = temp_ledger_path();
        let records = load_records_from_path(&path).expect("missing ledger should not error");
        assert!(records.is_empty());
    }

    #[test]
    fn resolve_ledger_path_uses_epistemic_default_suffix() {
        let path = resolve_ledger_path().expect("resolve witness ledger path");

        if let Ok(expected) = std::env::var("EPISTEMIC_WITNESS") {
            assert_eq!(path, PathBuf::from(expected));
            return;
        }

        assert!(
            path.ends_with(".epistemic/witness.jsonl")
                || path.ends_with(".epistemic\\witness.jsonl")
        );
    }
}
