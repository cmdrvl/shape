use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::refusal::codes::RefusalCode;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefusalPayload {
    pub code: RefusalCode,
    pub message: String,
    pub detail: Value,
    pub next_command: Option<String>,
}

impl RefusalPayload {
    pub fn from_code(code: RefusalCode) -> Self {
        Self::new(code, code.reason())
    }

    pub fn new(code: RefusalCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: Value::Object(Map::new()),
            next_command: None,
        }
    }

    pub fn with_detail(mut self, detail: Value) -> Self {
        self.detail = detail;
        self
    }

    pub fn with_next_command(mut self, next_command: impl Into<String>) -> Self {
        if self.code.supports_next_command() {
            self.next_command = Some(next_command.into());
        }
        self
    }

    pub fn with_optional_next_command(mut self, next_command: Option<String>) -> Self {
        if self.code.supports_next_command() {
            self.next_command = next_command;
        }
        self
    }

    pub fn build_next_command_for_dialect(
        old_file: &str,
        new_file: &str,
        delimiter: &str,
    ) -> String {
        format!(
            "shape {} {} --delimiter {} --json",
            shell_quote(old_file),
            shell_quote(new_file),
            shell_quote(delimiter)
        )
    }

    pub fn build_next_command_for_too_large(
        old_file: &str,
        new_file: &str,
        limit_flag: &str,
        actual: u64,
    ) -> String {
        format!(
            "shape {} {} {} {} --json",
            shell_quote(old_file),
            shell_quote(new_file),
            shell_quote(limit_flag),
            actual
        )
    }

    pub fn io(file: impl Into<String>, error: impl Into<String>) -> Self {
        Self::from_code(RefusalCode::EIo)
            .with_detail(json!({ "file": file.into(), "error": error.into() }))
    }

    pub fn encoding(file: impl Into<String>, issue: impl Into<String>) -> Self {
        Self::from_code(RefusalCode::EEncoding)
            .with_detail(json!({ "file": file.into(), "issue": issue.into() }))
    }

    pub fn csv_parse(file: impl Into<String>, line: u64, error: impl Into<String>) -> Self {
        Self::from_code(RefusalCode::ECsvParse)
            .with_detail(json!({ "file": file.into(), "line": line, "error": error.into() }))
    }

    pub fn empty(file: impl Into<String>, rows: u64) -> Self {
        Self::from_code(RefusalCode::EEmpty)
            .with_detail(json!({ "file": file.into(), "rows": rows }))
    }

    pub fn headers_missing(file: impl Into<String>) -> Self {
        Self::from_code(RefusalCode::EHeaders)
            .with_detail(json!({ "file": file.into(), "issue": "missing" }))
    }

    pub fn headers_duplicate(file: impl Into<String>, name: impl Into<String>) -> Self {
        Self::from_code(RefusalCode::EHeaders).with_detail(json!({
            "file": file.into(),
            "issue": "duplicate",
            "name": name.into(),
        }))
    }

    pub fn dialect(
        file: impl Into<String>,
        candidates: Vec<String>,
        next_command: Option<String>,
    ) -> Self {
        Self::from_code(RefusalCode::EDialect)
            .with_detail(json!({ "file": file.into(), "candidates": candidates }))
            .with_optional_next_command(next_command)
    }

    pub fn ambiguous_profile(
        profile_path: impl Into<String>,
        profile_id: impl Into<String>,
    ) -> Self {
        Self::from_code(RefusalCode::EAmbiguousProfile).with_detail(json!({
            "profile_path": profile_path.into(),
            "profile_id": profile_id.into(),
        }))
    }

    pub fn input_not_locked(file: impl Into<String>) -> Self {
        Self::from_code(RefusalCode::EInputNotLocked).with_detail(json!({ "file": file.into() }))
    }

    pub fn input_drift(
        file: impl Into<String>,
        expected_hash: impl Into<String>,
        actual_hash: impl Into<String>,
    ) -> Self {
        Self::from_code(RefusalCode::EInputDrift).with_detail(json!({
            "file": file.into(),
            "expected_hash": expected_hash.into(),
            "actual_hash": actual_hash.into(),
        }))
    }

    pub fn too_large(
        file: impl Into<String>,
        limit_flag: impl Into<String>,
        limit: u64,
        actual: u64,
        next_command: Option<String>,
    ) -> Self {
        Self::from_code(RefusalCode::ETooLarge)
            .with_detail(json!({
                "file": file.into(),
                "limit_flag": limit_flag.into(),
                "limit": limit,
                "actual": actual,
            }))
            .with_optional_next_command(next_command)
    }
}

fn shell_quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-' | b':'))
    {
        return arg.to_string();
    }

    let escaped = arg.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

#[cfg(test)]
mod tests {
    use super::RefusalCode;
    use super::RefusalPayload;
    use serde_json::Value;

    #[test]
    fn io_detail_shape_matches_plan() {
        let payload = RefusalPayload::io("old.csv", "permission denied");
        assert_eq!(payload.code, RefusalCode::EIo);
        assert_eq!(payload.message, RefusalCode::EIo.reason());
        assert_eq!(payload.detail["file"].as_str(), Some("old.csv"));
        assert_eq!(payload.detail["error"].as_str(), Some("permission denied"));
        assert_eq!(payload.next_command, None);
    }

    #[test]
    fn encoding_detail_shape_matches_plan() {
        let payload = RefusalPayload::encoding("old.csv", "utf16_le_bom");
        assert_eq!(payload.code, RefusalCode::EEncoding);
        assert_eq!(payload.message, RefusalCode::EEncoding.reason());
        assert_eq!(payload.detail["file"].as_str(), Some("old.csv"));
        assert_eq!(payload.detail["issue"].as_str(), Some("utf16_le_bom"));
    }

    #[test]
    fn csv_parse_detail_shape_matches_plan() {
        let payload = RefusalPayload::csv_parse("old.csv", 42, "quote not closed");
        assert_eq!(payload.code, RefusalCode::ECsvParse);
        assert_eq!(payload.message, RefusalCode::ECsvParse.reason());
        assert_eq!(payload.detail["file"].as_str(), Some("old.csv"));
        assert_eq!(payload.detail["line"].as_u64(), Some(42));
        assert_eq!(payload.detail["error"].as_str(), Some("quote not closed"));
    }

    #[test]
    fn empty_detail_shape_matches_plan() {
        let payload = RefusalPayload::empty("new.csv", 0);
        assert_eq!(payload.code, RefusalCode::EEmpty);
        assert_eq!(payload.message, RefusalCode::EEmpty.reason());
        assert_eq!(payload.detail["file"].as_str(), Some("new.csv"));
        assert_eq!(payload.detail["rows"].as_u64(), Some(0));
    }

    #[test]
    fn headers_missing_shape_matches_plan() {
        let payload = RefusalPayload::headers_missing("new.csv");
        assert_eq!(payload.code, RefusalCode::EHeaders);
        assert_eq!(payload.message, RefusalCode::EHeaders.reason());
        assert_eq!(payload.detail["file"].as_str(), Some("new.csv"));
        assert_eq!(payload.detail["issue"].as_str(), Some("missing"));
        assert_eq!(payload.detail.get("name"), None);
    }

    #[test]
    fn headers_duplicate_shape_matches_plan() {
        let payload = RefusalPayload::headers_duplicate("old.csv", "u8:amount");
        assert_eq!(payload.code, RefusalCode::EHeaders);
        assert_eq!(payload.message, RefusalCode::EHeaders.reason());
        assert_eq!(payload.detail["file"].as_str(), Some("old.csv"));
        assert_eq!(payload.detail["issue"].as_str(), Some("duplicate"));
        assert_eq!(payload.detail["name"].as_str(), Some("u8:amount"));
    }

    #[test]
    fn dialect_is_actionable_when_next_command_provided() {
        let payload = RefusalPayload::dialect(
            "old.csv",
            vec!["0x2c".to_string(), "0x09".to_string()],
            Some("shape old.csv new.csv --delimiter tab --json".to_string()),
        );
        assert_eq!(payload.code, RefusalCode::EDialect);
        assert_eq!(payload.detail["file"].as_str(), Some("old.csv"));
        assert_eq!(payload.detail["candidates"][0].as_str(), Some("0x2c"));
        assert_eq!(
            payload.next_command.as_deref(),
            Some("shape old.csv new.csv --delimiter tab --json")
        );
    }

    #[test]
    fn ambiguous_profile_shape_matches_plan() {
        let payload = RefusalPayload::ambiguous_profile("profiles/a.json", "profile-a");
        assert_eq!(payload.code, RefusalCode::EAmbiguousProfile);
        assert_eq!(payload.message, RefusalCode::EAmbiguousProfile.reason());
        assert_eq!(
            payload.detail["profile_path"].as_str(),
            Some("profiles/a.json")
        );
        assert_eq!(payload.detail["profile_id"].as_str(), Some("profile-a"));
        assert_eq!(payload.next_command, None);
    }

    #[test]
    fn input_not_locked_shape_matches_plan() {
        let payload = RefusalPayload::input_not_locked("old.csv");
        assert_eq!(payload.code, RefusalCode::EInputNotLocked);
        assert_eq!(payload.message, RefusalCode::EInputNotLocked.reason());
        assert_eq!(payload.detail["file"].as_str(), Some("old.csv"));
    }

    #[test]
    fn input_drift_shape_matches_plan() {
        let payload = RefusalPayload::input_drift("old.csv", "sha256:expected", "sha256:actual");
        assert_eq!(payload.code, RefusalCode::EInputDrift);
        assert_eq!(payload.message, RefusalCode::EInputDrift.reason());
        assert_eq!(payload.detail["file"].as_str(), Some("old.csv"));
        assert_eq!(
            payload.detail["expected_hash"].as_str(),
            Some("sha256:expected")
        );
        assert_eq!(
            payload.detail["actual_hash"].as_str(),
            Some("sha256:actual")
        );
    }

    #[test]
    fn too_large_is_actionable_when_next_command_provided() {
        let payload = RefusalPayload::too_large(
            "old.csv",
            "--max-rows",
            10_000,
            50_000,
            Some("shape old.csv new.csv --max-rows 50000 --json".to_string()),
        );
        assert_eq!(payload.code, RefusalCode::ETooLarge);
        assert_eq!(payload.message, RefusalCode::ETooLarge.reason());
        assert_eq!(payload.detail["file"].as_str(), Some("old.csv"));
        assert_eq!(payload.detail["limit_flag"].as_str(), Some("--max-rows"));
        assert_eq!(payload.detail["limit"].as_u64(), Some(10_000));
        assert_eq!(payload.detail["actual"].as_u64(), Some(50_000));
        assert_eq!(
            payload.next_command.as_deref(),
            Some("shape old.csv new.csv --max-rows 50000 --json")
        );
    }

    #[test]
    fn non_actionable_code_ignores_next_command_setter() {
        let payload = RefusalPayload::io("old.csv", "permission denied")
            .with_next_command("shape old.csv new.csv --json");
        assert_eq!(payload.next_command, None);
    }

    #[test]
    fn dialect_next_command_builder_matches_plan_shape() {
        let next = RefusalPayload::build_next_command_for_dialect("old.csv", "new.csv", "tab");
        assert_eq!(next, "shape old.csv new.csv --delimiter tab --json");
    }

    #[test]
    fn too_large_next_command_builder_matches_plan_shape() {
        let next = RefusalPayload::build_next_command_for_too_large(
            "old.csv",
            "new.csv",
            "--max-rows",
            50_000,
        );
        assert_eq!(next, "shape old.csv new.csv --max-rows 50000 --json");
    }

    #[test]
    fn new_payload_starts_with_empty_object_detail() {
        let payload = RefusalPayload::from_code(RefusalCode::EIo);
        assert_eq!(payload.detail, serde_json::json!({}));
    }

    #[test]
    fn next_command_builders_quote_paths_with_spaces() {
        let next =
            RefusalPayload::build_next_command_for_dialect("old data.csv", "new data.csv", "tab");
        assert_eq!(
            next,
            "shape 'old data.csv' 'new data.csv' --delimiter tab --json"
        );
    }

    #[test]
    fn serialized_non_actionable_refusal_has_null_next_command() {
        let payload = RefusalPayload::empty("new.csv", 0);
        let encoded = serde_json::to_value(&payload).expect("serialize refusal payload");

        assert_eq!(encoded["code"].as_str(), Some("E_EMPTY"));
        assert_eq!(
            encoded["message"].as_str(),
            Some(RefusalCode::EEmpty.reason())
        );
        assert_eq!(encoded["detail"]["file"].as_str(), Some("new.csv"));
        assert_eq!(encoded["detail"]["rows"].as_u64(), Some(0));
        assert_eq!(encoded["next_command"], Value::Null);
    }

    #[test]
    fn serialized_actionable_refusal_has_non_null_next_command() {
        let payload = RefusalPayload::dialect(
            "old.csv",
            vec!["0x2c".to_string(), "0x09".to_string()],
            Some("shape old.csv new.csv --delimiter tab --json".to_string()),
        );
        let encoded = serde_json::to_value(&payload).expect("serialize refusal payload");

        assert_eq!(encoded["code"].as_str(), Some("E_DIALECT"));
        assert_eq!(
            encoded["message"].as_str(),
            Some(RefusalCode::EDialect.reason())
        );
        assert_eq!(
            encoded["next_command"].as_str(),
            Some("shape old.csv new.csv --delimiter tab --json")
        );
        assert!(encoded["detail"]["candidates"].is_array());
    }
}
