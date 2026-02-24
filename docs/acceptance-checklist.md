# Shape v0 Acceptance Checklist

> **Status:** Support document (non-canonical).
> Canonical release artifacts live in:
> - [`PLAN_SIGNOFF.md`](./PLAN_SIGNOFF.md)
> - [`ACCEPTANCE_CHECKLIST.md`](./ACCEPTANCE_CHECKLIST.md)

Last verified: 2026-02-24 (UTC)

## Quality Gates

- [x] `cargo fmt --check`
- [x] `cargo check --all-targets`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`

All gates passed in a single run after integration hardening (`bd-2b5`) and acceptance verification (`bd-2km`).

## PLAN Coverage Checklist

- [x] CLI behavior and exit semantics (`0/1/2`) are validated.
Evidence: `src/cli/args.rs` tests, `src/cli/exit.rs` tests, `tests/stream_routing.rs`, `tests/e2e_matrix.rs`, `tests/e2e_harness.rs`.

- [x] CSV/parser contract coverage is present for delimiter handling, `sep=`, parse failures, and header normalization/refusals.
Evidence: `src/csv/*.rs` tests, `src/normalize/headers.rs` tests, parser/refusal tests in `tests/mod.rs`.

- [x] Core check evaluation logic is covered for schema overlap, key viability, row granularity, type consistency, outcome, and reason ordering.
Evidence: `src/checks/*.rs` tests and suite assembly/reason tests in `src/checks/suite.rs`.

- [x] Refusal code schema/actions are validated for all declared refusal codes and payload detail structures.
Evidence: `src/refusal/codes.rs` tests and `src/refusal/payload.rs` tests.

- [x] JSON output contract (`shape.v0`) including top-level key presence and nullability matrix is covered.
Evidence: `src/output/json.rs` tests and `tests/json_golden.rs`.

- [x] Human output layout/sections/formatting and refusal header behavior are covered.
Evidence: `src/output/human.rs` snapshot-style tests, `tests/human_golden.rs`, `tests/human_snapshots.rs`, `tests/mod.rs` human snapshot assertions.

- [x] Orchestrator fail-fast and handoff behavior (human/json, partial refusal context) is covered.
Evidence: `src/orchestrator.rs` tests, `tests/orchestrator_pipeline.rs`, `tests/orchestrator_handoff.rs`, `tests/e2e_harness.rs`, `tests/e2e_matrix.rs`.

## v0.1 Must-Have Checklist (PLAN Scope)

- [x] `<OLD> <NEW>` positional CLI contract is implemented and tested.
Evidence: `src/cli/args.rs` + integration invocations in `tests/mod.rs`.

- [x] `--key <column>` is implemented and exercised across compatible/incompatible cases.
Evidence: `src/checks/key_viability.rs`, `tests/json_golden.rs`, `tests/human_golden.rs`.

- [x] `--delimiter <delim>` is implemented with forced-delimiter precedence and `sep=` interaction.
Evidence: `src/csv/sep.rs`, `src/csv/input.rs`, `src/orchestrator.rs` regression tests.

- [x] `--json` output mode is implemented and validated for all outcomes.
Evidence: `src/output/json.rs`, `tests/json_golden.rs`, `tests/stream_routing.rs`.

- [x] `--version` prints `shape <semver>` and exits `0`.
Evidence: `tests/version_flag.rs`.

- [x] COMPATIBLE / INCOMPATIBLE domain outcomes with four checks are implemented.
Evidence: `src/checks/suite.rs`, `tests/e2e_matrix.rs`.

- [x] Exit-code semantics (`0/1/2`) are implemented and verified end-to-end.
Evidence: `src/cli/exit.rs`, `tests/stream_routing.rs`, `tests/e2e_harness.rs`.

- [x] Refusal system includes required v0.1 refusal set (`E_IO`, `E_ENCODING`, `E_CSV_PARSE`, `E_EMPTY`, `E_HEADERS`, `E_DIALECT`) with deterministic payloads.
Evidence: `src/refusal/codes.rs`, `src/refusal/payload.rs`, refusal coverage in integration tests.

- [x] Human and JSON outputs are both implemented with deterministic layout/schema coverage.
Evidence: `src/output/human.rs`, `src/output/json.rs`, golden tests.

- [x] `operator.json` and `--describe` are implemented and validated.
Evidence: `operator.json`, `tests/mod.rs::cli_harness_describe_mode_emits_operator_json`.

## Deferred Items (PLAN "Can defer")

- [x] `--profile` / `--profile-id`: CLI shape exists; full profile-tool semantics intentionally deferred.
- [x] `--lock` input verification: CLI shape exists; lock-tool semantics intentionally deferred.
- [x] `--max-rows` / `--max-bytes`: CLI flags exist; enforcement deferred by PLAN scope.
- [x] `--schema` flag: intentionally out of v0.1 scope.
- [x] `--progress` flag: intentionally out of v0.1 scope.

No deferred item above blocks v0.1 ship criteria.

## One-Shot Readiness Summary

- All dependency beads for final signoff are closed (`bd-2b5`, `bd-2km`, `bd-3lb`).
- Quality gates and regression suites are green on integrated tree.
- PLAN must-haves are mapped to in-repo evidence and deferred scope is explicitly documented.
- Team can execute one-shot parallel pickup from this baseline without unresolved ambiguity.

## Acceptance Result

- [x] No remaining known compile/lint/test regressions after hardening.
- [x] Coverage spans parser/checks/output/refusal/stream-routing/e2e paths required by PLAN.
- [x] v0.1 must-haves are complete; deferred items are explicitly documented.
- [x] Project is ready for final PLAN conformance signoff (`bd-1dt`).
