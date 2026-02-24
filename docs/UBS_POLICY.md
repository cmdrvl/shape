# UBS Policy (shape)

Updated: 2026-02-24

## Goal

Make UBS useful for regression detection in `shape` by gating on high-signal findings and tracking noisy classes separately.

## Snapshot (Current Baseline)

Command:

```bash
ubs --ci --format=json --report-json /tmp/shape-ubs-final.json .
```

Observed totals:

- `critical`: `0`
- `warning`: `1116`
- `info`: `183`
- `files`: `52`

Warning classes (from `ubs-rust` findings export):

- `255` `Ownership & Error Handling`: `Potential panics via unwrap/expect`
- `2` `Ownership & Error Handling`: `unreachable! may panic if reached`
- `827` `Panic Surfaces & Unwinding`: `assert! macros present (panic surface)`
- `32` `Parsing & Validation Robustness`: `serde_json::from_str(...).unwrap()/expect()`

## Triage Buckets

### Fix Now

- Production panic-surface findings in `src/**`:
  - `unwrap/expect` in non-test runtime paths
  - `unreachable!` in non-test runtime paths

Rationale: these are real runtime panic risks and should not be allowed to regress.

### Backlog

- Test-heavy warning classes:
  - `assert!` macro volume
  - `serde_json::from_str(...).expect()` in tests
  - `unwrap/expect` in test modules

Rationale: high volume, low runtime risk, but still worth gradual cleanup for test robustness.

### Ignore For Gate (Track Only)

- Informational classes (allocation/style/perf hints without concrete correctness risk)

Rationale: useful for periodic quality sweeps, not stable enough for per-PR hard fail.

## Gating Policy

### 1) Hard Gate: Critical Findings Only

Run UBS, then fail CI only when `critical > 0`:

```bash
ubs --ci --format=json --report-json .ubs/summary.json .
jq -e '.totals.critical == 0' .ubs/summary.json >/dev/null
```

### 2) Soft Gate: Warning Trend Visibility

Always publish UBS summary/artifacts for review:

```bash
ubs --ci --format=json --report-json .ubs/summary.json .
ubs --ci --format=text . > .ubs/report.txt
```

Warnings are reviewed but do not fail CI until noise is reduced.

### 3) Scope Guidance For Local Developer Loops

For fast iteration on changed code:

```bash
ubs --diff --only=rust .
```

## CI Integration Note

Recommended CI shape:

1. Run `ubs` summary JSON step and enforce `critical == 0`.
2. Upload `.ubs/summary.json` and `.ubs/report.txt` as artifacts.
3. Keep warning-based hard fail disabled until test-noise backlog is reduced.
