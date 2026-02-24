use std::ffi::OsStr;
use std::panic::panic_any;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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
    let path = fixture_path(name);
    match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => panic_any(format!(
            "failed to read fixture {name} ({}): {error}",
            path.display()
        )),
    }
}

/// Captured process output from invoking the `shape` binary.
#[derive(Debug)]
pub struct ShapeInvocation {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl ShapeInvocation {
    /// Stdout with trailing newlines trimmed for stable assertions.
    #[allow(dead_code)]
    pub fn stdout_trimmed(&self) -> &str {
        self.stdout.trim_end()
    }
}

/// Run the compiled `shape` binary with arbitrary CLI args.
#[allow(dead_code)]
pub fn run_shape<I, S>(args: I) -> ShapeInvocation
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(env!("CARGO_BIN_EXE_shape"));
    command.args(args);
    execute_shape_command(&mut command)
}

/// Run the compiled `shape` binary with arbitrary CLI args and extra env vars.
#[allow(dead_code)]
pub fn run_shape_with_env(args: &[&str], env_vars: &[(&str, &str)]) -> ShapeInvocation {
    let mut command = Command::new(env!("CARGO_BIN_EXE_shape"));
    command.args(args);
    for (key, value) in env_vars {
        command.env(key, value);
    }
    execute_shape_command(&mut command)
}

/// Assert that a display-mode invocation exits successfully and writes only stdout.
#[allow(dead_code)]
pub fn assert_display_stdout_only(args: &[&str], expected_stdout_fragment: &str) {
    let result = run_shape(args);
    assert_eq!(result.status, 0, "args={args:?}");
    assert!(
        result.stderr.trim().is_empty(),
        "display mode should not write stderr for args {args:?}: {}",
        result.stderr
    );
    assert!(
        result.stdout.contains(expected_stdout_fragment),
        "stdout should contain {expected_stdout_fragment:?} for args {args:?}: {}",
        result.stdout
    );
}

/// Reuse one display-only assertion flow across table-driven cases.
#[allow(dead_code)]
pub struct DisplayCase<'a> {
    pub args: &'a [&'a str],
    pub expected_stdout: &'a str,
}

/// Assert display-mode behavior for a full matrix of cases.
#[allow(dead_code)]
pub fn assert_display_stdout_only_matrix(cases: &[DisplayCase<'_>]) {
    for case in cases {
        assert_display_stdout_only(case.args, case.expected_stdout);
    }
}

/// Matching modes for display-mode stdout assertions.
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub enum StdoutMatch<'a> {
    Contains(&'a str),
    StartsWith(&'a str),
}

/// Assert display-mode behavior with configurable stdout matching semantics.
#[allow(dead_code)]
pub fn assert_display_stdout_only_with_match(args: &[&str], expected: StdoutMatch<'_>) {
    let result = run_shape(args);
    assert_eq!(result.status, 0, "args={args:?}");
    assert!(
        result.stderr.trim().is_empty(),
        "display mode should not write stderr for args {args:?}: {}",
        result.stderr
    );
    match expected {
        StdoutMatch::Contains(fragment) => assert!(
            result.stdout.contains(fragment),
            "stdout should contain {fragment:?} for args {args:?}: {}",
            result.stdout
        ),
        StdoutMatch::StartsWith(prefix) => assert!(
            result.stdout.starts_with(prefix),
            "stdout should start with {prefix:?} for args {args:?}: {}",
            result.stdout
        ),
    }
}

/// Reuse one display-match assertion flow across table-driven cases.
#[allow(dead_code)]
pub struct DisplayMatchCase<'a> {
    pub args: &'a [&'a str],
    pub expected_stdout: StdoutMatch<'a>,
}

/// Assert display-mode behavior with configurable match semantics for a full matrix of cases.
#[allow(dead_code)]
pub fn assert_display_stdout_only_with_match_matrix(cases: &[DisplayMatchCase<'_>]) {
    for case in cases {
        assert_display_stdout_only_with_match(case.args, case.expected_stdout);
    }
}

/// Assert that a process-level CLI parse failure keeps stdout empty and writes stderr.
#[allow(dead_code)]
pub fn assert_parse_failure_routes_to_stderr(args: &[&str], expected_stderr_fragment: &str) {
    let result = run_shape(args);
    assert_eq!(result.status, 2, "args={args:?}");
    assert!(
        result.stdout.trim().is_empty(),
        "process-level parse errors should not write stdout for args {args:?}: {}",
        result.stdout
    );
    assert!(
        result.stderr.contains(expected_stderr_fragment),
        "stderr should contain {expected_stderr_fragment:?} for args {args:?}: {}",
        result.stderr
    );
}

/// Assert that a process-level parse failure with fixture inputs routes to stderr.
#[allow(dead_code)]
pub fn assert_parse_failure_with_fixtures_routes_to_stderr(
    old_fixture: &str,
    new_fixture: &str,
    extra_args: &[&str],
    expected_stderr_fragments: &[&str],
) {
    let result = run_shape_with_fixtures(old_fixture, new_fixture, extra_args);
    assert_eq!(result.status, 2, "args={extra_args:?}");
    assert!(
        result.stdout.trim().is_empty(),
        "process-level parse errors should not write stdout for args {extra_args:?}: {}",
        result.stdout
    );
    for fragment in expected_stderr_fragments {
        assert!(
            result.stderr.contains(fragment),
            "stderr should contain {fragment:?} for args {extra_args:?}: {}",
            result.stderr
        );
    }
}

/// Reuse one parse-failure assertion flow across table-driven cases.
#[allow(dead_code)]
pub struct ParseFailureCase<'a> {
    pub args: &'a [&'a str],
    pub expected_stderr_fragment: &'a str,
}

/// Assert process-level parse-failure routing for a full matrix of cases.
#[allow(dead_code)]
pub fn assert_parse_failure_routes_to_stderr_matrix(cases: &[ParseFailureCase<'_>]) {
    for case in cases {
        assert_parse_failure_routes_to_stderr(case.args, case.expected_stderr_fragment);
    }
}

/// Run `shape` against two fixture files plus extra args.
#[allow(dead_code)]
pub fn run_shape_with_fixtures(
    old_fixture: &str,
    new_fixture: &str,
    extra_args: &[&str],
) -> ShapeInvocation {
    let old = fixture_path(old_fixture);
    let new = fixture_path(new_fixture);

    let mut command = Command::new(env!("CARGO_BIN_EXE_shape"));
    command.arg(old).arg(new).args(extra_args);
    execute_shape_command(&mut command)
}

fn execute_shape_command(command: &mut Command) -> ShapeInvocation {
    let output = command.output().expect("failed to execute shape binary");
    shape_invocation_from_output(output)
}

fn shape_invocation_from_output(output: Output) -> ShapeInvocation {
    ShapeInvocation {
        status: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}
