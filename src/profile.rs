use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::normalize::headers::ascii_trim;

#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub include_columns: Vec<Vec<u8>>,
    pub key_columns: Vec<Vec<u8>>,
    pub key_labels: Vec<String>,
    pub profile_id: Option<String>,
    pub profile_sha256: Option<String>,
}

impl ResolvedProfile {
    pub fn include_set(&self) -> HashSet<Vec<u8>> {
        self.include_columns.iter().cloned().collect()
    }

    pub fn primary_key(&self) -> Option<&[u8]> {
        self.key_columns.first().map(|value| value.as_slice())
    }
}

#[derive(Debug, Clone)]
pub enum ResolveError {
    NotFound { selector: String },
    Invalid { selector: String, error: String },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::NotFound { selector } => write!(f, "profile not found: {selector}"),
            ResolveError::Invalid { selector, error } => {
                write!(f, "invalid profile {selector}: {error}")
            }
        }
    }
}

impl std::error::Error for ResolveError {}

#[derive(Debug, Default)]
struct RawProfile {
    profile_id: Option<String>,
    profile_sha256: Option<String>,
    include_columns: Vec<String>,
    key: Vec<String>,
}

pub fn load_profile_from_path(path: &Path) -> Result<ResolvedProfile, ResolveError> {
    let selector = path.to_string_lossy().to_string();

    if path.is_dir() {
        return Err(ResolveError::Invalid {
            selector,
            error: "path is a directory, not a file".to_string(),
        });
    }

    let raw = fs::read_to_string(path).map_err(|err| ResolveError::Invalid {
        selector: selector.clone(),
        error: err.to_string(),
    })?;

    let parsed = parse_profile_yaml(&raw).map_err(|err| ResolveError::Invalid {
        selector: selector.clone(),
        error: err,
    })?;

    let mut include_columns = Vec::new();
    let mut include_seen = HashSet::new();
    for column in parsed.include_columns {
        if let Some(bytes) = parse_column_identifier(&column)
            && include_seen.insert(bytes.clone())
        {
            include_columns.push(bytes);
        }
    }

    let mut key_columns = Vec::new();
    let mut key_labels = Vec::new();
    for key in parsed.key {
        if let Some((bytes, label)) = parse_key_entry(&key) {
            key_columns.push(bytes);
            key_labels.push(label);
        }
    }

    Ok(ResolvedProfile {
        include_columns,
        key_columns,
        key_labels,
        profile_id: parsed.profile_id,
        profile_sha256: parsed.profile_sha256,
    })
}

pub fn resolve_profile_id(selector: &str) -> Result<ResolvedProfile, ResolveError> {
    let selector_path = Path::new(selector);
    if selector_path.exists() {
        return load_profile_from_path(selector_path);
    }

    let Some(search_root) = default_profile_dir() else {
        return Err(ResolveError::NotFound {
            selector: selector.to_string(),
        });
    };

    resolve_profile_id_in_directory(selector, &search_root)
}

fn default_profile_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".epistemic").join("profiles"))
}

fn parse_profile_yaml(raw: &str) -> Result<RawProfile, String> {
    let mut parsed = RawProfile::default();
    let lines: Vec<&str> = raw.lines().collect();
    let mut index = 0usize;
    while index < lines.len() {
        let line = strip_comment(lines[index]).trim();
        if line.is_empty() {
            index += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("profile_id:") {
            parsed.profile_id = parse_scalar(rest.trim());
            index += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("profile_sha256:") {
            parsed.profile_sha256 = parse_scalar(rest.trim());
            index += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("include_columns:") {
            let (items, consumed) = parse_list(rest.trim(), &lines[index + 1..]);
            parsed.include_columns = items;
            index += consumed + 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("key:") {
            let (items, consumed) = parse_list(rest.trim(), &lines[index + 1..]);
            parsed.key = items;
            index += consumed + 1;
            continue;
        }

        index += 1;
    }
    Ok(parsed)
}

fn parse_list(inline_value: &str, following_lines: &[&str]) -> (Vec<String>, usize) {
    if !inline_value.is_empty() {
        return (parse_inline_list(inline_value), 0);
    }

    let mut values = Vec::new();
    let mut consumed = 0usize;
    for raw_line in following_lines {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            consumed += 1;
            continue;
        }
        let Some(item) = line.strip_prefix('-') else {
            break;
        };
        if let Some(value) = parse_scalar(item.trim()) {
            values.push(value);
        }
        consumed += 1;
    }
    (values, consumed)
}

fn parse_inline_list(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    let inner = if trimmed.starts_with('[') && trimmed.ends_with(']') {
        &trimmed[1..trimmed.len().saturating_sub(1)]
    } else {
        trimmed
    };

    inner
        .split(',')
        .filter_map(|item| parse_scalar(item.trim()))
        .collect()
}

fn parse_scalar(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        return Some(value[1..value.len() - 1].to_string());
    }
    Some(value.to_string())
}

fn strip_comment(raw: &str) -> &str {
    raw.split('#').next().unwrap_or(raw)
}

/// Parse a column identifier as plain UTF-8 bytes with ASCII trimming.
///
/// Shape does not support u8:/hex: prefix parsing (unlike rvl) because shape
/// only works with header names, not cell byte values.
fn parse_column_identifier(raw: &str) -> Option<Vec<u8>> {
    let trimmed = ascii_trim(raw.as_bytes());
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_vec())
    }
}

fn parse_key_entry(raw: &str) -> Option<(Vec<u8>, String)> {
    let trimmed = ascii_trim(raw.as_bytes());
    if trimmed.is_empty() {
        return None;
    }

    let label = String::from_utf8_lossy(trimmed).to_string();
    Some((trimmed.to_vec(), label))
}

fn is_frozen_with_id(profile: &ResolvedProfile, selector: &str) -> bool {
    matches!(profile.profile_id.as_deref(), Some(id) if id == selector)
        && profile.profile_sha256.is_some()
}

fn resolve_profile_id_in_directory(
    selector: &str,
    directory: &Path,
) -> Result<ResolvedProfile, ResolveError> {
    let entries = fs::read_dir(directory).map_err(|_| ResolveError::NotFound {
        selector: selector.to_string(),
    })?;

    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("yaml"))
        .collect();
    paths.sort();

    for path in paths {
        let Ok(profile) = load_profile_from_path(&path) else {
            continue;
        };
        if is_frozen_with_id(&profile, selector) {
            return Ok(profile);
        }
    }

    Err(ResolveError::NotFound {
        selector: selector.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("shape_test_profile_{id}_{seq}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // 1. loads_draft_profile_from_path
    #[test]
    fn loads_draft_profile_from_path() {
        let dir = temp_dir();
        let path = dir.join("draft.yaml");
        std::fs::write(
            &path,
            r#"
include_columns:
  - loan_id
  - balance
key: [loan_id]
"#,
        )
        .unwrap();

        let profile = load_profile_from_path(&path).expect("profile should load");
        assert_eq!(profile.include_columns.len(), 2);
        assert_eq!(profile.include_columns[0], b"loan_id");
        assert_eq!(profile.include_columns[1], b"balance");
        assert_eq!(profile.primary_key(), Some(b"loan_id".as_slice()));
        assert!(profile.profile_id.is_none());
        assert!(profile.profile_sha256.is_none());

        std::fs::remove_dir_all(dir).ok();
    }

    // 2. loads_frozen_profile_with_id_and_sha
    #[test]
    fn loads_frozen_profile_with_id_and_sha() {
        let dir = temp_dir();
        let path = dir.join("frozen.yaml");
        std::fs::write(
            &path,
            r#"
profile_id: loan-tape.v0
profile_sha256: sha256:abcdef1234567890
include_columns:
  - loan_id
  - balance
  - rate
key:
  - loan_id
  - begin_date
"#,
        )
        .unwrap();

        let profile = load_profile_from_path(&path).expect("profile should load");
        assert_eq!(profile.profile_id.as_deref(), Some("loan-tape.v0"));
        assert_eq!(
            profile.profile_sha256.as_deref(),
            Some("sha256:abcdef1234567890")
        );
        assert_eq!(profile.include_columns.len(), 3);
        assert_eq!(profile.key_columns.len(), 2);
        assert_eq!(profile.key_labels, vec!["loan_id", "begin_date"]);

        std::fs::remove_dir_all(dir).ok();
    }

    // 3. resolves_frozen_profile_by_id_from_directory
    #[test]
    fn resolves_frozen_profile_by_id_from_directory() {
        let dir = temp_dir();
        std::fs::write(
            dir.join("first.yaml"),
            r#"
profile_id: csv.demo.v0
profile_sha256: sha256:abc
include_columns: [loan_id, balance]
key: [loan_id]
"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("second.yaml"),
            r#"
profile_id: csv.other.v0
include_columns: [loan_id]
key: [loan_id]
"#,
        )
        .unwrap();

        let resolved =
            resolve_profile_id_in_directory("csv.demo.v0", &dir).expect("should resolve");
        assert_eq!(resolved.profile_id.as_deref(), Some("csv.demo.v0"));
        assert_eq!(resolved.profile_sha256.as_deref(), Some("sha256:abc"));
        assert_eq!(resolved.include_columns.len(), 2);

        std::fs::remove_dir_all(dir).ok();
    }

    // 4. resolve_rejects_unfrozen_profile_by_id
    #[test]
    fn resolve_rejects_unfrozen_profile_by_id() {
        let dir = temp_dir();
        std::fs::write(
            dir.join("unfrozen.yaml"),
            r#"
profile_id: csv.draft.v0
include_columns: [loan_id]
"#,
        )
        .unwrap();

        let result = resolve_profile_id_in_directory("csv.draft.v0", &dir);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ResolveError::NotFound { .. }));

        std::fs::remove_dir_all(dir).ok();
    }

    // 5. resolve_returns_not_found_for_missing_id
    #[test]
    fn resolve_returns_not_found_for_missing_id() {
        let dir = temp_dir();
        std::fs::write(
            dir.join("other.yaml"),
            r#"
profile_id: csv.other.v0
profile_sha256: sha256:xyz
include_columns: [loan_id]
"#,
        )
        .unwrap();

        let result = resolve_profile_id_in_directory("csv.nonexistent.v0", &dir);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ResolveError::NotFound { .. }));

        std::fs::remove_dir_all(dir).ok();
    }

    // 6. parse_handles_inline_list_syntax
    #[test]
    fn parse_handles_inline_list_syntax() {
        let dir = temp_dir();
        let path = dir.join("inline.yaml");
        std::fs::write(
            &path,
            "include_columns: [loan_id, balance, rate]\nkey: [loan_id]\n",
        )
        .unwrap();

        let profile = load_profile_from_path(&path).expect("profile should load");
        assert_eq!(profile.include_columns.len(), 3);
        assert_eq!(profile.include_columns[0], b"loan_id");
        assert_eq!(profile.include_columns[1], b"balance");
        assert_eq!(profile.include_columns[2], b"rate");

        std::fs::remove_dir_all(dir).ok();
    }

    // 7. parse_handles_quoted_values
    #[test]
    fn parse_handles_quoted_values() {
        let dir = temp_dir();
        let path = dir.join("quoted.yaml");
        std::fs::write(
            &path,
            r#"
include_columns:
  - "loan_id"
  - 'balance'
  - rate
"#,
        )
        .unwrap();

        let profile = load_profile_from_path(&path).expect("profile should load");
        assert_eq!(profile.include_columns.len(), 3);
        assert_eq!(profile.include_columns[0], b"loan_id");
        assert_eq!(profile.include_columns[1], b"balance");
        assert_eq!(profile.include_columns[2], b"rate");

        std::fs::remove_dir_all(dir).ok();
    }

    // 8. parse_ignores_comments_and_blank_lines
    #[test]
    fn parse_ignores_comments_and_blank_lines() {
        let dir = temp_dir();
        let path = dir.join("comments.yaml");
        std::fs::write(
            &path,
            r#"
# This is a comment
profile_id: test.v0

include_columns:
  - loan_id  # inline comment
  - balance

# Another comment
key: [loan_id]
"#,
        )
        .unwrap();

        let profile = load_profile_from_path(&path).expect("profile should load");
        assert_eq!(profile.profile_id.as_deref(), Some("test.v0"));
        assert_eq!(profile.include_columns.len(), 2);
        assert_eq!(profile.include_columns[0], b"loan_id");
        assert_eq!(profile.include_columns[1], b"balance");

        std::fs::remove_dir_all(dir).ok();
    }

    // 9. parse_deduplicates_include_columns
    #[test]
    fn parse_deduplicates_include_columns() {
        let dir = temp_dir();
        let path = dir.join("dedup.yaml");
        std::fs::write(
            &path,
            r#"
include_columns:
  - loan_id
  - balance
  - loan_id
  - balance
  - rate
"#,
        )
        .unwrap();

        let profile = load_profile_from_path(&path).expect("profile should load");
        assert_eq!(profile.include_columns.len(), 3);
        assert_eq!(profile.include_columns[0], b"loan_id");
        assert_eq!(profile.include_columns[1], b"balance");
        assert_eq!(profile.include_columns[2], b"rate");

        std::fs::remove_dir_all(dir).ok();
    }

    // 10. invalid_path_returns_error
    #[test]
    fn invalid_path_returns_error() {
        let result = load_profile_from_path(Path::new("/nonexistent/path/profile.yaml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ResolveError::Invalid { .. }));
    }

    // 11. profile_path_is_directory_not_file
    #[test]
    fn profile_path_is_directory_not_file() {
        let dir = temp_dir();
        let result = load_profile_from_path(&dir);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            ResolveError::Invalid { error, .. } => {
                assert!(
                    error.contains("directory"),
                    "error should mention directory: {error}"
                );
            }
            other => panic!("expected ResolveError::Invalid, got: {other:?}"),
        }

        std::fs::remove_dir_all(dir).ok();
    }

    // include_set returns correct HashSet
    #[test]
    fn include_set_returns_hashset_of_column_bytes() {
        let profile = ResolvedProfile {
            include_columns: vec![b"loan_id".to_vec(), b"balance".to_vec()],
            key_columns: vec![],
            key_labels: vec![],
            profile_id: None,
            profile_sha256: None,
        };
        let set = profile.include_set();
        assert_eq!(set.len(), 2);
        assert!(set.contains(b"loan_id".as_slice()));
        assert!(set.contains(b"balance".as_slice()));
    }
}
