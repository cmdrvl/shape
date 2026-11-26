use serde_json::Value;
use std::fs;
use std::panic::panic_any;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SEED: u64 = 0x5348_4150_4533_0001;

#[derive(Clone, Copy)]
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    fn shuffle<T>(&mut self, values: &mut [T]) {
        for index in (1..values.len()).rev() {
            let swap = (self.next() as usize) % (index + 1);
            values.swap(index, swap);
        }
    }
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("test clock must be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "shape-key-edges-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("test workspace should be creatable");
    path
}

fn write_csv(path: &Path, ids: &[u32]) {
    let mut csv = String::from("id,value\n");
    for id in ids {
        csv.push_str(&format!("{id},{}\n", id * 3 + 1));
    }
    fs::write(path, csv).expect("generated CSV should be writable");
}

fn run_shape_with_timeout(args: &[String], seed: u64) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_shape"))
        .args(args)
        .env("EPISTEMIC_WITNESS", "")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| {
            panic_any(format!("seed {seed:#x}: failed to start shape: {error}"))
        });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child.wait_with_output().unwrap_or_else(|error| {
                    panic_any(format!("seed {seed:#x}: collect output: {error}"))
                });
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Ok(None) => {
                child.kill().unwrap_or_else(|error| {
                    panic_any(format!("seed {seed:#x}: kill timed-out child: {error}"))
                });
                let _ = child.wait();
                panic_any(format!(
                    "seed {seed:#x}: shape child exceeded five-second timeout"
                ));
            }
            Err(error) => panic_any(format!("seed {seed:#x}: poll shape child: {error}")),
        }
    }
}

#[test]
fn seeded_key_overlap_matrix_matches_integer_oracle() {
    // Invariant: with non-empty, unique key sets, compatibility requires a
    // positive overlap and an inclusive coverage floor. Oracle: intersection
    // size / max(set sizes), computed from the generator's integer IDs. The
    // seed and case identify every generated child if this invariant breaks.
    let mut rng = XorShift64::new(SEED);
    for case in 0..48u64 {
        let workspace = unique_temp_dir(&format!("{case}"));
        let old_path = workspace.join("old.csv");
        let new_path = workspace.join("new.csv");
        let old_len = 1 + (rng.next() % 7) as usize;
        let new_len = 1 + (rng.next() % 7) as usize;
        let maximum_shared = old_len.min(new_len);
        let shared = if case % 4 == 0 {
            0
        } else {
            1 + (rng.next() as usize % maximum_shared)
        };

        let mut old_ids: Vec<u32> = (0..old_len)
            .map(|index| 10_000 + (case as u32 * 100) + index as u32)
            .collect();
        let mut new_ids: Vec<u32> = old_ids[..shared].to_vec();
        new_ids.extend(
            (0..(new_len - shared)).map(|index| 20_000 + (case as u32 * 100) + index as u32),
        );
        rng.shuffle(&mut old_ids);
        rng.shuffle(&mut new_ids);
        write_csv(&old_path, &old_ids);
        write_csv(&new_path, &new_ids);

        let coverage = shared as f64 / old_len.max(new_len) as f64;
        let threshold = match case % 3 {
            0 => 0.0,
            1 => coverage,
            _ => (coverage + 0.01).min(1.0),
        };
        let expected_status = if shared > 0 && coverage >= threshold {
            0
        } else {
            1
        };
        let args = vec![
            old_path.display().to_string(),
            new_path.display().to_string(),
            "--json".to_owned(),
            "--no-witness".to_owned(),
            "--key".to_owned(),
            "id".to_owned(),
            "--min-coverage".to_owned(),
            // Keep the inclusive-floor case at the exact f64 spelling used by
            // the oracle; rounding a repeating ratio upward would test the
            // threshold parser instead of key-viability semantics.
            format!("{threshold:.17}"),
        ];
        let output = run_shape_with_timeout(&args, SEED ^ case);
        let status = output.status.code().unwrap_or(-1);
        assert_eq!(
            status,
            expected_status,
            "seed {SEED:#x}, case {case}: stdout={:?}, stderr={:?}, old={old_ids:?}, new={new_ids:?}, threshold={threshold}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "seed {SEED:#x}, case {case}: unexpected stderr {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic_any(format!(
                "seed {SEED:#x}, case {case}: JSON oracle input failed: {error}; stdout={:?}",
                String::from_utf8_lossy(&output.stdout)
            ))
        });
        assert_eq!(value["checks"]["key_viability"]["coverage"], coverage);
        assert_eq!(
            value["checks"]["key_viability"]["status"],
            if expected_status == 0 { "pass" } else { "fail" }
        );
        if shared == 0 {
            // Planted negative: removing the positive-overlap guard would make
            // every zero-overlap case with a zero floor pass this assertion.
            assert_eq!(value["outcome"], "INCOMPATIBLE");
            assert!(value["reasons"].as_array().is_some_and(|reasons| {
                reasons.iter().any(|reason| {
                    reason
                        .as_str()
                        .is_some_and(|text| text.contains("no overlapping key values"))
                })
            }));
        }
    }
}
