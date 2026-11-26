//! `--lock` input verification against `lock.v0` lockfiles.
//!
//! Matching mirrors `benchmark`/`verify`: an exact path match wins, otherwise a
//! unique basename match is used. A matched member whose `bytes_hash`
//! (`sha256:<hex>` of the raw file bytes) differs refuses with `E_INPUT_DRIFT`;
//! no match refuses with `E_INPUT_NOT_LOCKED`. Unreadable or malformed
//! lockfiles refuse with `E_IO`.

use std::path::{Path, PathBuf};

use cmdrvl_core::hash::{Algorithm, hash_bytes};
use serde::Deserialize;

use crate::refusal::payload::RefusalPayload;

#[derive(Debug, Deserialize)]
struct Lockfile {
    version: String,
    members: Vec<LockMember>,
}

#[derive(Debug, Deserialize)]
struct LockMember {
    path: String,
    bytes_hash: String,
}

#[derive(Debug, Clone)]
struct Candidate {
    lockfile: PathBuf,
    member_path: String,
    bytes_hash: String,
}

fn load_lockfile(path: &Path) -> Result<Lockfile, RefusalPayload> {
    let file = path.to_string_lossy().into_owned();
    let contents = std::fs::read_to_string(path)
        .map_err(|err| RefusalPayload::io(file.clone(), err.to_string()))?;
    let lockfile: Lockfile = serde_json::from_str(&contents)
        .map_err(|err| RefusalPayload::io(file.clone(), format!("invalid lockfile: {err}")))?;
    if lockfile.version != "lock.v0" {
        return Err(RefusalPayload::io(
            file,
            format!("unsupported lockfile version '{}'", lockfile.version),
        ));
    }
    Ok(lockfile)
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, RefusalPayload> {
    std::fs::read(path).map_err(|err| RefusalPayload::io(path.to_string_lossy(), err.to_string()))
}

/// Hash `bytes` with the algorithm named by the prefix of `expected`
/// (`sha256:` or `blake3:`, as `lock`/`hashbytes` emit). `None` if unsupported.
///
/// Only the hashing comes from cmdrvl-core. Member matching stays here on purpose:
/// `cmdrvl_core::lockfile` differs (it strips a leading `./`, so it verifies inputs that this
/// module refuses as `E_INPUT_NOT_LOCKED`).
fn hash_like(bytes: &[u8], expected: &str) -> Option<String> {
    Algorithm::of(expected).map(|algorithm| hash_bytes(algorithm, bytes))
}

fn check_matches(
    file: &str,
    bytes: &[u8],
    matches: &[Candidate],
) -> Option<Result<(), RefusalPayload>> {
    if matches.is_empty() {
        return None;
    }
    for candidate in matches {
        match hash_like(bytes, &candidate.bytes_hash) {
            Some(actual) if actual == candidate.bytes_hash => return Some(Ok(())),
            Some(_) => {}
            None => {
                return Some(Err(RefusalPayload::io(
                    candidate.lockfile.to_string_lossy(),
                    format!(
                        "unsupported bytes_hash algorithm in '{}'",
                        candidate.bytes_hash
                    ),
                )));
            }
        }
    }
    let first = &matches[0];
    let actual = hash_like(bytes, &first.bytes_hash).unwrap_or_default();
    Some(Err(RefusalPayload::input_drift(
        file,
        first.bytes_hash.clone(),
        actual,
    )))
}

/// Verify one input file against the provided lockfiles.
pub fn verify_input(input: &Path, lockfiles: &[PathBuf]) -> Result<(), RefusalPayload> {
    let file = input.to_string_lossy().into_owned();
    let bytes = read_bytes(input)?;
    let exact_path = file.replace('\\', "/");
    let basename = input.file_name().and_then(|n| n.to_str());

    let mut exact = Vec::new();
    let mut by_basename: Vec<Candidate> = Vec::new();
    for lockfile_path in lockfiles {
        for member in load_lockfile(lockfile_path)?.members {
            let candidate = Candidate {
                lockfile: lockfile_path.clone(),
                member_path: member.path,
                bytes_hash: member.bytes_hash,
            };
            if candidate.member_path == exact_path {
                exact.push(candidate);
            } else if basename.is_some()
                && Path::new(&candidate.member_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    == basename
                && !by_basename.iter().any(|seen| {
                    seen.lockfile == candidate.lockfile && seen.member_path == candidate.member_path
                })
            {
                by_basename.push(candidate);
            }
        }
    }

    if let Some(result) = check_matches(&file, &bytes, &exact) {
        return result;
    }
    if by_basename.len() > 1 {
        // Ambiguous basename match cannot be verified safely.
        return Err(RefusalPayload::input_not_locked(file));
    }
    if let Some(result) = check_matches(&file, &bytes, &by_basename) {
        return result;
    }
    Err(RefusalPayload::input_not_locked(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refusal::codes::RefusalCode;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);

    fn scratch(name: &str, contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "shape-lock-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn lock_for(member: &str, hash: &str) -> PathBuf {
        scratch(
            "x.lock.json",
            &format!(
                r#"{{"version":"lock.v0","members":[{{"path":"{member}","bytes_hash":"{hash}"}}]}}"#
            ),
        )
    }

    #[test]
    fn matching_hash_verifies() {
        let input = scratch("a.csv", "id,v\n1,2\n");
        let hash = hash_like(&read_bytes(&input).unwrap(), "sha256:").unwrap();
        let lock = lock_for(&input.to_string_lossy(), &hash);
        verify_input(&input, &[lock]).expect("verified");
    }

    #[test]
    fn blake3_hash_verifies_and_detects_drift() {
        let input = scratch("a.csv", "id,v\n1,2\n");
        let good = format!("blake3:{}", blake3::hash(b"id,v\n1,2\n").to_hex());
        verify_input(&input, &[lock_for(&input.to_string_lossy(), &good)]).expect("verified");

        let stale = format!("blake3:{}", blake3::hash(b"other bytes").to_hex());
        let err = verify_input(&input, &[lock_for(&input.to_string_lossy(), &stale)]).unwrap_err();
        assert_eq!(err.code, RefusalCode::EInputDrift);
    }

    #[test]
    fn unsupported_hash_algorithm_refuses_io() {
        let input = scratch("a.csv", "id,v\n1,2\n");
        let lock = lock_for(&input.to_string_lossy(), "md5:abc");
        let err = verify_input(&input, &[lock]).unwrap_err();
        assert_eq!(err.code, RefusalCode::EIo);
    }

    #[test]
    fn basename_match_verifies() {
        let input = scratch("a.csv", "id,v\n1,2\n");
        let hash = hash_like(&read_bytes(&input).unwrap(), "sha256:").unwrap();
        let lock = lock_for("elsewhere/a.csv", &hash);
        verify_input(&input, &[lock]).expect("verified by basename");
    }

    #[test]
    fn unlocked_input_refuses_not_locked() {
        let input = scratch("a.csv", "id,v\n1,2\n");
        let lock = lock_for("other.csv", "sha256:00");
        let err = verify_input(&input, &[lock]).unwrap_err();
        assert_eq!(err.code, RefusalCode::EInputNotLocked);
    }

    #[test]
    fn changed_bytes_refuse_drift() {
        let input = scratch("a.csv", "id,v\n1,2\n");
        let lock = lock_for(&input.to_string_lossy(), "sha256:deadbeef");
        let err = verify_input(&input, &[lock]).unwrap_err();
        assert_eq!(err.code, RefusalCode::EInputDrift);
        assert_eq!(err.detail["expected_hash"], "sha256:deadbeef");
    }

    #[test]
    fn malformed_lockfile_refuses_io() {
        let input = scratch("a.csv", "id,v\n1,2\n");
        let lock = scratch("bad.lock.json", "not json");
        let err = verify_input(&input, &[lock]).unwrap_err();
        assert_eq!(err.code, RefusalCode::EIo);
    }
}
