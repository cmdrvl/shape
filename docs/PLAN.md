# shape — Structural Comparability Gate

## One-line promise

**Deterministically answer: can these two CSV datasets be compared at all?**

If they can't be compared, say exactly why.

Second promise: **Gate before you diff. Know shape before you seek meaning.**

---

## Problem (clearly understood)

Before comparing two CSV files (with `rvl`, `compare`, or any other tool), you need to know:
- Do the columns match? (schema overlap)
- Is the key column actually unique? (key viability)
- How many rows were added or removed? (row granularity)
- Did any column change type? (type consistency)

Today this means:
- Manual header inspection
- Ad-hoc uniqueness checks
- Silent schema drift that breaks downstream analysis
- No structured answer to "are these comparable?"

`shape` replaces that with **one trusted gate**.

---

## Non-goals (explicit)

`shape` is NOT:
- A diff tool (that's `compare`)
- A materiality detector (that's `rvl`)
- A validation engine (that's `verify`)
- A profile manager (that's `profile`)

It does not tell you *what changed*.
It tells you *whether comparison is structurally meaningful*.

---

## Relationship to rvl

`shape` is the gate; `rvl` is the explanation. In a typical pipeline:

```bash
shape nov.csv dec.csv --key loan_id --json > shape.json \
  && rvl nov.csv dec.csv --key loan_id --json > rvl.json
```

If shape says INCOMPATIBLE, there's no point running rvl. If shape says COMPATIBLE, rvl can proceed with confidence that the structural prerequisites are met.

Both tools share:
- The same CSV parsing conventions (RFC4180, `sep=` directive, auto-detection, ASCII-trim)
- The same delimiter handling (auto-detect, `sep=`, `--delimiter`)
- The same identifier encoding (`u8:`/`hex:` in JSON)
- The same refusal system (`E_UPPERCASE` codes, concrete next steps)
- The same exit code conventions (0 = positive, 1 = negative, 2 = refusal)
- The same output routing (human mode: refusals to stderr; `--json` mode: everything to stdout)

Code should be structurally similar to rvl where applicable. shape should feel like the same family of tools.

---

## CLI (v0)

```bash
shape <old.csv> <new.csv> [--key <column>] [--delimiter <delim>] [--json]
```

### Flags (v0.1 — core)

- `--key <column>`: key column to check for alignment viability (uniqueness, empty values, coverage)
- `--delimiter <delim>`: force CSV delimiter for both files (same accepted values as rvl)
- `--json`: machine output (stable schema; no human formatting)
- `--describe`: print the compiled-in `operator.json` to stdout and exit 0. Checked before file arguments are validated, so `shape --describe` works with no positional args.
- `--version`: print `shape <semver>` to stdout and exit 0

### Flags (v0.1 — epistemic spine extensions)

These are deferred until the respective spine tools exist, but the CLI shape is defined now:

- `--profile <path>`: scope checks to columns in this profile's `include_columns`
- `--profile-id <id>`: profile ID (resolved from search path). Mutually exclusive with `--profile`
- `--lock <lockfile>`: verify inputs are members of these lockfiles (repeatable)
- `--max-rows <n>`: refuse if input exceeds N rows (default: unlimited)
- `--max-bytes <n>`: refuse if input file exceeds N bytes (default: unlimited)

### Exit codes

- `0`: COMPATIBLE
- `1`: INCOMPATIBLE
- `2`: REFUSAL / CLI error

### Streams

- Human mode: COMPATIBLE / INCOMPATIBLE go to stdout; REFUSAL goes to stderr.
- `--json` mode: emit exactly one JSON object on stdout for all domain outcomes; stderr is reserved for process-level failures only.

---

## Outcomes (exactly one)

### 1. COMPATIBLE

All structural checks pass. These datasets can be meaningfully compared.

Reports:
- Schema overlap (common columns, old-only columns, new-only columns, overlap ratio)
- Key viability (uniqueness, coverage) — when `--key` is provided
- Row granularity (counts, key overlap)
- Type consistency (numeric column count, type shifts)

### 2. INCOMPATIBLE

One or more structural checks failed. The `reasons` array explains exactly what broke.

Reports same fields as COMPATIBLE, plus `reasons` — a list of human-readable strings explaining each failure.

### 3. REFUSAL

When shape cannot parse or read the inputs. Always includes a concrete next step. Refusals are operator handoffs, never dead ends.

No other outcomes.

---

## The Four Checks

### Schema Overlap

**Question:** How many columns are shared between the two files?

**Implementation:**
1. Parse headers from both files (same normalization as rvl: ASCII-trim, empty → `__shape_col_<N>`)
2. Compute set intersection and set differences
3. Calculate `overlap_ratio = columns_common / columns_total` (where `columns_total` is the union)

**Pass condition:** `overlap_ratio > 0` (at least 1 common column)

**When a profile is provided:** overlap is measured only against the profile's `include_columns`. Columns outside the profile are ignored.

**Output fields:**

| Field | Type | Notes |
|-------|------|-------|
| `status` | string | `"pass"` or `"fail"` |
| `columns_common` | u64 | Count of columns in both files |
| `columns_old_only` | string[] | Column names only in old file (encoded) |
| `columns_new_only` | string[] | Column names only in new file (encoded) |
| `overlap_ratio` | f64 | `columns_common / (columns_common + columns_old_only.len + columns_new_only.len)` |

### Key Viability

**Question:** Is the key column suitable for row alignment?

**Implementation:**
1. Only checked when `--key` is provided. If no `--key`, this check is null in JSON and omitted in human output.
2. Verify the key column exists in both files' headers (after normalization). If not found → the check **fails** (status `"fail"`) with a reason like `"Key viability: loan_id not found in new file"`. This is INCOMPATIBLE, not a refusal — the files parsed fine, the key just doesn't exist.
3. Scan all rows in both files (part of the single-pass scan)
4. Check: key values are non-empty (after ASCII-trim) and unique within each file
5. Compute coverage: `key_overlap / max(keys_old, keys_new)`. If both key sets are empty, coverage = 0.0.

**Pass condition:** key column exists in both files AND is unique in both AND has no empty values

**Output fields:**

| Field | Type | Notes |
|-------|------|-------|
| `status` | string | `"pass"` or `"fail"` |
| `key_column` | string | Encoded column name |
| `found_old` | bool | Key column exists in old file's headers |
| `found_new` | bool | Key column exists in new file's headers |
| `unique_old` | bool? | No duplicates AND no empty/missing values in old (null if key not found in old) |
| `unique_new` | bool? | No duplicates AND no empty/missing values in new (null if key not found in new) |
| `coverage` | f64? | Key overlap ratio (null if key not found in either file) |

### Row Granularity

**Question:** How many rows are in each file, and how much do they overlap?

**Implementation:**
1. Count rows in both files
2. If `--key` is provided: compute key set intersection and differences
3. If no `--key`: report row counts only (overlap fields are null)

**Pass condition:** Always passes — this check is informational only. Agents and policies interpret the counts.

**Output fields:**

| Field | Type | Notes |
|-------|------|-------|
| `status` | string | Always `"pass"` |
| `rows_old` | u64 | Row count in old file |
| `rows_new` | u64 | Row count in new file |
| `key_overlap` | u64? | Keys in both files (null if no `--key` or key not found) |
| `keys_old_only` | u64? | Keys only in old file (null if no `--key` or key not found) |
| `keys_new_only` | u64? | Keys only in new file (null if no `--key` or key not found) |

### Type Consistency

**Question:** Did any common column change from numeric to non-numeric (or vice versa)?

**Implementation:**
1. For each common column, classify it in each file using the single-pass scan results (every row is examined, not sampled)
2. Classification: if every non-missing value parses as a finite number → Numeric. If any non-missing value doesn't parse → NonNumeric. If all values are missing → AllMissing.
3. A "type shift" is when a column is Numeric in one file and NonNumeric in the other. AllMissing is compatible with both — it never triggers a type shift (there was no established type to shift from).

**Pass condition:** No type shifts detected

**Output fields:**

| Field | Type | Notes |
|-------|------|-------|
| `status` | string | `"pass"` or `"fail"` |
| `numeric_columns` | u64 | Count of common columns classified numeric in both files |
| `type_shifts` | object[] | `[{ "column": "u8:...", "old_type": "numeric", "new_type": "non-numeric" }]` |

---

## Default Check Thresholds (v0)

| Check | Pass condition | Notes |
|-------|---------------|-------|
| `schema_overlap` | `overlap_ratio > 0` (at least 1 common column) | Profiles override: when provided, overlap is measured against `include_columns` |
| `key_viability` | Key exists in both files, is unique in both, and has no empty values | Only checked when `--key` is provided |
| `row_granularity` | Always passes — reports row/key counts but does not gate | Agents or policies interpret the counts |
| `type_consistency` | No columns changed from numeric to non-numeric or vice versa | Only checked on columns common to both files |

These are intentionally permissive defaults. Strict gating is the job of `assess` policies, not `shape`. Shape reports structure; policies judge it.

---

## Input Contract (CSV Only)

shape follows the exact same CSV parsing contract as rvl:

- Byte-oriented CSV (no encoding assumption; UTF-8 BOM allowed and stripped)
- Header required (first record after optional `sep=`)
- Optional `sep=<char>` delimiter directive (same rules as rvl)
- Delimiter auto-detection per file (same algorithm as rvl: `,` → `\t` → `;` → `|` → `^`)
- `--delimiter` forces delimiter for both files
- RFC4180 quoting with backslash-escape fallback
- Header names are ASCII-trimmed; empty names normalized to `__shape_col_<N>`
- Column names must be unique within each file after normalization
- Duplicate headers → REFUSAL (`E_HEADERS`)
- Empty file (no header) → REFUSAL (`E_HEADERS`)

### `sep=` directive

Same rules as rvl:
- After BOM strip, check if the first line matches exactly `sep=X` where X is a single byte
- If match: consume that line (it is NOT part of the header), use X as the delimiter for that file, skip dialect auto-detection for that file
- `--delimiter` overrides `sep=` when both are present
- Per-file: old and new can each have their own `sep=` line independently

### Delimiter auto-detection

Same algorithm as rvl. Candidate delimiters in priority order: `,` → `\t` → `;` → `|` → `^`.

For each candidate:
1. Try RFC4180 escape mode (no escape character)
2. If that fails, try backslash-escape mode
3. Score: build histogram of field counts across sample records (max 200 records, max 64 KB)
4. Score tuple: `(records_parsed, mode_count, mode_fields)` — compared lexicographically

Winner selection:
- If one candidate has strictly the best score → use it
- If multiple candidates have identical scores from identical samples → use the one with highest priority (comma > tab > semicolon > pipe > caret)
- If identical scores but different samples → refuse with `E_DIALECT`
- If the winning delimiter produces only 1 column → refuse with `E_DIALECT`

### Delimiter `--delimiter` parsing

Accepted values (case-insensitive keywords, hex, or single-char literal):
- Named: `comma`, `tab`, `semicolon`, `pipe`, `caret`
- Hex: `0x2c` (exactly two hex digits after `0x`)
- Literal: `,` (single ASCII character)
- Rejects: `0x00`, `0x7F+`, `"` (0x22), `\r` (0x0D), `\n` (0x0A)

### Blank records

Blank records (every field empty after ASCII-trim) are skipped before counting, same as rvl.

### Numeric detection

For type consistency checking, shape uses the same numeric detection as rvl:
- Plain: `123`, `-123.45`, `1e6`
- Thousands separators: `1,234`, `-1,234,567.89`
- Currency prefix: `$123.45`, `-$1,234.56`
- Accounting parentheses: `(123.45)` → negative
- Missing tokens (case-insensitive): empty, `-`, `NA`, `N/A`, `NULL`, `NAN`, `NONE`

A column is numeric if every non-missing value parses as a finite number.

---

## Refusal Codes

| Code | Trigger | Next step |
|------|---------|-----------|
| `E_IO` | Can't read file | Check file path and permissions |
| `E_ENCODING` | Unsupported encoding (UTF-16/32 BOM or NUL bytes) | Convert/re-export as UTF-8 |
| `E_CSV_PARSE` | Can't parse as CSV | Re-export as standard RFC4180 CSV |
| `E_EMPTY` | One or both files empty (no data rows after header) | Provide non-empty datasets |
| `E_HEADERS` | Missing header or duplicate headers | Fix headers or re-export |
| `E_DIALECT` | Delimiter ambiguous or undetectable | Use `--delimiter <delim>` |
| `E_AMBIGUOUS_PROFILE` | Both `--profile` and `--profile-id` were provided | Provide exactly one profile selector |
| `E_INPUT_NOT_LOCKED` | Input file not present in any provided lockfile | Re-run with correct `--lock` or lock inputs first |
| `E_INPUT_DRIFT` | Input file hash doesn't match the referenced lock member | Use the locked file; regenerate lock if expected |
| `E_TOO_LARGE` | Input exceeds `--max-rows` or `--max-bytes` | Increase limit or split input |

Refusal envelope (same as all spine tools):

```json
{
  "code": "E_UPPERCASE",
  "message": "short human-readable reason",
  "detail": { },
  "next_command": "shape nov.csv dec.csv --delimiter tab --json"
}
```

`next_command` is a concrete re-run suggestion when actionable (`E_DIALECT`, `E_TOO_LARGE`), otherwise `null`.

### Refusal detail schemas

Each refusal code has a specific `detail` object:

```
E_IO:
  { "file": "nov.csv", "error": "No such file or directory" }

E_ENCODING:
  { "file": "nov.csv", "issue": "utf32_be_bom" | "utf32_le_bom" | "utf16_be_bom" | "utf16_le_bom" | "nul_byte" }

E_CSV_PARSE:
  { "file": "nov.csv", "line": 42, "error": "..." }

E_EMPTY:
  { "file": "dec.csv", "rows": 0 }

E_HEADERS:
  { "file": "nov.csv", "issue": "duplicate" | "missing",
    "name": "u8:amount" }
  "name" is present only for "duplicate"; encoded via u8:/hex: convention.

E_DIALECT:
  { "file": "nov.csv", "candidates": ["0x2c", "0x09"] }
  next_command: "shape nov.csv dec.csv --delimiter tab --json"

E_AMBIGUOUS_PROFILE:
  { "profile_path": "...", "profile_id": "..." }

E_INPUT_NOT_LOCKED:
  { "file": "nov.csv" }

E_INPUT_DRIFT:
  { "file": "nov.csv", "expected_hash": "sha256:...", "actual_hash": "sha256:..." }

E_TOO_LARGE:
  { "file": "nov.csv", "limit_flag": "--max-rows", "limit": 10000, "actual": 50000 }
  next_command: "shape nov.csv dec.csv --max-rows 50000 --json"
```

---

## JSON Output Schema (`shape.v0`)

All domain outcomes emit exactly one JSON object on stdout. Every field is always present (no omitted keys). Nullable fields use `null`, never omission.

### Nullable field rules

- `profile_id`, `profile_sha256`: null unless `--profile` / `--profile-id` used
- `input_verification`: null unless `--lock` used
- `key_viability`: null if no `--key` provided
- `key_viability.unique_old`, `unique_new`: null if key column not found in that file
- `key_viability.coverage`: null if key column not found in either file
- `row_granularity.key_overlap`, `keys_old_only`, `keys_new_only`: null if no `--key` or if key not found in either file
- `row_granularity.rows_old`, `rows_new`: always present
- `checks`: null when outcome is REFUSAL
- `reasons`: empty `[]` when COMPATIBLE, non-empty when INCOMPATIBLE, null when REFUSAL
- `refusal`: null when COMPATIBLE or INCOMPATIBLE

### JSON float precision

All f64 values (`overlap_ratio`, `coverage`) are emitted at full precision (serde default — enough digits to roundtrip the f64). No rounding. The examples below use simplified values for readability; actual output will show full precision (e.g., `0.8823529411764706` not `0.88`).

### File paths in output

`files.old` and `files.new` display exactly what the user passed on the command line. No normalization, no basename extraction. If the user passes `../../data/nov.csv`, that's what appears.

### COMPATIBLE (with `--key`)

```json
{
  "version": "shape.v0",
  "outcome": "COMPATIBLE",
  "profile_id": null,
  "profile_sha256": null,
  "input_verification": null,
  "files": { "old": "nov.csv", "new": "dec.csv" },
  "dialect": {
    "old": { "delimiter": ",", "quote": "\"", "escape": "none" },
    "new": { "delimiter": ",", "quote": "\"", "escape": "none" }
  },
  "checks": {
    "schema_overlap": {
      "status": "pass",
      "columns_common": 15,
      "columns_old_only": ["u8:retired_field"],
      "columns_new_only": ["u8:new_field"],
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
      "rows_new": 4183,
      "key_overlap": 4183,
      "keys_old_only": 0,
      "keys_new_only": 0
    },
    "type_consistency": {
      "status": "pass",
      "numeric_columns": 12,
      "type_shifts": []
    }
  },
  "reasons": [],
  "refusal": null
}
```

### COMPATIBLE (without `--key`)

```json
{
  "version": "shape.v0",
  "outcome": "COMPATIBLE",
  "profile_id": null,
  "profile_sha256": null,
  "input_verification": null,
  "files": { "old": "nov.csv", "new": "dec.csv" },
  "dialect": {
    "old": { "delimiter": ",", "quote": "\"", "escape": "none" },
    "new": { "delimiter": ",", "quote": "\"", "escape": "none" }
  },
  "checks": {
    "schema_overlap": {
      "status": "pass",
      "columns_common": 22,
      "columns_old_only": [],
      "columns_new_only": [],
      "overlap_ratio": 1.0
    },
    "key_viability": null,
    "row_granularity": {
      "status": "pass",
      "rows_old": 3214,
      "rows_new": 3201,
      "key_overlap": null,
      "keys_old_only": null,
      "keys_new_only": null
    },
    "type_consistency": {
      "status": "pass",
      "numeric_columns": 12,
      "type_shifts": []
    }
  },
  "reasons": [],
  "refusal": null
}
```

### INCOMPATIBLE

All four checks are always present (when not REFUSAL), even passing ones. This lets consumers see the full structural picture.

```json
{
  "version": "shape.v0",
  "outcome": "INCOMPATIBLE",
  "profile_id": null,
  "profile_sha256": null,
  "input_verification": null,
  "files": { "old": "nov.csv", "new": "dec.csv" },
  "dialect": {
    "old": { "delimiter": ",", "quote": "\"", "escape": "none" },
    "new": { "delimiter": ",", "quote": "\"", "escape": "none" }
  },
  "checks": {
    "schema_overlap": {
      "status": "pass",
      "columns_common": 15,
      "columns_old_only": ["u8:retired_field"],
      "columns_new_only": ["u8:new_field"],
      "overlap_ratio": 0.88
    },
    "key_viability": {
      "status": "pass",
      "key_column": "u8:loan_id",
      "found_old": true,
      "found_new": true,
      "unique_old": true,
      "unique_new": true,
      "coverage": 0.99
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
      "status": "fail",
      "numeric_columns": 12,
      "type_shifts": [
        { "column": "u8:balance", "old_type": "numeric", "new_type": "non-numeric" }
      ]
    }
  },
  "reasons": ["Type shift: balance changed from numeric to non-numeric"],
  "refusal": null
}
```

### REFUSAL

All top-level fields are present (no omitted keys). `checks` and `reasons` are null. `dialect` includes whatever was gathered before the failure — each side is null only if that file couldn't be parsed at all (e.g., E_IO or E_ENCODING on that file).

E_EMPTY fires at step 11 or step 16 (both are after header parsing), so both dialects are known:

```json
{
  "version": "shape.v0",
  "outcome": "REFUSAL",
  "profile_id": null,
  "profile_sha256": null,
  "input_verification": null,
  "files": { "old": "nov.csv", "new": "dec.csv" },
  "dialect": {
    "old": { "delimiter": ",", "quote": "\"", "escape": "none" },
    "new": { "delimiter": ",", "quote": "\"", "escape": "none" }
  },
  "checks": null,
  "reasons": null,
  "refusal": {
    "code": "E_EMPTY",
    "message": "One or both files are empty",
    "detail": { "file": "dec.csv", "rows": 0 },
    "next_command": null
  }
}
```

When a refusal fires before new file is parsed (e.g., E_IO on new), `dialect.new` is null while `dialect.old` is populated.

### Input Verification (when `--lock` is used)

```json
{
  "input_verification": {
    "verified": true,
    "locks": [
      { "path": "dec.lock.json", "lock_hash": "sha256:..." }
    ],
    "files": {
      "old": { "path": "nov.csv", "bytes_hash": "sha256:..." },
      "new": { "path": "dec.csv", "bytes_hash": "sha256:..." }
    }
  }
}
```

---

## Human Output Format

### Layout rules

The human output has a fixed structure with conditional sections:

For COMPATIBLE / INCOMPATIBLE:

```
SHAPE                              ← always
                                   ← blank line
{OUTCOME}                          ← COMPATIBLE | INCOMPATIBLE
                                   ← blank line
Compared: {old_path} -> {new_path} ← always; paths are exactly as user passed them
Key: {column} ({viability_note})   ← only when --key provided
Dialect(old): ...                  ← always
Dialect(new): ...                  ← always
                                   ← blank line
Schema:    ...                     ← always
Key:       ...                     ← only when --key provided
Rows:      ...                     ← always
Types:     ...                     ← always
                                   ← blank line (only if INCOMPATIBLE)
Reasons:                           ← only if INCOMPATIBLE
  1. ...
```

For REFUSAL (different header — combined first line):

```
SHAPE ERROR ({E_CODE})             ← first line combines SHAPE + error code
                                   ← blank line
Compared: {old_path} -> {new_path} ← always
Dialect(old): ...                  ← if old was parsed
Dialect(new): ...                  ← if new was parsed (omitted if not)
                                   ← blank line
{message}                          ← refusal message
Next: {next_step}                  ← concrete remediation
```

### Number formatting

- Row counts: comma-separated thousands (`3,214`, `4,183`)
- Overlap ratio: displayed as integer percentage (`100%`, `88%`) — round to nearest
- Coverage: 1-2 significant decimal places (`1.0`, `0.85`)
- Numeric column count: plain integer (`12`)

### Dialect display

Format: `delimiter={d} quote={q} escape={e}`
- Delimiter: the literal character (`,`, `\t` for tab, `;`, `|`, `^`)
- Quote: always `"`
- Escape: `none` (RFC4180) or `backslash`
- Tab is displayed as `\t`, not a literal tab character

### COMPATIBLE (with `--key`)

```
SHAPE

COMPATIBLE

Compared: nov.csv -> dec.csv
Key: loan_id (unique in both files)
Dialect(old): delimiter=, quote=" escape=none
Dialect(new): delimiter=, quote=" escape=none

Schema:    22 common / 22 total (100% overlap)
Key:       loan_id — unique in both, coverage=1.0
Rows:      3,214 old / 3,214 new (0 removed, 0 added, 3,214 overlap)
Types:     12 numeric columns, 0 type shifts
```

### COMPATIBLE (without `--key`)

```
SHAPE

COMPATIBLE

Compared: nov.csv -> dec.csv
Dialect(old): delimiter=, quote=" escape=none
Dialect(new): delimiter=, quote=" escape=none

Schema:    22 common / 22 total (100% overlap)
Rows:      3,214 old / 3,201 new
Types:     12 numeric columns, 0 type shifts
```

No `Key:` lines at all. Rows line shows only counts (no overlap detail).

### INCOMPATIBLE

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
Key:       loan_id — unique in both, coverage=0.99
Rows:      4,183 old / 4,201 new (33 removed, 51 added, 4,150 overlap)
Types:     12 numeric columns, 1 type shift
           balance: numeric -> non-numeric

Reasons:
  1. Type shift: balance changed from numeric to non-numeric
```

### INCOMPATIBLE (key fails viability)

```
SHAPE

INCOMPATIBLE

Compared: nov.csv -> dec.csv
Key: loan_id (NOT VIABLE — 42 duplicates in old)
Dialect(old): delimiter=, quote=" escape=none
Dialect(new): delimiter=, quote=" escape=none

Schema:    22 common / 22 total (100% overlap)
Key:       loan_id — 42 duplicates in old, coverage=0.95
Rows:      4,200 old / 4,100 new (208 removed, 150 added, 3,950 overlap)
Types:     12 numeric columns, 0 type shifts

Reasons:
  1. Key viability: loan_id has 42 duplicate values in old file
```

Math check: 4,200 rows − 42 duplicates = 4,158 unique keys in old. 4,100 rows = 4,100 unique keys in new. Overlap = 3,950. keys\_old\_only = 4,158 − 3,950 = 208. keys\_new\_only = 4,100 − 3,950 = 150. Coverage = 3,950 / max(4,158, 4,100) = 3,950 / 4,158 ≈ 0.95.

### INCOMPATIBLE (key not found)

```
SHAPE

INCOMPATIBLE

Compared: nov.csv -> dec.csv
Key: loan_id (NOT FOUND in new file)
Dialect(old): delimiter=, quote=" escape=none
Dialect(new): delimiter=, quote=" escape=none

Schema:    22 common / 22 total (100% overlap)
Key:       loan_id — not found in new file
Rows:      4,183 old / 4,201 new
Types:     12 numeric columns, 0 type shifts

Reasons:
  1. Key viability: loan_id not found in new file
```

When key is not found, key scan is skipped — no coverage or overlap detail in the Rows line (same as no-key mode).

### REFUSAL

Refusals include whatever context was gathered before the failure. If old was parsed but new fails, old's dialect is shown.

```
SHAPE ERROR (E_EMPTY)

Compared: nov.csv -> dec.csv
Dialect(old): delimiter=, quote=" escape=none
Dialect(new): delimiter=, quote=" escape=none

One or both files are empty (no data rows after header).
Next: provide non-empty datasets.
```

### Reasons templates

Exact format strings for the `reasons` array (both human and JSON):

```
"Schema overlap: 0 common columns (old={n}, new={n})"
"Key viability: {column} has {n} duplicate values in {file} file"
"Key viability: {column} has {n} empty values in {file} file"
"Key viability: {column} not found in {file} file"
"Type shift: {column} changed from numeric to non-numeric"
"Type shift: {column} changed from non-numeric to numeric"
```

Multiple reasons can accumulate (e.g., key has duplicates AND a type shift occurred).

---

## Identifier Encoding (JSON)

All identifiers (column names) in JSON output use a tagged encoding:

- `"u8:<string>"` — valid UTF-8 with no ASCII control bytes (0x00–0x1F, 0x7F)
- `"hex:<lowercase-hex>"` — everything else

Same convention as rvl. File paths are emitted as-is (no prefix).

---

## Implementation Notes

### Execution flow

The `run()` function in `lib.rs` orchestrates the full pipeline. **Fail fast on first refusal** — if old file can't parse, don't attempt new. Context (parsed dialect, etc.) carries forward so refusal output includes whatever was gathered before the failure.

```
 1. Parse CLI args (clap)           → exit 2 on bad args; --version handled here by clap
 2. If --describe: print operator.json to stdout, exit 0
 3. Read old file bytes             → E_IO if fail (STOP)
 4. Guard encoding old              → E_ENCODING if fail (STOP)
 5. Detect sep= / auto-detect / forced delimiter old → E_DIALECT if fail (STOP)
 6. Parse headers old               → E_HEADERS / E_CSV_PARSE if fail (STOP)
 7. Read new file bytes             → E_IO if fail (STOP)
 8. Guard encoding new              → E_ENCODING if fail (STOP)
 9. Detect sep= / auto-detect / forced delimiter new → E_DIALECT if fail (STOP)
10. Parse headers new               → E_HEADERS / E_CSV_PARSE if fail (STOP)
11. Check both have data rows       → E_EMPTY if fail (STOP)
    (Quick check: are there bytes after the header? Catches the common case.
     Edge case: all-blank files are caught post-scan — see note below.)
12. Compute common columns          (set intersection of normalized headers)
13. If --key: find key column index in both headers
    (if not found in one/both: note failure for key_viability, skip key scan)
14. Single-pass scan old file       → row count, key scan, column types
15. Single-pass scan new file       → row count, key scan, column types
16. Post-scan E_EMPTY guard         → if either row_count is 0, E_EMPTY (STOP)
    (Catches all-blank files that passed the step 11 quick check.)
17. Evaluate all four checks from scan results
18. Determine outcome               (all pass → COMPATIBLE, any fail → INCOMPATIBLE)
19. Build reasons array             (collect failure explanations)
20. Render output                   (human or JSON based on --json flag)
21. Route to stream                 (stdout or stderr)
22. Return exit code                (0, 1, or 2)
```

Steps 3-6 produce `ParsedInput` for old. Steps 7-10 produce `ParsedInput` for new. If any step in 3-6 fails, we have no old context (refusal shows what we can). If any step in 7-10 fails, we have old's dialect but not new's (refusal shows old's dialect).

### Core data structures

```rust
// === Top-level result ===

pub struct PipelineResult {
    pub outcome: Outcome,
    pub output: String,   // pre-rendered output (human or JSON string)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Compatible,    // exit 0
    Incompatible,  // exit 1
    Refusal,       // exit 2
}

// === CLI ===

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode { Human, Json }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream { Stdout, Stderr }

// Routing logic (functions, not hardcoded):
// exit_code(Outcome) → u8
// output_stream(Outcome, OutputMode) → OutputStream
//   Json + anything → Stdout
//   Human + Refusal → Stderr
//   Human + anything else → Stdout

// === Parsed input (one per file) ===

pub struct ParsedInput {
    pub path: PathBuf,            // as passed by user
    pub raw_bytes: Vec<u8>,       // file content after BOM strip
    pub dialect: Dialect,
    pub headers: Vec<Vec<u8>>,    // normalized header names
    pub data_offset: usize,       // byte offset where data rows begin
}

pub struct Dialect {
    pub delimiter: u8,
    pub quote: u8,                // always b'"'
    pub escape: EscapeMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeMode {
    None,       // RFC4180 (doubled quotes)
    Backslash,  // backslash before quote
}

// === Scan results (one per file, from single-pass scan) ===

pub struct ScanResult {
    pub row_count: u64,
    pub key_scan: Option<KeyScan>,                // only if --key AND key found in headers
    pub column_types: Vec<ColumnClassification>,   // one per common column, same order
}

pub struct KeyScan {
    pub values: HashSet<Vec<u8>>,   // all distinct key values seen
    pub duplicate_count: u64,        // number of rows where key was already seen
    pub empty_count: u64,            // rows where key is empty/missing after trim
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnClassification {
    Numeric,      // all non-missing values parse as finite numbers
    NonNumeric,   // at least one non-missing value is not numeric
    AllMissing,   // every value is a missing token — compatible with both Numeric and NonNumeric
}

// === Check results ===

pub struct CheckSuite {
    pub schema_overlap: SchemaOverlapResult,
    pub key_viability: Option<KeyViabilityResult>,   // None when no --key
    pub row_granularity: RowGranularityResult,
    pub type_consistency: TypeConsistencyResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus { Pass, Fail }

pub struct SchemaOverlapResult {
    pub status: CheckStatus,
    pub columns_common: Vec<Vec<u8>>,    // ordered by position in old file
    pub columns_old_only: Vec<Vec<u8>>,
    pub columns_new_only: Vec<Vec<u8>>,
    pub overlap_ratio: f64,              // columns_common.len / union.len
}

pub struct KeyViabilityResult {
    pub status: CheckStatus,
    pub key_column: Vec<u8>,      // normalized key name
    pub found_old: bool,
    pub found_new: bool,
    pub unique_old: Option<bool>,  // None when key not found in old
    pub unique_new: Option<bool>,  // None when key not found in new
    pub coverage: Option<f64>,     // None when key not found in either file
}

pub struct RowGranularityResult {
    pub status: CheckStatus,       // always Pass
    pub rows_old: u64,
    pub rows_new: u64,
    pub key_overlap: Option<u64>,
    pub keys_old_only: Option<u64>,
    pub keys_new_only: Option<u64>,
}

pub struct TypeConsistencyResult {
    pub status: CheckStatus,
    pub numeric_columns: u64,      // count of common columns classified numeric in both
    pub type_shifts: Vec<TypeShift>,
}

pub struct TypeShift {
    pub column: Vec<u8>,
    pub old_type: ColumnClassification,
    pub new_type: ColumnClassification,
}

// === Refusal payload ===

pub struct RefusalPayload {
    pub code: RefusalCode,
    pub message: &'static str,
    pub detail: serde_json::Value,
    pub next_command: Option<String>,
}
```

### Outcome determination

After all four checks are evaluated:

```rust
fn determine_outcome(suite: &CheckSuite) -> Outcome {
    let failed = suite.schema_overlap.status == Fail
        || suite.key_viability.as_ref().is_some_and(|k| k.status == Fail)
        || suite.type_consistency.status == Fail;
    // row_granularity never fails (always Pass)

    if failed { Outcome::Incompatible } else { Outcome::Compatible }
}
```

Reasons are collected from every check that failed — multiple reasons can accumulate in one run.

### Single-pass scan architecture

**This is the performance-critical section.** Each file gets exactly ONE pass through its data rows. That pass collects everything simultaneously:

```rust
fn scan_file(
    raw_bytes: &[u8],
    data_offset: usize,
    dialect: &Dialect,
    common_column_indices: &[usize],  // indices into this file's headers
    key_column_index: Option<usize>,
) -> Result<ScanResult, RefusalPayload> {
    let mut row_count: u64 = 0;
    let mut key_scan: Option<KeyScan> = key_column_index.map(|_| KeyScan::new());
    let mut classifiers: Vec<NumericClassifier> = vec![NumericClassifier::new(); common_column_indices.len()];

    // Stream records using csv crate's ByteRecord iterator
    // NEVER collect all records into a Vec
    for record in csv_reader.byte_records() {
        let record = record?;

        // Skip blank records (all fields empty after ASCII-trim)
        if is_blank_record(&record) { continue; }

        row_count += 1;

        // Key tracking
        if let (Some(idx), Some(ref mut ks)) = (key_column_index, &mut key_scan) {
            let key_value = ascii_trim(record.get(idx));
            if is_missing(key_value) {
                ks.empty_count += 1;
            } else if !ks.values.insert(key_value.to_vec()) {
                ks.duplicate_count += 1;
            }
        }

        // Type classification per common column
        for (i, &col_idx) in common_column_indices.iter().enumerate() {
            let value = ascii_trim(record.get(col_idx));
            classifiers[i].observe(value);
        }
    }

    Ok(ScanResult {
        row_count,
        key_scan,
        column_types: classifiers.into_iter().map(|c| c.classify()).collect(),
    })
}
```

**Memory profile:** Both files are loaded fully into memory (steps 3 and 7 read entire files into `Vec<u8>`), so baseline memory is O(F_old + F_new) where F is file size. Beyond that, the only growing structure is the key HashSet: O(K) where K is the number of distinct key values. Column classifiers are O(1) each (two booleans). Row records are never stored — the csv reader streams over the in-memory byte slice.

**NumericClassifier:**

```rust
struct NumericClassifier {
    seen_non_missing: bool,
    seen_non_numeric: bool,
}

impl NumericClassifier {
    fn observe(&mut self, value: &[u8]) {
        if is_missing(value) { return; }
        self.seen_non_missing = true;
        if self.seen_non_numeric { return; }  // early termination — already decided
        if !parses_as_numeric(value) {
            self.seen_non_numeric = true;
        }
    }

    fn classify(&self) -> ColumnClassification {
        if !self.seen_non_missing { ColumnClassification::AllMissing }
        else if self.seen_non_numeric { ColumnClassification::NonNumeric }
        else { ColumnClassification::Numeric }
    }
}
```

**Early termination optimization:** Once `seen_non_numeric` is true for a column, further observations for that column can skip the expensive `parses_as_numeric` call. This is a significant optimization for wide files where most columns are non-numeric.

### Key overlap computation

After scanning both files, if `--key` is provided AND key was found in both files (both key_scans are Some), compute key set overlap:

```rust
// Only computed when key was found in both files (both key_scans are Some).
// When key is not found in either file, key_scan is None → skip overlap computation,
// and key_overlap/keys_old_only/keys_new_only are null in output.
if let (Some(ks_old), Some(ks_new)) = (&scan_old.key_scan, &scan_new.key_scan) {
    let key_overlap = ks_old.values.intersection(&ks_new.values).count() as u64;
    let keys_old_only = ks_old.values.len() as u64 - key_overlap;
    let keys_new_only = ks_new.values.len() as u64 - key_overlap;
    let coverage = if ks_old.values.is_empty() && ks_new.values.is_empty() {
        0.0
    } else {
        key_overlap as f64 / ks_old.values.len().max(ks_new.values.len()) as f64
    };
}
```

### BOM handling and encoding guard

Same as rvl. Order matters:

```rust
fn guard_input_bytes(input: &[u8]) -> Result<&[u8], RefusalPayload> {
    // 1. Check for UTF-16/UTF-32 BOMs → E_ENCODING
    //    UTF-32 BE: [0x00, 0x00, 0xFE, 0xFF]
    //    UTF-32 LE: [0xFF, 0xFE, 0x00, 0x00]
    //    UTF-16 BE: [0xFE, 0xFF]
    //    UTF-16 LE: [0xFF, 0xFE]
    //    (Check UTF-32 before UTF-16 — UTF-32 LE starts with FF FE too)

    // 2. Strip UTF-8 BOM if present: [0xEF, 0xBB, 0xBF] → no error, just skip 3 bytes

    // 3. Check for NUL bytes in first 8KB → E_ENCODING
    //    input.iter().take(8192).any(|&b| b == 0)

    // 4. Return remaining bytes (after BOM strip)
}
```

### `--key` column matching

The `--key <column>` value is matched against normalized headers byte-for-byte (case-sensitive, after ASCII-trim). The CLI value is UTF-8 encoded and compared directly against each normalized header as bytes. If no header matches, the check fails with reason `"Key viability: {column} not found in {file} file"` — this is INCOMPATIBLE, not a refusal.

### Header normalization

Same as rvl, with `__shape_col_<N>` instead of `__rvl_col_<N>`:

- ASCII-trim each header: strip only 0x20 (space) and 0x09 (tab) from both ends
- If empty after trim → generate `__shape_col_<N>` (1-indexed)
- Check uniqueness: byte-for-byte, case-sensitive (`Foo` ≠ `foo`)
- Duplicate → `E_HEADERS` refusal with the duplicate name

### Identifier encoding (u8:/hex:)

Same as rvl:

```rust
fn encode_identifier(bytes: &[u8]) -> String {
    if std::str::from_utf8(bytes).is_ok()
        && !bytes.iter().any(|&b| b <= 0x1F || b == 0x7F)
    {
        format!("u8:{}", std::str::from_utf8(bytes).unwrap())
    } else {
        let hex: String = bytes.iter()
            .flat_map(|b| [HEX[(b >> 4) as usize], HEX[(b & 0x0F) as usize]])
            .collect();
        format!("hex:{hex}")
    }
}
```

Used in JSON output for all column names. File paths are emitted as-is (no prefix).

### Module structure

```
src/
├── cli/
│   ├── args.rs          # clap derive Args struct
│   ├── delimiter.rs     # Delimiter parsing (shared logic with rvl)
│   ├── exit.rs          # Outcome, OutputMode, OutputStream, exit_code(), output_stream()
│   └── mod.rs
├── csv/
│   ├── dialect.rs       # Auto-detection (same algorithm as rvl)
│   ├── input.rs         # File reading, BOM handling, encoding guard
│   ├── parser.rs        # CSV record iteration (streaming ByteRecord)
│   ├── sep.rs           # sep= directive parsing
│   └── mod.rs
├── checks/
│   ├── schema_overlap.rs
│   ├── key_viability.rs
│   ├── row_granularity.rs
│   ├── type_consistency.rs
│   ├── suite.rs         # CheckSuite, determine_outcome()
│   └── mod.rs
├── scan.rs              # Single-pass scan: scan_file(), NumericClassifier, KeyScan
├── normalize/
│   ├── headers.rs       # Header normalization (ASCII-trim, empty → __shape_col_<N>)
│   └── mod.rs
├── format/
│   ├── ident.rs         # u8:/hex: encoding
│   ├── numbers.rs       # Comma-separated thousands, percentage formatting
│   └── mod.rs
├── output/
│   ├── human.rs         # Human output rendering (all three outcomes)
│   ├── json.rs          # JSON output rendering (serde Serialize structs)
│   └── mod.rs
├── refusal/
│   ├── codes.rs         # RefusalCode enum, as_str(), reason()
│   ├── payload.rs       # RefusalPayload, detail builders per code
│   └── mod.rs
├── lib.rs               # pub fn run() → Result<u8, Box<dyn Error>>
└── main.rs              # Minimal: calls shape::run(), maps to ExitCode
```

### What to reuse from rvl

shape and rvl share significant CSV parsing infrastructure. In v0, this code is duplicated (each tool is an independent binary). Future extraction into a shared crate is a possibility but not a v0 concern.

The following logic should match rvl exactly (read rvl's implementation and replicate):
- Delimiter auto-detection algorithm (`csv/dialect.rs`)
- `sep=` directive parsing (`csv/sep.rs`)
- Header normalization (`normalize/headers.rs`) — change `__rvl_col_` prefix to `__shape_col_`
- BOM handling and encoding guard (`csv/input.rs`)
- Identifier encoding (`format/ident.rs`)
- Delimiter CLI parsing (`cli/delimiter.rs`)
- Exit code mapping and output stream routing (`cli/exit.rs`)
- Refusal envelope JSON shape (`refusal/payload.rs`)

### What is new to shape

- The four structural checks (`checks/`)
- Single-pass row scanning with key tracking + type classification (`scan.rs`)
- The COMPATIBLE/INCOMPATIBLE outcome model (vs rvl's REAL_CHANGE/NO_REAL_CHANGE)
- Numeric **classification** without full numeric **parsing** (shape only needs "is this column numeric?", not the exact value — so `parses_as_numeric()` returns `bool`, not `Option<f64>`)
- The `reasons` array for INCOMPATIBLE outcomes
- Dialect reporting in JSON output (rvl has this too, but shape's JSON schema is different)

### `parses_as_numeric()` — classification only

shape uses the same numeric grammar as rvl (see `rvl/src/numeric/parse.rs`) but only needs a `bool` answer. This is simpler and faster than rvl's `parse_numeric()` which returns `Option<f64>`:

```rust
fn parses_as_numeric(value: &[u8]) -> bool {
    // Same patterns as rvl:
    // - Plain: 123, -123.45, 1e6, 1E-3
    // - Thousands: 1,234  -1,234,567.89
    // - Currency prefix: $123.45, -$1,234.56
    // - Accounting parens: (123.45)
    //
    // Returns true if the value matches any of these patterns.
    // Does NOT need to compute the actual f64 value.
    // Missing tokens are handled by the caller (is_missing check happens first).
}
```

### `is_missing()` — missing value tokens

```rust
fn is_missing(value: &[u8]) -> bool {
    // Expects a pre-trimmed value (caller ASCII-trims before calling).
    // Returns true if value matches any of:
    // empty, "-", "NA", "N/A", "NULL", "NAN", "NONE"
    // Case-insensitive comparison.
}
```

---

## Testing Requirements

### Fixtures

Provide basic test fixtures in `tests/fixtures/`:

- `basic_old.csv` / `basic_new.csv` — simple compatible pair (same schema, unique keys)
- `schema_drift_old.csv` / `schema_drift_new.csv` — old/new with different columns
- `type_shift_old.csv` / `type_shift_new.csv` — a column changes from numeric to non-numeric
- `dup_key_old.csv` / `dup_key_new.csv` — old file has duplicate key values
- `empty.csv` — header only, no data rows (pair with any valid file to trigger E_EMPTY)
- `no_header.csv` — completely empty file (triggers E_HEADERS)

### Test categories

- **Schema overlap tests:** matching columns, partial overlap, zero overlap (profile-scoped overlap deferred until profile tool exists)
- **Key viability tests:** unique keys, duplicate keys, empty keys, missing key column
- **Row granularity tests:** same row count, different row count, key overlap
- **Type consistency tests:** consistent types, type shifts, all-missing columns
- **Delimiter tests:** auto-detection, `sep=` directive, `--delimiter` forced
- **Refusal tests:** each refusal code triggered correctly
- **Output tests:** golden file tests for human and JSON output
- **Exit code tests:** 0/1/2 for each outcome

---

## Scope: v0.1 (ship this)

### Must have

- `<OLD> <NEW>` positional args
- `--key <column>` flag
- `--delimiter <delim>` flag (same parsing as rvl)
- `--json` flag
- `--version` flag (prints `shape <semver>`)
- COMPATIBLE / INCOMPATIBLE outcome with four checks
- Exit codes 0/1/2
- Refusal system with `E_IO`, `E_ENCODING`, `E_CSV_PARSE`, `E_EMPTY`, `E_HEADERS`, `E_DIALECT`
- Human and JSON output
- `operator.json` + `--describe`

### Can defer

- `--profile` / `--profile-id` (needs profile tool)
- `--lock` input verification (needs lock tool)
- `--max-rows` / `--max-bytes` guardrails
- `--schema` flag (JSON Schema output)
- `--progress` flag (not needed — shape is fast)

---

## Open Questions

*None currently blocking. Build it.*
