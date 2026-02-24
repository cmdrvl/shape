use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;
use std::sync::OnceLock;

/// Hash an in-memory byte slice with BLAKE3.
pub fn hash_bytes(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

/// Stream-hash a file with BLAKE3.
pub fn hash_file(path: &Path) -> io::Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0_u8; 16 * 1024];

    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Hash the current executable, cached for process lifetime.
pub fn hash_self() -> io::Result<String> {
    static SELF_HASH: OnceLock<String> = OnceLock::new();
    if let Some(cached) = SELF_HASH.get() {
        return Ok(cached.clone());
    }

    let exe = std::env::current_exe()?;
    let computed = hash_file(&exe)?;
    Ok(SELF_HASH.get_or_init(|| computed).clone())
}

#[cfg(test)]
mod tests {
    use super::{hash_bytes, hash_file, hash_self};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "shape_witness_hash_test_{}-{seq}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn hash_bytes_matches_blake3() {
        let expected = blake3::hash(b"hello").to_hex().to_string();
        assert_eq!(hash_bytes(b"hello"), expected);
    }

    #[test]
    fn hash_file_matches_in_memory_hash() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("sample.bin");
        let data = b"shape witness hash file content";
        std::fs::write(&path, data).expect("write file");

        let file_hash = hash_file(&path).expect("hash file");
        let bytes_hash = hash_bytes(data);
        assert_eq!(file_hash, bytes_hash);

        std::fs::remove_file(path).ok();
        std::fs::remove_dir(dir).ok();
    }

    #[test]
    fn hash_self_is_stable_across_calls() {
        let first = hash_self().expect("hash self");
        let second = hash_self().expect("hash self cached");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }
}
