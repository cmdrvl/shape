# Acceptance Checklist (Canonical)

Updated: 2026-02-24

This is the canonical acceptance checklist companion for v0.1 signoff.

## Canonical pair

- `docs/ACCEPTANCE_CHECKLIST.md`
- `docs/PLAN_SIGNOFF.md`

## Support artifacts (non-canonical)

- `docs/acceptance-checklist.md`
- `docs/final-signoff.md`
- `docs/plan-conformance-audit.md`

If wording differs across docs, treat the canonical pair above as source of truth.

## Quick gate status

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`
- [x] `shape --version` returns `shape 0.1.0` with exit `0`
- [x] `shape --describe` exits `0`
