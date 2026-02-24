# Final PLAN Conformance Signoff (v0.1)

> **Status:** Support document (non-canonical).
> Canonical release artifacts live in:
> - [`PLAN_SIGNOFF.md`](./PLAN_SIGNOFF.md)
> - [`ACCEPTANCE_CHECKLIST.md`](./ACCEPTANCE_CHECKLIST.md)

Date: 2026-02-24
Issue: `bd-1dt`

## Dependency Closure

- [x] `bd-2b5` post-integration regression hardening: closed
- [x] `bd-2km` quality gates + acceptance checklist: closed
- [x] `bd-3lb` README/operator docs alignment: closed

## Must-Have Checklist (From `docs/PLAN.md`)

- [x] `<OLD> <NEW>` positional args
Evidence: `src/cli/args.rs` parser tests (`parse_requires_positionals_without_describe`), integration invocations in `tests/*`.

- [x] `--key <column>`
Evidence: `src/cli/args.rs` parsing tests and end-to-end checks in `tests/human_golden.rs`, `tests/json_golden.rs`, `tests/e2e_*`.

- [x] `--delimiter <delim>` with rvl-compatible parsing semantics
Evidence: `src/cli/delimiter.rs` unit tests + dialect/`sep=` coverage in `src/csv/dialect.rs`, `src/csv/sep.rs`, and parser tests.

- [x] `--json`
Evidence: renderer contract tests in `src/output/json.rs`; integration snapshots in `tests/json_golden.rs`; routing tests in `tests/stream_routing.rs`.

- [x] `--version` prints `shape <semver>` and exits successfully
Evidence: runtime verification (`shape --version` -> `shape 0.1.0`, exit `0`) and clap parse coverage in `src/cli/args.rs`.

- [x] COMPATIBLE / INCOMPATIBLE outcomes with all four checks
Evidence: check modules (`src/checks/*.rs`), suite assembly/outcome tests (`src/checks/suite.rs`), orchestrator integration tests (`src/orchestrator.rs`, `tests/orchestrator_handoff.rs`).

- [x] Exit codes `0/1/2`
Evidence: `src/cli/exit.rs` tests plus end-to-end assertions in `tests/stream_routing.rs`, `tests/e2e_harness.rs`, `tests/e2e_matrix.rs`.

- [x] Refusal system with `E_IO`, `E_ENCODING`, `E_CSV_PARSE`, `E_EMPTY`, `E_HEADERS`, `E_DIALECT`
Evidence: refusal code/payload tests in `src/refusal/codes.rs` and `src/refusal/payload.rs`; parser/input/orchestrator refusal-path tests.

- [x] Human and JSON output modes
Evidence: renderer unit/snapshot tests (`src/output/human.rs`, `src/output/json.rs`) + integration snapshots (`tests/human_golden.rs`, `tests/json_golden.rs`).

- [x] `operator.json` + `--describe`
Evidence: checked `operator.json` schema contract, `shape --describe` smoke, and test `cli_harness_describe_mode_emits_operator_json`.

## Deferred (Explicitly Out of Scope Per PLAN)

- [x] `--profile` / `--profile-id` operational scoping
- [x] `--lock` input verification
- [x] `--max-rows` / `--max-bytes` enforcement
- [x] `--schema` flag
- [x] `--progress` flag

These are documented as deferred/reserved in README/operator contract and do not block v0.1 signoff.

## Final Readiness Summary

- [x] Full quality gates are green (`fmt`, `check`, `clippy -D warnings`, `test`).
- [x] Must-have PLAN items are implemented and mapped to concrete evidence.
- [x] Deferred scope is explicitly documented.
- [x] No blocking ambiguities remain for one-shot execution readiness.
