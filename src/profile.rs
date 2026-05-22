use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::normalize::headers::{ascii_trim, canonicalize_header_identifier};

const COLUMN_NAME_CANONICAL_TYPE: &str = "column_name";

#[derive(Debug, Deserialize)]
struct MappingEntry {
    input: String,
    canonical_id: String,
    canonical_type: String,
    rule_id: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub include_columns: Vec<Vec<u8>>,
    pub key_columns: Vec<Vec<u8>>,
    pub key_labels: Vec<String>,
    pub profile_id: Option<String>,
    pub profile_sha256: Option<String>,
    pub column_registry: Option<String>,
    pub source_path: PathBuf,
    pub resolved_registry_path: Option<PathBuf>,
    pub column_aliases: Option<HashMap<Vec<u8>, Vec<u8>>>,
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
    column_registry: Option<String>,
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

    let column_registry = parsed.column_registry.clone();
    let (column_aliases, resolved_registry_path) = match column_registry.as_deref() {
        Some(registry_ref) => {
            let registry_path = resolve_registry_path(path, registry_ref);
            let aliases = load_column_registry_aliases(&registry_path).map_err(|error| {
                ResolveError::Invalid {
                    selector: selector.clone(),
                    error,
                }
            })?;
            (Some(aliases), Some(registry_path))
        }
        None => (None, None),
    };

    let mut include_columns = Vec::new();
    let mut include_seen = HashSet::new();
    for column in parsed.include_columns {
        if let Some(bytes) = parse_column_identifier(&column) {
            let canonical = canonicalize_header_identifier(&bytes, column_aliases.as_ref());
            if include_seen.insert(canonical.clone()) {
                include_columns.push(canonical);
            }
        }
    }

    let mut key_columns = Vec::new();
    let mut key_labels = Vec::new();
    for key in parsed.key {
        if let Some((bytes, label)) = parse_key_entry(&key, column_aliases.as_ref()) {
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
        column_registry,
        source_path: path.to_path_buf(),
        resolved_registry_path,
        column_aliases,
    })
}

pub fn resolve_profile_id(selector: &str) -> Result<ResolvedProfile, ResolveError> {
    let selector_path = Path::new(selector);
    if selector_path.exists() {
        return load_profile_from_path(selector_path);
    }

    let search_root =
        crate::paths::profile_dir_for_read().map_err(|error| ResolveError::Invalid {
            selector: selector.to_string(),
            error,
        })?;

    resolve_profile_id_in_directory(selector, &search_root)
}

pub fn render_profile_yaml(profile: &ResolvedProfile) -> String {
    let mut out = String::new();
    if let Some(profile_id) = profile.profile_id.as_deref() {
        out.push_str("profile_id: ");
        out.push_str(profile_id);
        out.push('\n');
    }
    if let Some(profile_sha256) = profile.profile_sha256.as_deref() {
        out.push_str("profile_sha256: ");
        out.push_str(profile_sha256);
        out.push('\n');
    }
    if let Some(column_registry) = profile.column_registry.as_deref() {
        out.push_str("column_registry: ");
        out.push_str(column_registry);
        out.push('\n');
    }
    out.push_str("include_columns:\n");
    for column in &profile.include_columns {
        out.push_str("  - ");
        out.push_str(String::from_utf8_lossy(column).as_ref());
        out.push('\n');
    }
    out.push_str("key:\n");
    if profile.key_labels.is_empty() {
        for key in &profile.key_columns {
            out.push_str("  - ");
            out.push_str(String::from_utf8_lossy(key).as_ref());
            out.push('\n');
        }
    } else {
        for key in &profile.key_labels {
            out.push_str("  - ");
            out.push_str(key);
            out.push('\n');
        }
    }
    out
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
        if let Some(rest) = line.strip_prefix("column_registry:") {
            parsed.column_registry = parse_scalar(rest.trim());
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

fn resolve_registry_path(anchor_path: &Path, registry_ref: &str) -> PathBuf {
    let registry_path = Path::new(registry_ref);
    if registry_path.is_absolute() {
        registry_path.to_path_buf()
    } else {
        anchor_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(registry_path)
    }
}

fn load_column_registry_aliases(registry_dir: &Path) -> Result<HashMap<Vec<u8>, Vec<u8>>, String> {
    if !registry_dir.exists() || !registry_dir.is_dir() {
        return Err(format!(
            "registry directory not found: {}",
            registry_dir.display()
        ));
    }

    let registry_json_path = registry_dir.join("registry.json");
    let registry_json = fs::read_to_string(&registry_json_path)
        .map_err(|error| format!("{}: {error}", registry_json_path.display()))?;
    serde_json::from_str::<serde_json::Value>(&registry_json).map_err(|error| {
        format!(
            "failed to parse registry definition '{}': {error}",
            registry_json_path.display()
        )
    })?;

    let mut mapping_paths = fs::read_dir(registry_dir)
        .map_err(|error| format!("{}: {error}", registry_dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.is_file()
                && path.extension().is_some_and(|ext| ext == "json")
                && path.file_name() != Some("registry.json".as_ref())
                && path.file_name() != Some("_build.json".as_ref())
        })
        .collect::<Vec<_>>();
    mapping_paths.sort();

    let mut aliases = HashMap::new();
    for path in mapping_paths {
        let content =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let entries: Vec<MappingEntry> = serde_json::from_str(&content).map_err(|error| {
            format!("failed to parse mapping file '{}': {error}", path.display())
        })?;

        for (index, entry) in entries.into_iter().enumerate() {
            if entry.input.trim().is_empty()
                || entry.canonical_id.trim().is_empty()
                || entry.canonical_type.trim().is_empty()
                || entry.rule_id.trim().is_empty()
            {
                return Err(format!(
                    "invalid mapping entry {index} in '{}': missing required fields",
                    path.display()
                ));
            }

            if entry.canonical_type == COLUMN_NAME_CANONICAL_TYPE {
                aliases
                    .entry(entry.input.into_bytes())
                    .or_insert(entry.canonical_id.into_bytes());
            }
        }
    }

    Ok(aliases)
}

fn strip_comment(raw: &str) -> &str {
    raw.split('#').next().unwrap_or(raw)
}

/// Parse a column identifier as plain UTF-8 bytes with ASCII trimming.
///
fn parse_column_identifier(raw: &str) -> Option<Vec<u8>> {
    let trimmed = ascii_trim(raw.as_bytes());
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_vec())
    }
}

fn parse_key_entry(
    raw: &str,
    aliases: Option<&HashMap<Vec<u8>, Vec<u8>>>,
) -> Option<(Vec<u8>, String)> {
    let bytes = parse_column_identifier(raw)?;
    let canonical = canonicalize_header_identifier(&bytes, aliases);
    if canonical.is_empty() {
        return None;
    }

    let label = String::from_utf8_lossy(&canonical).to_string();
    Some((canonical, label))
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

    fn write_registry_fixture(dir: &Path) -> PathBuf {
        let registry_dir = dir.join("registry");
        std::fs::create_dir_all(&registry_dir).unwrap();
        std::fs::write(
            registry_dir.join("registry.json"),
            r#"{"id":"annex-columns-v0","version":"1.0.0"}"#,
        )
        .unwrap();
        std::fs::write(
            registry_dir.join("aliases.json"),
            r#"
[
  {
    "input": "Loan Number",
    "canonical_id": "loan_id_number",
    "canonical_type": "column_name",
    "rule_id": "ANNEX_COLUMN_ALIAS"
  },
  {
    "input": "Current Balance",
    "canonical_id": "current_balance",
    "canonical_type": "column_name",
    "rule_id": "ANNEX_COLUMN_ALIAS"
  }
]
"#,
        )
        .unwrap();
        registry_dir
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

    #[test]
    fn loads_profile_with_column_registry_and_canonicalizes_columns() {
        let dir = temp_dir();
        let registry_dir = write_registry_fixture(&dir);
        let path = dir.join("registry_profile.yaml");
        std::fs::write(
            &path,
            r#"
profile_id: loan-tape.v0
profile_sha256: sha256:test
column_registry: registry
include_columns:
  - Loan Number
  - Current Balance
  - Current Balance
key:
  - Loan Number
"#,
        )
        .unwrap();

        let profile = load_profile_from_path(&path).expect("profile should load");
        assert_eq!(profile.profile_id.as_deref(), Some("loan-tape.v0"));
        assert_eq!(profile.profile_sha256.as_deref(), Some("sha256:test"));
        assert_eq!(profile.column_registry.as_deref(), Some("registry"));
        assert_eq!(profile.source_path, path);
        assert_eq!(profile.resolved_registry_path, Some(registry_dir));
        assert_eq!(
            profile.include_columns,
            vec![b"loan_id_number".to_vec(), b"current_balance".to_vec()]
        );
        assert_eq!(profile.key_columns, vec![b"loan_id_number".to_vec()]);
        assert_eq!(profile.key_labels, vec!["loan_id_number".to_string()]);
        assert!(profile.column_aliases.is_some());

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn invalid_column_registry_returns_invalid_error() {
        let dir = temp_dir();
        let path = dir.join("bad_registry.yaml");
        std::fs::write(
            &path,
            "column_registry: missing\ninclude_columns: [loan_id]\n",
        )
        .unwrap();

        let err = load_profile_from_path(&path).expect_err("missing registry should fail");
        let error = match err {
            ResolveError::Invalid { error, .. } => error,
            ResolveError::NotFound { .. } => String::new(),
        };
        assert!(error.contains("registry directory not found"));

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
        assert!(
            matches!(err, ResolveError::Invalid { .. }),
            "expected ResolveError::Invalid, got: {err:?}"
        );
        let ResolveError::Invalid { error, .. } = &err else {
            return;
        };
        assert!(
            error.contains("directory"),
            "error should mention directory: {error}"
        );

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
            column_registry: None,
            source_path: PathBuf::new(),
            resolved_registry_path: None,
            column_aliases: None,
        };
        let set = profile.include_set();
        assert_eq!(set.len(), 2);
        assert!(set.contains(b"loan_id".as_slice()));
        assert!(set.contains(b"balance".as_slice()));
    }

    #[test]
    fn rendered_profile_round_trips_through_loader() {
        let dir = temp_dir();
        let path = dir.join("rendered.yaml");
        let profile = ResolvedProfile {
            include_columns: vec![b"loan_id".to_vec(), b"balance".to_vec(), b"rate".to_vec()],
            key_columns: vec![b"loan_id".to_vec(), b"as_of_date".to_vec()],
            key_labels: vec!["loan_id".to_string(), "as_of_date".to_string()],
            profile_id: Some("csv.demo.v0".to_string()),
            profile_sha256: Some("sha256:abc123".to_string()),
            column_registry: None,
            source_path: path.clone(),
            resolved_registry_path: None,
            column_aliases: None,
        };
        std::fs::write(&path, render_profile_yaml(&profile)).unwrap();

        let loaded = load_profile_from_path(&path).expect("rendered profile should load");
        assert_eq!(loaded.include_columns, profile.include_columns);
        assert_eq!(loaded.key_columns, profile.key_columns);
        assert_eq!(loaded.key_labels, profile.key_labels);
        assert_eq!(loaded.profile_id, profile.profile_id);
        assert_eq!(loaded.profile_sha256, profile.profile_sha256);
        assert_eq!(loaded.column_registry, profile.column_registry);

        std::fs::remove_dir_all(dir).ok();
    }
}
