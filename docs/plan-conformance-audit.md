# PLAN Conformance Audit (bd-1dt)

> **Status:** Support document (non-canonical).
> Canonical release artifacts live in:
> - [`PLAN_SIGNOFF.md`](./PLAN_SIGNOFF.md)
> - [`ACCEPTANCE_CHECKLIST.md`](./ACCEPTANCE_CHECKLIST.md)

Last verified: 2026-02-24 (UTC)
Source of truth: `docs/PLAN.md` ("Scope: v0.1")

## v0.1 Must-Have Checklist

- [x] Positional args `<OLD> <NEW>`
  - Evidence: `src/cli/args.rs` (`required_unless_present = "describe"`), `tests/mod.rs` (`cli_harness_captures_usage_error_for_missing_positionals`).

- [x] `--key <column>`
  - Evidence: parser + runtime wiring in `src/cli/args.rs` and `src/orchestrator.rs`; exercised by `tests/human_golden.rs` and `tests/json_golden.rs`.

- [x] `--delimiter <delim>` with rvl-compatible parsing
  - Evidence: `src/cli/delimiter.rs` tests; forced-delimiter paths in parser/orchestrator tests.

- [x] `--json`
  - Evidence: JSON renderer (`src/output/json.rs`) plus CLI/integration tests (`tests/json_golden.rs`, `tests/stream_routing.rs`, `tests/orchestrator_handoff.rs`).

- [x] `--version` prints semver
  - Evidence: clap version wiring in `src/cli/args.rs`; smoke-run `shape --version` emits `shape 0.1.0`.

- [x] COMPATIBLE / INCOMPATIBLE outcomes with four checks
  - Evidence: check modules in `src/checks/*`, orchestration in `src/orchestrator.rs`, and outcome coverage across unit + E2E tests.

- [x] Exit codes `0/1/2`
  - Evidence: `src/cli/exit.rs` tests and matrix/E2E coverage (`tests/stream_routing.rs`, `tests/e2e_matrix.rs`, `tests/e2e_harness.rs`).

- [x] Refusal system with required codes (`E_IO`, `E_ENCODING`, `E_CSV_PARSE`, `E_EMPTY`, `E_HEADERS`, `E_DIALECT`)
  - Evidence: `src/refusal/codes.rs`, `src/refusal/payload.rs`, parser/input/orchestrator refusal tests.

- [x] Human + JSON output modes
  - Evidence: `src/output/human.rs`, `src/output/json.rs`, snapshots/golden tests in `tests/human_golden.rs` and `tests/json_golden.rs`.

- [x] `operator.json` + `--describe`
  - Evidence: `operator.json` committed; `tests/mod.rs` (`cli_harness_describe_mode_emits_operator_json`), smoke-run returns exit `0`.

## Deferred Scope (Explicitly Out-Of-Scope for v0.1 Runtime)

- [x] `--profile` / `--profile-id` runtime scoping deferred
- [x] `--lock` input verification deferred
- [x] `--max-rows` / `--max-bytes` enforcement deferred
- [x] `--schema` deferred
- [x] `--progress` deferred

Status: CLI/schema shapes exist where defined; runtime behavior remains deferred by intent and is documented in README/operator artifacts.

## Quality Gate Evidence

- [x] `cargo fmt --check`
- [x] `cargo check --all-targets`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`

All above commands passed on the integrated tree after hardening and acceptance updates.

## One-Shot Readiness Signoff

- [x] No known compile/lint/test regressions.
- [x] Required outcomes/refusals and stream routing verified by integration tests.
- [x] Docs and operator contract aligned with shipped behavior (README + `operator.json` + `--describe` checks).

Conclusion: `shape` v0.1 is ready for one-shot execution/signoff under the PLAN-defined must-have scope.
