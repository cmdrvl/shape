# Fixture Corpus (`bd-1bk`)

All fixtures are deterministic and intentionally small so unit/integration tests can assert exact structural behavior.

## Pairs

- `basic_old.csv` + `basic_new.csv`: baseline compatible schema with unique key candidates.
- `schema_drift_old.csv` + `schema_drift_new.csv`: shared core columns with old/new-only fields.
- `type_shift_old.csv` + `type_shift_new.csv`: `balance` shifts from numeric to non-numeric.
- `dup_key_old.csv` + `dup_key_new.csv`: old file contains duplicate key value (`D1`).

## Singles

- `empty.csv`: header-only file (zero data rows) for `E_EMPTY` flows.
- `no_header.csv`: truly empty file (no bytes) for missing-header refusal paths.
- `ambiguous_old.csv`: delimiter-ambiguous content (`a,b;c`) for `E_DIALECT` refusal paths.

## Notes

- Fixtures avoid randomness and locale-specific formatting.
- Values are ASCII/UTF-8 and stable across platforms.
