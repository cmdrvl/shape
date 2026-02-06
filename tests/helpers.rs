use std::path::{Path, PathBuf};

/// Root directory for test fixtures.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Path to a specific fixture file.
#[allow(dead_code)]
pub fn fixture_path(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

/// Read a fixture file to a string.
#[allow(dead_code)]
pub fn read_fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|e| panic!("failed to read fixture {name}: {e}"))
}
