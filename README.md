# shape

**Structural comparability gate — can these two CSV datasets be compared at all?**

*Built for teams who need to know whether two files are structurally compatible before running analysis.*

---

## Why This Exists

Before you can compare two CSV exports, you need to know if comparison is even meaningful. Do the columns match? Is the key column unique? Did the schema drift? `shape` answers these questions deterministically — before you waste time diffing.

- **COMPATIBLE** — columns align, key is viable, types are consistent. Safe to proceed with `rvl`, `compare`, or `verify`.
- **INCOMPATIBLE** — structural mismatch with explicit reasons. Tells you exactly what broke.
- **REFUSAL** — when parsing or reading fails, with a concrete next step.

No guessing. No silent schema drift. Just a structural gate or an explanation of why it failed.

---

## Install

**Homebrew (macOS / Linux):**

```bash
brew install cmdrvl/tap/shape
```

**Shell script (macOS / Linux):**

```bash
curl -fsSL https://raw.githubusercontent.com/cmdrvl/shape/main/scripts/install.sh | bash
```

**Windows (PowerShell):**

```powershell
Set-ExecutionPolicy -ExecutionPolicy Bypass -Scope Process -Force; iex ((New-Object System.Net.WebClient).DownloadString('https://raw.githubusercontent.com/cmdrvl/shape/main/scripts/install.ps1'))
```

**From source:**

```bash
cargo build --release
./target/release/shape --help
```

Prebuilt binaries are available for x86_64 and ARM64 on Linux, macOS, and Windows (x86_64). Each release includes SHA256 checksums, cosign signatures, and an SBOM.

---

## Quickstart

Check if two CSVs are structurally compatible:

```bash
shape old.csv new.csv
```

Check with a specific key column:

```bash
shape old.csv new.csv --key loan_id
```

Machine-readable JSON:

```bash
shape old.csv new.csv --key loan_id --json
```

---

## Optional: Auto Proceed Nudges (for NTM sessions)

If you run multi-agent sessions and want periodic `proceed` nudges, use:

```bash
scripts/ntm_proceed_ctl.sh start --session codex53-high
```

This feature is **off by default**. When started with defaults, it:

- Runs every `10m`
- Sends only during overnight hours (`20:00` to `08:00`, local time)
- Sends only if there are open or in-progress beads

Check/stop it:

```bash
scripts/ntm_proceed_ctl.sh status
scripts/ntm_proceed_ctl.sh stop
```

Useful overrides:

```bash
# Enable during daytime too
scripts/ntm_proceed_ctl.sh start --session codex53-high --mode always

# Custom overnight window and interval
scripts/ntm_proceed_ctl.sh start --session codex53-high --overnight-start 21 --overnight-end 7 --interval 15m
```

---

## CLI Reference

```
shape <old.csv> <new.csv> [OPTIONS]
shape witness <query|last|count> [OPTIONS]
```

### Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--key <column>` | string | *(none)* | Key column to check for alignment viability (uniqueness, coverage). |
| `--delimiter <delim>` | string | *(auto-detect)* | Force CSV delimiter for both files. See [Delimiter](#delimiter). |
| `--json` | flag | `false` | Emit a single JSON object on stdout instead of human-readable output. |
| `--no-witness` | flag | `false` | Suppress ambient witness ledger recording for this compare run. |
| `--profile <path>` | path | *(none)* | Reserved in v0: parsed, but not yet applied to check scoping. |
| `--profile-id <id>` | string | *(none)* | Reserved in v0: parsed and echoed as `profile_id` in JSON; no check-scoping effect yet. |
| `--lock <lockfile>` | path | *(none)* | Reserved in v0: parsed, but lock verification is not yet enforced. |
| `--max-rows <n>` | integer | *(unlimited)* | Reserved in v0: parsed, but row-limit refusal is not yet enforced. |
| `--max-bytes <n>` | integer | *(unlimited)* | Reserved in v0: parsed, but byte-limit refusal is not yet enforced. |
| `--describe` | flag | `false` | Print the compiled-in `operator.json` to stdout and exit `0` without positional args. |

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | COMPATIBLE |
| `1` | INCOMPATIBLE |
| `2` | REFUSAL or CLI error |

For `shape witness <...>` subcommands:
- `0` = one or more matching records returned
- `1` = no matches (or empty ledger for `last`)
- `2` = CLI parse error or witness internal error (for example, unreadable ledger path)

### Output Routing

| Mode | COMPATIBLE | INCOMPATIBLE | REFUSAL |
|------|------------|--------------|---------|
| Human (default) | stdout | stdout | stderr |
| `--json` | stdout | stdout | stdout |

In `--json` mode, stderr is reserved for process-level failures only (CLI parse errors, panics).

`--profile`, `--profile-id`, `--lock`, `--max-rows`, and `--max-bytes` are intentionally accepted early for operator-schema stability; runtime enforcement is deferred.

For witness subcommands:
- Human mode writes successful results to stdout and no-match/internal messages to stderr.
- `--json` writes structured payloads to stdout, while no-match messages still appear on stderr.

## Witness Subcommands

`shape` now accepts witness subcommand syntax:

```bash
shape witness query [--tool <name>] [--since <iso8601>] [--until <iso8601>] \
  [--outcome <COMPATIBLE|INCOMPATIBLE|REFUSAL>] [--input-hash <substring>] \
  [--limit <n>] [--json]

shape witness last [--json]

shape witness count [--tool <name>] [--since <iso8601>] [--until <iso8601>] \
  [--outcome <COMPATIBLE|INCOMPATIBLE|REFUSAL>] [--input-hash <substring>] [--json]
```

Current runtime behavior:
- `query` returns matching witness records.
- `last` returns the most recent witness record.
- `count` returns the number of matching records.
- All three commands read from `EPISTEMIC_WITNESS` when set, otherwise `~/.epistemic/witness.jsonl`.
- Malformed ledger lines are skipped; valid lines continue to be processed.

Ambient witness recording for compare runs is enabled by default; pass `--no-witness` to suppress appending for a specific run.

---

## The Three Outcomes

`shape` always produces exactly one of three outcomes. There are no partial results.

### 1. COMPATIBLE

All structural checks pass. These datasets can be meaningfully compared.

```
SHAPE

COMPATIBLE

Compared: nov.csv -> dec.csv
Key: loan_id (unique in both files)
Dialect(old): delimiter=, quote=" escape=none
Dialect(new): delimiter=, quote=" escape=none

Schema:    22 common / 22 total (100% overlap)
Key:       loan_id — unique in both, coverage=1.0
Rows:      3,214 old / 3,201 new (13 removed, 0 added, 3,201 overlap)
Types:     12 numeric columns, 0 type shifts
```

**How to read this:**
- **Schema** — how many columns are shared between the two files.
- **Key** — whether the key column is unique and non-null in both files.
- **Rows** — row counts and key overlap (how many keys appear in both files).
- **Types** — whether any columns changed from numeric to non-numeric or vice versa.

### 2. INCOMPATIBLE

One or more structural checks failed. The `reasons` field explains exactly what broke.

```
SHAPE

INCOMPATIBLE

Compared: nov.csv -> dec.csv
Key: loan_id (unique in both files)
Dialect(old): delimiter=, quote=" escape=none
Dialect(new): delimiter=, quote=" escape=none

Schema:    15 common / 17 total (88% overlap)
           old_only: [retired_field]
           new_only: [new_field]
Key:       loan_id — unique in both, coverage=1.0
Rows:      4,183 old / 4,201 new (33 removed, 51 added, 4,150 overlap)
Types:     12 numeric columns, 1 type shift
           balance: numeric -> non-numeric

Reasons:
  1. Type shift: balance changed from numeric to non-numeric
```

### 3. REFUSAL

When `shape` cannot parse or read the inputs. Always includes a concrete next step.

```
SHAPE ERROR (E_EMPTY)

Compared: nov.csv -> dec.csv
Dialect(old): delimiter=, quote=" escape=none

One or both files empty (no data rows after header)
Next: provide non-empty datasets.
```

---

## The Four Checks

`shape` runs four independent structural checks. All must pass for COMPATIBLE.

### Schema Overlap

Measures how many columns are shared between the two files.

- **Pass condition:** at least 1 common column (`overlap_ratio > 0`)
- Reports: `columns_common`, `columns_old_only`, `columns_new_only`, `overlap_ratio`

### Key Viability

Checks whether the key column is suitable for row alignment.

- **Pass condition:** key is unique in both files with no nulls
- Only checked when `--key` is provided
- Reports: `key_column`, `unique_old`, `unique_new`, `coverage`

### Row Granularity

Reports row counts and key overlap. Does not gate — agents and policies interpret the counts.

- **Always passes** — informational only
- Reports: `rows_old`, `rows_new`, `key_overlap`, `keys_old_only`, `keys_new_only`

### Type Consistency

Checks whether any common columns changed type between files.

- **Pass condition:** no columns changed from numeric to non-numeric or vice versa
- Only checked on columns common to both files
- Reports: `numeric_columns`, `type_shifts`

---

## Delimiter

### Auto-Detection (default)

Each file's delimiter is detected independently. Candidate delimiters are evaluated in order:
`,`, `\t`, `;`, `|`, `^`.

If detection is ambiguous (or the winner yields a single-column parse), `shape` refuses with
`E_DIALECT` and provides an actionable `next_command`.

### `sep=` Directive

If the first line is exactly `sep=<char>`, that delimiter is used for that file and the `sep=` line
is consumed (not treated as header data).

`--delimiter` still overrides `sep=` when both are present.

### `--delimiter` (forced)

Accepted values:

| Format | Examples |
|--------|----------|
| Named | `comma`, `tab`, `semicolon`, `pipe`, `caret` (case-insensitive) |
| Hex | `0x2c` (comma), `0x09` (tab) |
| Single ASCII char | `,`, `;`, `|`, `^`, `=` |

Rules:
- Hex form must be exactly two digits after `0x`.
- Allowed bytes are ASCII, excluding `"` (`0x22`), `\r`, `\n`, NUL (`0x00`), and DEL (`0x7f`).
- Invalid values fail as CLI argument errors (exit `2`).

---

## Refusal Codes

Every refusal includes the error code and a concrete next step.

| Code | Meaning | Next Step |
|------|---------|-----------|
| `E_IO` | File read error | Check file path and permissions |
| `E_ENCODING` | Unsupported encoding (UTF-16/32 BOM or NUL bytes) | Convert/re-export as UTF-8 |
| `E_CSV_PARSE` | CSV parse failure | Re-export as standard RFC4180 CSV |
| `E_EMPTY` | One or both files empty | Provide non-empty datasets |
| `E_HEADERS` | Missing header or duplicate headers | Fix headers or re-export |
| `E_DIALECT` | Delimiter ambiguous or undetectable | Use `--delimiter <delim>` |
| `E_AMBIGUOUS_PROFILE` | Both `--profile` and `--profile-id` provided | Provide exactly one profile selector |
| `E_INPUT_NOT_LOCKED` | Input not in any provided lockfile | Re-run with correct `--lock` or lock inputs first |
| `E_INPUT_DRIFT` | Input hash doesn't match locked member | Use the locked file; regenerate lock if expected |
| `E_TOO_LARGE` | Input exceeds `--max-rows` or `--max-bytes` | Increase limit or split input |

`E_AMBIGUOUS_PROFILE`, `E_INPUT_NOT_LOCKED`, `E_INPUT_DRIFT`, and `E_TOO_LARGE` are defined for schema stability, but not emitted yet in v0 runtime paths.

---

## JSON Output (`--json`)

A single JSON object on stdout. If the process fails before domain evaluation (e.g., invalid CLI args), JSON may not be emitted.

```jsonc
{
  "version": "shape.v0",
  "outcome": "COMPATIBLE",                 // "COMPATIBLE" | "INCOMPATIBLE" | "REFUSAL"
  "profile_id": null,                      // echoes --profile-id when provided
  "profile_sha256": null,                  // reserved in v0 (currently null)
  "input_verification": null,              // reserved in v0 (currently null)
  "files": { "old": "nov.csv", "new": "dec.csv" },
  "checks": {
    "schema_overlap": {
      "status": "pass",                    // "pass" | "fail"
      "columns_common": 15,
      "columns_old_only": ["retired_field"],
      "columns_new_only": ["new_field"],
      "overlap_ratio": 0.88
    },
    "key_viability": {
      "status": "pass",
      "key_column": "u8:loan_id",
      "found_old": true,
      "found_new": true,
      "unique_old": true,
      "unique_new": true,
      "coverage": 1.0
    },
    "row_granularity": {
      "status": "pass",
      "rows_old": 4183,
      "rows_new": 4201,
      "key_overlap": 4150,
      "keys_old_only": 33,
      "keys_new_only": 51
    },
    "type_consistency": {
      "status": "pass",
      "numeric_columns": 12,
      "type_shifts": []
    }
  },
  "reasons": [],                           // non-empty when INCOMPATIBLE
  "refusal": null                          // non-null when REFUSAL
}
```

### Nullable Field Rules

- `checks` is `null` for `REFUSAL`.
- `reasons` is `[]` for `COMPATIBLE`, non-empty for `INCOMPATIBLE`, and `null` for `REFUSAL`.
- `refusal` is `null` unless outcome is `REFUSAL`.
- `profile_id` echoes `--profile-id` when provided, otherwise `null`.
- `profile_sha256` and `input_verification` are reserved v0 contract fields and remain `null` in current runtime behavior.
- `key_viability` is `null` when `--key` is not provided.
- `key_viability.unique_old` / `unique_new` are `null` if the key column is missing in that file.
- `key_viability.coverage` is `null` when key overlap is not computable.
- `row_granularity.key_overlap` / `keys_old_only` / `keys_new_only` are `null` when key metrics are unavailable.

### Identifier Encoding (JSON)

Column names in JSON use unambiguous encoding:
- `u8:<string>` — valid UTF-8 with no ASCII control bytes (e.g., `u8:loan_id`)
- `hex:<hex-bytes>` — anything else (e.g., `hex:ff00ab`)

Same convention as `rvl`.

---

## Scripting Examples

Check if files are compatible (exit code only):

```bash
shape old.csv new.csv > /dev/null 2>&1
echo $?  # 0 = compatible, 1 = incompatible, 2 = refused
```

Extract schema overlap from JSON:

```bash
shape old.csv new.csv --json | jq '.checks.schema_overlap'
```

Get incompatibility reasons:

```bash
shape old.csv new.csv --json | jq '.reasons'
```

Gate a pipeline (shape before rvl):

```bash
shape nov.csv dec.csv --key loan_id --json > shape.json \
  && rvl nov.csv dec.csv --key loan_id --json > rvl.json
```

---

## Spec

The full specification is `docs/PLAN.md`. This README covers everything needed to use the tool; the spec adds implementation details, edge-case definitions, and testing requirements.

For canonical release/signoff docs, start at `docs/README.md`.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
