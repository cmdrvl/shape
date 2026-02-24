# Documentation Canonical Map

Updated: 2026-02-24

## Canonical v0.1 readiness artifacts

- `docs/PLAN_SIGNOFF.md`: final PLAN must-have/deferred signoff.
- `docs/ACCEPTANCE_CHECKLIST.md`: quality-gate and acceptance checklist companion.

## Support artifacts (historical parallel slices)

These files are retained for auditability but are not authoritative for release signoff decisions:

- `docs/final-signoff.md`
- `docs/plan-conformance-audit.md`
- `docs/acceptance-checklist.md`

## Rule

When docs differ, treat the canonical pair above as source of truth.

## UBS Policy (Noise Baseline + Gating)

UBS is useful in this repository, but raw warning volume is high and includes many non-actionable heuristic classes. Use the scoped policy below.

### Current Baseline Snapshot (Rust `src/`)

- Command: `ubs --ci --only=rust src --report-json .ubs-baseline.json`
- Baseline totals from `.ubs-baseline.json`:
  - critical: `0`
  - warning: `688`
  - info: `156`

### Buckets

- Fix now:
  - Critical findings in production code
  - High-confidence panic-path findings in production (`bd-3e8a`)
  - Direct panic macro findings in tests (`bd-agpe`)
- Backlog:
  - `unreachable!` in production paths where invariants should be proven
  - targeted `expect` cleanup beyond current critical hotspots
- Ignore for gate (advisory only):
  - broad heuristic inventories (`assert!` count, clone count, cast inventory, format-literal suggestions)
  - tool-environment warnings (`cargo-audit not installed`, etc.)

### Reproducible Gate Command

Use the repository gate wrapper (local and CI):

```bash
./scripts/ubs_gate.sh
```

Behavior:
- scans Rust sources only (default scope: `src`)
- fails only on critical findings
- reports warning/info counts for trend tracking
- optional strict mode: `UBS_FAIL_ON_WARNING=1 ./scripts/ubs_gate.sh`

### CI Integration Note

If UBS is installed in CI, run `./scripts/ubs_gate.sh` as an advisory/quality step and persist `.ubs-gate-summary.json` as an artifact for trend comparison.
