//! `--lock` input verification against `lock.v0` lockfiles.
//!
//! Matching uses cmdrvl-core's pure R1 names-and-digests rule: drop leading
//! `./`, prefer an exact normalized path, otherwise require one normalized
//! basename path, validate every candidate digest, and require every candidate
//! digest to match. Loading and hashing remain in shape. Unreadable or malformed
//! lockfiles refuse with `E_IO`.

use std::path::{Path, PathBuf};

use cmdrvl_core::hash::{Algorithm, hash_bytes};
use cmdrvl_core::lock_matching::{Member, SelectError, Verdict, select};
use serde::Deserialize;

use crate::refusal::payload::RefusalPayload;

#[derive(Debug, Deserialize)]
struct Lockfile {
    version: String,
    #[serde(default)]
    lock_hash: Option<String>,
    members: Vec<LockMember>,
}

#[derive(Debug, Deserialize)]
struct LockMember {
    path: String,
    bytes_hash: String,
}

#[derive(Debug, Clone)]
struct Candidate {
    lock: usize,
    lock_hash: Option<String>,
    member_path: String,
    bytes_hash: String,
}

/// Evidence that one input matched a member in a lockfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedInput {
    pub lockfile: String,
    pub lock_hash: Option<String>,
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
/// This remains as a focused unit-test helper; production matching uses R1's
/// algorithm list and hashes each required algorithm before judging.
#[cfg(test)]
fn hash_like(bytes: &[u8], expected: &str) -> Option<String> {
    Algorithm::of(expected).map(|algorithm| hash_bytes(algorithm, bytes))
}

#[cfg(windows)]
fn typed_name_for_matching(file: &str) -> String {
    file.replace('\\', "/")
}

#[cfg(not(windows))]
fn typed_name_for_matching(file: &str) -> String {
    file.to_owned()
}

/// Verify one input file against the provided lockfiles.
pub fn verify_input(input: &Path, lockfiles: &[PathBuf]) -> Result<VerifiedInput, RefusalPayload> {
    let file = input.to_string_lossy().into_owned();
    let bytes = read_bytes(input)?;
    let input_name = typed_name_for_matching(&file);
    let mut candidates = Vec::new();
    for (lock, lockfile_path) in lockfiles.iter().enumerate() {
        let lockfile = load_lockfile(lockfile_path)?;
        for member in lockfile.members {
            candidates.push(Candidate {
                lock,
                lock_hash: lockfile.lock_hash.clone(),
                member_path: member.path,
                bytes_hash: member.bytes_hash,
            });
        }
    }

    let members = candidates.iter().map(|candidate| Member {
        lock: candidate.lock,
        path: candidate.member_path.as_str(),
        digest: candidate.bytes_hash.as_str(),
    });
    let selection = match select(&input_name, members) {
        Ok(selection) => selection,
        Err(SelectError::NotListed | SelectError::SeveralPaths(_)) => {
            return Err(RefusalPayload::input_not_locked(file));
        }
        Err(SelectError::UnusableDigest(member)) => {
            let lockfile = lockfiles
                .get(member.lock)
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Err(RefusalPayload::io(
                lockfile,
                format!("unsupported bytes_hash algorithm in '{}'", member.digest),
            ));
        }
    };

    let observed: Vec<(Algorithm, String)> = selection
        .algorithms()
        .into_iter()
        .map(|algorithm| (algorithm, hash_bytes(algorithm, &bytes)))
        .collect();
    let observed_refs: Vec<(Algorithm, &str)> = observed
        .iter()
        .map(|(algorithm, digest)| (*algorithm, digest.as_str()))
        .collect();
    match selection.judge(&observed_refs) {
        Ok(Verdict::Verified { locks, .. }) => {
            let lock = locks
                .first()
                .copied()
                .ok_or_else(|| RefusalPayload::io(file.clone(), "R1 selected no matching lock"))?;
            let lockfile = lockfiles.get(lock).ok_or_else(|| {
                RefusalPayload::io(file.clone(), "R1 selected an invalid lock position")
            })?;
            let lock_hash = candidates
                .iter()
                .find(|candidate| candidate.lock == lock)
                .and_then(|candidate| candidate.lock_hash.clone());
            Ok(VerifiedInput {
                lockfile: lockfile.to_string_lossy().into_owned(),
                lock_hash,
            })
        }
        Ok(Verdict::Drift { differing, .. }) => {
            let first = differing.first().ok_or_else(|| {
                RefusalPayload::io(file.clone(), "R1 reported drift without a differing member")
            })?;
            let algorithm = Algorithm::of(first.digest).ok_or_else(|| {
                RefusalPayload::io(file.clone(), "R1 reported an unknown digest algorithm")
            })?;
            let actual = observed
                .iter()
                .find_map(|(kind, digest)| match (*kind, algorithm) {
                    (Algorithm::Blake3, Algorithm::Blake3)
                    | (Algorithm::Sha256, Algorithm::Sha256) => Some(digest.as_str()),
                    _ => None,
                })
                .unwrap_or_default();
            Err(RefusalPayload::input_drift(file, first.digest, actual))
        }
        Err(missing) => Err(RefusalPayload::io(
            file,
            format!(
                "R1 did not receive the computed {} digest",
                missing.algorithm.prefix()
            ),
        )),
    }
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
        let stale = "sha256:deadbeef00000000000000000000000000000000000000000000000000000000";
        let lock = lock_for(&input.to_string_lossy(), stale);
        let err = verify_input(&input, &[lock]).unwrap_err();
        assert_eq!(err.code, RefusalCode::EInputDrift);
        assert_eq!(err.detail["expected_hash"], stale);
    }

    #[test]
    fn malformed_lockfile_refuses_io() {
        let input = scratch("a.csv", "id,v\n1,2\n");
        let lock = scratch("bad.lock.json", "not json");
        let err = verify_input(&input, &[lock]).unwrap_err();
        assert_eq!(err.code, RefusalCode::EIo);
    }
}
