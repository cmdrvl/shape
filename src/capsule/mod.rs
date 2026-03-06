use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

mod min_repro;

use self::min_repro::{ReproLimits, extract_minimal_repro};
use crate::checks::suite::Outcome;
use crate::cli::args::Args;
use crate::profile::{ResolvedProfile, render_profile_yaml};
use crate::refusal::payload::RefusalPayload;
use crate::witness::hash::hash_bytes;

const MANIFEST_FILENAME: &str = "manifest.json";
const OLD_ARTIFACT_PATH: &str = "inputs/old.csv";
const NEW_ARTIFACT_PATH: &str = "inputs/new.csv";
const OUTPUT_ARTIFACT_PATH: &str = "outputs/report.txt";
const PROFILE_ARTIFACT_PATH: &str = "profile.yaml";

#[derive(Debug, Serialize)]
struct CapsuleManifest {
    schema_version: &'static str,
    tool: CapsuleTool,
    args: CapsuleArgs,
    result: CapsuleResult,
    replay: CapsuleReplay,
    artifacts: Vec<CapsuleArtifact>,
}

#[derive(Debug, Serialize)]
struct CapsuleTool {
    name: &'static str,
    version: &'static str,
}

#[derive(Debug, Serialize)]
struct CapsuleArgs {
    old: Option<String>,
    new: Option<String>,
    key: Option<String>,
    delimiter: Option<String>,
    json: bool,
    no_witness: bool,
    profile: Option<String>,
    profile_id: Option<String>,
    lock: Vec<String>,
    max_rows: Option<u64>,
    max_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
struct CapsuleResult {
    outcome: &'static str,
    refusal: Option<CapsuleRefusal>,
}

#[derive(Debug, Serialize)]
struct CapsuleRefusal {
    code: &'static str,
    message: String,
    detail: Value,
    next_command: Option<String>,
}

#[derive(Debug, Serialize)]
struct CapsuleReplay {
    argv: Vec<String>,
    shell: String,
}

#[derive(Debug, Serialize)]
struct CapsuleArtifact {
    name: &'static str,
    path: &'static str,
    source_path: Option<String>,
    bytes: Option<u64>,
    blake3: Option<String>,
    source_error: Option<String>,
}

/// Write deterministic capsule artifacts and manifest for one shape run.
pub fn write_run_capsule(
    args: &Args,
    outcome: Outcome,
    output: &str,
    refusal: Option<&RefusalPayload>,
    resolved_profile: Option<&ResolvedProfile>,
    capsule_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(capsule_dir)?;
    let repro_limits = ReproLimits::from_optional(args.max_rows, args.max_bytes);
    let refusal_code = refusal.map(|payload| payload.code.as_str());

    let mut artifacts = vec![
        write_input_artifact(
            capsule_dir,
            "old_input",
            OLD_ARTIFACT_PATH,
            args.old.as_deref(),
            outcome,
            refusal_code,
            repro_limits,
        )?,
        write_input_artifact(
            capsule_dir,
            "new_input",
            NEW_ARTIFACT_PATH,
            args.new.as_deref(),
            outcome,
            refusal_code,
            repro_limits,
        )?,
        write_output_artifact(capsule_dir, output)?,
    ];
    if let Some(profile) = resolved_profile {
        artifacts.push(write_profile_artifact(
            capsule_dir,
            profile,
            args.profile.as_deref(),
        )?);
    }

    let replay_argv = build_replay_argv(args, resolved_profile.is_some());
    let manifest = CapsuleManifest {
        schema_version: "shape.capsule.v0",
        tool: CapsuleTool {
            name: "shape",
            version: env!("CARGO_PKG_VERSION"),
        },
        args: CapsuleArgs {
            old: args.old.as_ref().map(|path| path_to_string(path.as_path())),
            new: args.new.as_ref().map(|path| path_to_string(path.as_path())),
            key: args.key.clone(),
            delimiter: args.delimiter.clone(),
            json: args.json,
            no_witness: args.no_witness,
            profile: args
                .profile
                .as_ref()
                .map(|path| path_to_string(path.as_path())),
            profile_id: args.profile_id.clone(),
            lock: args
                .lock
                .iter()
                .map(|path| path_to_string(path.as_path()))
                .collect(),
            max_rows: args.max_rows,
            max_bytes: args.max_bytes,
        },
        result: CapsuleResult {
            outcome: outcome_label(outcome),
            refusal: refusal.map(|payload| CapsuleRefusal {
                code: payload.code.as_str(),
                message: payload.message.clone(),
                detail: payload.detail.clone(),
                next_command: payload.next_command.clone(),
            }),
        },
        replay: CapsuleReplay {
            shell: replay_argv
                .iter()
                .map(|part| shell_quote(part))
                .collect::<Vec<_>>()
                .join(" "),
            argv: replay_argv,
        },
        artifacts,
    };

    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    fs::write(capsule_dir.join(MANIFEST_FILENAME), bytes)?;
    Ok(())
}

fn write_input_artifact(
    capsule_dir: &Path,
    name: &'static str,
    artifact_relative_path: &'static str,
    source_path: Option<&Path>,
    outcome: Outcome,
    refusal_code: Option<&str>,
    repro_limits: ReproLimits,
) -> Result<CapsuleArtifact, std::io::Error> {
    let destination = capsule_dir.join(artifact_relative_path);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    let source_display = source_path.map(path_to_string);
    let result = source_path.map(fs::read);

    let (bytes, blake3, source_error) = match result {
        Some(Ok(content)) => {
            let minimal = extract_minimal_repro(&content, outcome, refusal_code, repro_limits);
            fs::write(&destination, &minimal.bytes)?;
            (
                Some(minimal.bytes.len() as u64),
                Some(format!("blake3:{}", hash_bytes(&minimal.bytes))),
                None,
            )
        }
        Some(Err(error)) => {
            remove_file_if_exists(&destination)?;
            (None, None, Some(error.to_string()))
        }
        None => {
            remove_file_if_exists(&destination)?;
            (None, None, Some("missing source path argument".to_owned()))
        }
    };

    Ok(CapsuleArtifact {
        name,
        path: artifact_relative_path,
        source_path: source_display,
        bytes,
        blake3,
        source_error,
    })
}

fn write_output_artifact(
    capsule_dir: &Path,
    output: &str,
) -> Result<CapsuleArtifact, std::io::Error> {
    let output_path = capsule_dir.join(OUTPUT_ARTIFACT_PATH);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, output.as_bytes())?;

    Ok(CapsuleArtifact {
        name: "render_output",
        path: OUTPUT_ARTIFACT_PATH,
        source_path: None,
        bytes: Some(output.len() as u64),
        blake3: Some(format!("blake3:{}", hash_bytes(output.as_bytes()))),
        source_error: None,
    })
}

fn write_profile_artifact(
    capsule_dir: &Path,
    profile: &ResolvedProfile,
    source_path: Option<&Path>,
) -> Result<CapsuleArtifact, std::io::Error> {
    let profile_path = capsule_dir.join(PROFILE_ARTIFACT_PATH);
    fs::write(&profile_path, render_profile_yaml(profile).as_bytes())?;

    let bytes = fs::read(&profile_path)?;
    Ok(CapsuleArtifact {
        name: "profile",
        path: PROFILE_ARTIFACT_PATH,
        source_path: source_path.map(path_to_string),
        bytes: Some(bytes.len() as u64),
        blake3: Some(format!("blake3:{}", hash_bytes(&bytes))),
        source_error: None,
    })
}

fn remove_file_if_exists(path: &Path) -> Result<(), std::io::Error> {
    match fs::remove_file(path) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn build_replay_argv(args: &Args, use_local_profile: bool) -> Vec<String> {
    let mut argv = vec![
        "shape".to_owned(),
        OLD_ARTIFACT_PATH.to_owned(),
        NEW_ARTIFACT_PATH.to_owned(),
    ];

    if let Some(key) = args.key.as_ref() {
        argv.push("--key".to_owned());
        argv.push(key.clone());
    }
    if let Some(delimiter) = args.delimiter.as_ref() {
        argv.push("--delimiter".to_owned());
        argv.push(delimiter.clone());
    }
    if args.json {
        argv.push("--json".to_owned());
    }
    if args.no_witness {
        argv.push("--no-witness".to_owned());
    }
    if use_local_profile {
        argv.push("--profile".to_owned());
        argv.push(PROFILE_ARTIFACT_PATH.to_owned());
    } else if let Some(profile) = args.profile.as_ref() {
        argv.push("--profile".to_owned());
        argv.push(path_to_string(profile));
    } else if let Some(profile_id) = args.profile_id.as_ref() {
        argv.push("--profile-id".to_owned());
        argv.push(profile_id.clone());
    }
    for lock in &args.lock {
        argv.push("--lock".to_owned());
        argv.push(path_to_string(lock));
    }
    if let Some(max_rows) = args.max_rows {
        argv.push("--max-rows".to_owned());
        argv.push(max_rows.to_string());
    }
    if let Some(max_bytes) = args.max_bytes {
        argv.push("--max-bytes".to_owned());
        argv.push(max_bytes.to_string());
    }

    argv
}

fn shell_quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-' | b':'))
    {
        return arg.to_owned();
    }

    let escaped = arg.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn outcome_label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Compatible => "COMPATIBLE",
        Outcome::Incompatible => "INCOMPATIBLE",
        Outcome::Refusal => "REFUSAL",
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::Value;

    use super::{MANIFEST_FILENAME, NEW_ARTIFACT_PATH, OLD_ARTIFACT_PATH, write_run_capsule};
    use crate::checks::suite::Outcome;
    use crate::cli::args::Args;
    use crate::refusal::payload::RefusalPayload;

    fn unique_dir(label: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "shape-capsule-{label}-{}-{seq}-{nanos}",
            std::process::id(),
        ))
    }

    fn make_args(old: PathBuf, new: PathBuf) -> Args {
        Args {
            old: Some(old),
            new: Some(new),
            key: Some("loan_id".to_owned()),
            delimiter: Some("comma".to_owned()),
            json: true,
            no_witness: false,
            capsule_dir: None,
            profile: None,
            profile_id: Some("monthly".to_owned()),
            lock: vec![PathBuf::from("shape.lock")],
            max_rows: Some(100),
            max_bytes: Some(4096),
            explicit: false,
            schema: false,
            describe: false,
            command: None,
        }
    }

    fn parse_manifest(path: &Path) -> Value {
        let text = std::fs::read_to_string(path.join(MANIFEST_FILENAME)).expect("read manifest");
        serde_json::from_str(&text).expect("manifest should be valid json")
    }

    #[test]
    fn write_run_capsule_writes_manifest_and_artifacts() {
        let source_dir = unique_dir("sources-compatible");
        let capsule_dir = unique_dir("capsule-compatible");
        std::fs::create_dir_all(&source_dir).expect("create source dir");
        std::fs::create_dir_all(&capsule_dir).expect("create capsule dir");

        let old_path = source_dir.join("old.csv");
        let new_path = source_dir.join("new.csv");
        std::fs::write(&old_path, "loan_id,balance\nA1,100\n").expect("write old csv");
        std::fs::write(&new_path, "loan_id,balance\nA1,150\n").expect("write new csv");

        let args = make_args(old_path.clone(), new_path.clone());
        let output = "{\"outcome\":\"COMPATIBLE\"}\n";
        write_run_capsule(
            &args,
            Outcome::Compatible,
            output,
            None,
            None,
            capsule_dir.as_path(),
        )
        .expect("write capsule");

        let manifest = parse_manifest(&capsule_dir);
        assert_eq!(manifest["schema_version"], "shape.capsule.v0");
        assert_eq!(manifest["tool"]["name"], "shape");
        assert_eq!(manifest["tool"]["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(manifest["result"]["outcome"], "COMPATIBLE");
        assert!(manifest["result"]["refusal"].is_null());
        assert_eq!(manifest["replay"]["argv"][0], "shape");
        assert_eq!(manifest["replay"]["argv"][1], OLD_ARTIFACT_PATH);
        assert_eq!(manifest["replay"]["argv"][2], NEW_ARTIFACT_PATH);

        let old_artifact_bytes =
            std::fs::read(capsule_dir.join(OLD_ARTIFACT_PATH)).expect("read old artifact");
        let new_artifact_bytes =
            std::fs::read(capsule_dir.join(NEW_ARTIFACT_PATH)).expect("read new artifact");
        assert_eq!(
            old_artifact_bytes,
            std::fs::read(&old_path).expect("read old source")
        );
        assert_eq!(
            new_artifact_bytes,
            std::fs::read(&new_path).expect("read new source")
        );

        let artifacts = manifest["artifacts"]
            .as_array()
            .expect("artifacts should be an array");
        assert_eq!(artifacts.len(), 3);
        assert!(
            artifacts
                .iter()
                .all(|entry| entry["blake3"].is_string() || entry["blake3"].is_null())
        );

        let _ = std::fs::remove_dir_all(source_dir);
        let _ = std::fs::remove_dir_all(capsule_dir);
    }

    #[test]
    fn write_run_capsule_records_refusal_and_missing_input() {
        let source_dir = unique_dir("sources-refusal");
        let capsule_dir = unique_dir("capsule-refusal");
        std::fs::create_dir_all(&source_dir).expect("create source dir");
        std::fs::create_dir_all(&capsule_dir).expect("create capsule dir");

        let old_path = source_dir.join("old.csv");
        let missing_new = source_dir.join("missing-new.csv");
        std::fs::write(&old_path, "loan_id,balance\nA1,100\n").expect("write old csv");

        let args = make_args(old_path.clone(), missing_new.clone());
        let refusal = RefusalPayload::io(missing_new.to_string_lossy(), "No such file");

        write_run_capsule(
            &args,
            Outcome::Refusal,
            "SHAPE ERROR (E_IO)\n",
            Some(&refusal),
            None,
            capsule_dir.as_path(),
        )
        .expect("write refusal capsule");

        let manifest = parse_manifest(&capsule_dir);
        assert_eq!(manifest["result"]["outcome"], "REFUSAL");
        assert_eq!(manifest["result"]["refusal"]["code"], "E_IO");

        let new_artifact = manifest["artifacts"]
            .as_array()
            .expect("artifacts should be an array")
            .iter()
            .find(|entry| entry["name"] == "new_input")
            .expect("new_input artifact should exist");
        assert!(new_artifact["source_error"].is_string());
        assert!(new_artifact["blake3"].is_null());
        assert!(!capsule_dir.join(NEW_ARTIFACT_PATH).exists());

        let _ = std::fs::remove_dir_all(source_dir);
        let _ = std::fs::remove_dir_all(capsule_dir);
    }
}
