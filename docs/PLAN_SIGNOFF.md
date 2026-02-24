# PLAN Conformance Signoff (v0.1)

Updated: 2026-02-24

## Canonical artifact map

To avoid drift from parallel signoff slices, treat these as canonical for v0.1 readiness:

- `docs/ACCEPTANCE_CHECKLIST.md` — quality-gate + acceptance checklist
- `docs/PLAN_SIGNOFF.md` — final PLAN must-have/deferred signoff

Additional generated signoff/checklist docs under `docs/` are retained as historical support artifacts:

- `docs/acceptance-checklist.md`
- `docs/final-signoff.md`
- `docs/plan-conformance-audit.md`

## Dependency closure

`bd-1dt` blockers are closed:

- [x] `bd-2b5` post-integration hardening
- [x] `bd-2km` quality gates + acceptance checklist
- [x] `bd-3lb` README/operator alignment

## Must-have matrix (docs/PLAN.md v0.1)

| Requirement | Status | Evidence |
|---|---|---|
| `<OLD> <NEW>` positional args | [x] | `src/cli/args.rs`, `cli::args::tests::parse_requires_positionals_without_describe` |
| `--key <column>` | [x] | `src/cli/args.rs`, `src/checks/key_viability.rs`, `tests/mod.rs` key-path tests |
| `--delimiter <delim>` | [x] | `src/cli/delimiter.rs`, `src/csv/sep.rs`, `orchestrator::tests::run_forced_delimiter_consumes_sep_directive_before_header_parse` |
| `--json` | [x] | `src/output/json.rs`, `tests/json_golden.rs`, `tests/stream_routing.rs` |
| `--version` | [x] | clap metadata in `src/cli/args.rs`, `cli::args::tests::parse_version_without_positionals` |
| COMPATIBLE/INCOMPATIBLE with four checks | [x] | `src/checks/*`, `src/checks/suite.rs`, `orchestrator::tests::run_renders_*` |
| Exit codes `0/1/2` | [x] | `src/cli/exit.rs`, `cli::exit::tests::*`, integration tests in `tests/stream_routing.rs` |
| Refusals `E_IO`, `E_ENCODING`, `E_CSV_PARSE`, `E_EMPTY`, `E_HEADERS`, `E_DIALECT` | [x] | `src/refusal/codes.rs`, `src/refusal/payload.rs` tests, parser/input/orchestrator refusal tests |
| Human + JSON outputs | [x] | `src/output/human.rs`, `src/output/json.rs`, `tests/human_golden.rs`, `tests/json_golden.rs` |
| `operator.json` + `--describe` | [x] | `operator.json`, `lib::run()` describe path, `tests/mod.rs::cli_harness_describe_mode_emits_operator_json` |

## Deferred items (intentional)

Out-of-scope per PLAN "Can defer" and currently treated as reserved/deferred behavior:

- [x] `--profile` / `--profile-id`
- [x] `--lock` verification
- [x] `--max-rows` / `--max-bytes` enforcement
- [x] `--schema`
- [x] `--progress`

## One-shot readiness summary

- [x] Core CLI, parser, checks, orchestrator, refusal system, and output modes are implemented.
- [x] Golden/integration harness tests cover compatible/incompatible/refusal in both human and JSON modes.
- [x] Stream routing and exit semantics are verified end-to-end.
- [x] Current quality gates are green:
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`

Signoff: `shape` v0.1 is ready for one-shot parallel execution with no blocking ambiguity in required PLAN scope.
