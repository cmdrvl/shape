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

## CLI Reference

```
shape <old.csv> <new.csv> [OPTIONS]
```

### Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--key <column>` | string | *(none)* | Key column to check for alignment viability (uniqueness, coverage). |
| `--delimiter <delim>` | string | *(auto-detect)* | Force CSV delimiter for both files. See rvl docs for accepted formats. |
| `--json` | flag | `false` | Emit a single JSON object on stdout instead of human-readable output. |
| `--profile <path>` | path | *(none)* | Scope checks to columns in this profile's `include_columns`. |
| `--profile-id <id>` | string | *(none)* | Profile ID (resolved from search path). Mutually exclusive with `--profile`. |
| `--lock <lockfile>` | path | *(none)* | Verify inputs are members of these lockfiles. Repeatable. |
| `--max-rows <n>` | integer | *(unlimited)* | Refuse if input exceeds N rows. |
| `--max-bytes <n>` | integer | *(unlimited)* | Refuse if input file exceeds N bytes. |

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | COMPATIBLE |
| `1` | INCOMPATIBLE |
| `2` | REFUSAL or CLI error |

### Output Routing

| Mode | COMPATIBLE | INCOMPATIBLE | REFUSAL |
|------|------------|--------------|---------|
| Human (default) | stdout | stdout | stderr |
| `--json` | stdout | stdout | stdout |

In `--json` mode, stderr is reserved for process-level failures only (CLI parse errors, panics).

---

## The Two Outcomes

`shape` always produces exactly one of two domain outcomes (or a refusal). There are no partial results.

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

One or both files are empty (no data rows after header).
Next: provide non-empty datasets.
```

---

## The Four Checks

`shape` runs four independent structural checks. All must pass for COMPATIBLE.

### Schema Overlap

Measures how many columns are shared between the two files.

- **Pass condition:** at least 1 common column (`overlap_ratio > 0`)
- Reports: `columns_common`, `columns_old_only`, `columns_new_only`, `overlap_ratio`
- When a profile is provided, overlap is measured against the profile's `include_columns`

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

---

## JSON Output (`--json`)

A single JSON object on stdout. If the process fails before domain evaluation (e.g., invalid CLI args), JSON may not be emitted.

```jsonc
{
  "version": "shape.v0",
  "outcome": "COMPATIBLE",                 // "COMPATIBLE" | "INCOMPATIBLE"
  "profile_id": null,                      // profile ID if --profile/--profile-id used
  "profile_sha256": null,                  // SHA256 if frozen profile, null if draft or none
  "input_verification": null,              // non-null when --lock is used
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
      "key_column": "loan_id",
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

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
