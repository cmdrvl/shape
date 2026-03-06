# shape

<div align="center">

[![CI](https://github.com/cmdrvl/shape/actions/workflows/ci.yml/badge.svg)](https://github.com/cmdrvl/shape/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub release](https://img.shields.io/github/v/release/cmdrvl/shape)](https://github.com/cmdrvl/shape/releases)

**Can these two files even be compared? Find out before you waste two hours in Excel.**

```bash
brew install cmdrvl/tap/shape
```

</div>

---

Two CSV exports land on your desk — November and December. Before you open a single cell, ask the question that saves you the next two hours: *can these files even be compared?* Do the columns match? Is the key unique in both? Did the vendor silently change a column from numeric to text?

**shape answers in one command: COMPATIBLE or INCOMPATIBLE.** Four independent structural checks — schema overlap, key viability, row granularity, and type consistency — all at once, with concrete reasons when something breaks. Run shape before `rvl` and you'll never waste time analyzing files that can't be meaningfully compared.

### What makes this different

- **One verdict, four checks** — schema overlap, key uniqueness, row counts, and type consistency all evaluated in a single invocation. All must pass for COMPATIBLE.
- **Concrete reasons** — INCOMPATIBLE isn't a dead end. The `reasons` array tells you exactly what broke: "balance changed from numeric to non-numeric" or "key loan_id has duplicates in new file."
- **Pairs with rvl** — `shape` validates structure, [`rvl`](https://github.com/cmdrvl/rvl) explains numeric changes. Gate your pipeline: `shape old.csv new.csv --key id && rvl old.csv new.csv --key id`.
- **Repro capsules** — `--capsule-dir` writes the inputs, output, and a replay script so any result can be reproduced exactly.

---

## Quick Example

```bash
$ shape nov.csv dec.csv --key loan_id
```

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

All four checks pass. These files are structurally compatible — safe to proceed with `rvl`, `compare`, or `verify`.

```bash
# Gate a pipeline (shape before rvl):
$ shape nov.csv dec.csv --key loan_id --json > shape.json \
    && rvl nov.csv dec.csv --key loan_id --json > rvl.json

# Exit code only (for scripts):
$ shape old.csv new.csv > /dev/null 2>&1
$ echo $?  # 0 = compatible, 1 = incompatible, 2 = refused

# Machine-readable:
$ shape old.csv new.csv --json | jq '.checks.schema_overlap'
```

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

## How shape Compares

| Capability | shape | Manual inspection | csvkit | pandas profiling |
|------------|-------|-------------------|--------|-----------------|
| Schema overlap check | ✅ Automated | ❌ Eyeball headers | ⚠️ `csvstat` per-file | ⚠️ You write it |
| Key uniqueness validation | ✅ Both files | ❌ Manual | ⚠️ Separate step | ⚠️ You write it |
| Type shift detection | ✅ Cross-file | ❌ | ❌ | ⚠️ Per-file only |
| Single deterministic verdict | ✅ | ❌ | ❌ | ❌ |
| Machine-readable output | ✅ `--json` | ❌ | ⚠️ Text | ✅ |
| Audit trail (witness ledger) | ✅ Built-in | ❌ | ❌ | ❌ |
| Setup time | ✅ `brew install` | N/A | ⚠️ pip install | ⚠️ pip install + script |

**When to use shape:**
- Before running `rvl` — validate structure first, then explain numeric changes
- Monthly reconciliation pipelines — catch schema drift before it corrupts results
- CI gate — fail fast if upstream changed the export format

**When shape might not be ideal:**
- You need content comparison (use `rvl` for that)
- You need data profiling (distributions, outliers) — use pandas or Great Expectations
- You're comparing non-CSV formats

---

## Installation

### Homebrew (Recommended)

```bash
brew install cmdrvl/tap/shape
```

### Shell Script

```bash
curl -fsSL https://raw.githubusercontent.com/cmdrvl/shape/main/scripts/install.sh | bash
```

### Windows (PowerShell)

```powershell
Set-ExecutionPolicy -ExecutionPolicy Bypass -Scope Process -Force; iex ((New-Object System.Net.WebClient).DownloadString('https://raw.githubusercontent.com/cmdrvl/shape/main/scripts/install.ps1'))
```

### From Source

```bash
cargo build --release
./target/release/shape --help
```

Prebuilt binaries are available for x86_64 and ARM64 on Linux, macOS, and Windows (x86_64). Each release includes SHA256 checksums, cosign signatures, and an SBOM.

---

## CLI Reference

```
shape <old.csv> <new.csv> [OPTIONS]
```

### Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--key <column>` | string | *(none)* | Key column to check for alignment viability (uniqueness, coverage). |
| `--delimiter <delim>` | string | *(auto-detect)* | Force CSV delimiter for both files. See [Delimiter](#delimiter). |
| `--json` | flag | `false` | Emit a single JSON object on stdout instead of human-readable output. |
| `--no-witness` | flag | `false` | Suppress ambient witness ledger recording for this compare run. |
| `--capsule-dir <path>` | path | *(none)* | Write deterministic repro capsule artifacts (`manifest.json`, copied inputs, rendered output, and `profile.yaml` when a profile is active) to this directory. |
| `--describe` | flag | `false` | Print the compiled-in `operator.json` to stdout and exit `0` without positional args. |

<details>
<summary><strong>Reserved v0 flags</strong> (parsed for schema stability, not yet enforced at runtime)</summary>

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--profile <path>` | path | *(none)* | Profile for check scoping. |
| `--profile-id <id>` | string | *(none)* | Echoed as `profile_id` in JSON output. |
| `--lock <lockfile>` | path | *(none)* | Lock verification for inputs. |
| `--max-rows <n>` | integer | *(unlimited)* | Row-limit refusal. |
| `--max-bytes <n>` | integer | *(unlimited)* | Byte-limit refusal. |

</details>

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

## Repro Capsules

Use `--capsule-dir` to emit deterministic replay artifacts for a run without changing standard output behavior.

```bash
shape old.csv new.csv --key loan_id --json --no-witness --capsule-dir capsules/run-001
```

Generated layout:

```text
capsules/run-001/
  manifest.json
  inputs/old.csv
  inputs/new.csv
  outputs/report.txt
  profile.yaml   # when a profile is active
```

Replay from the capsule directory:

```bash
cd capsules/run-001
shape inputs/old.csv inputs/new.csv --key loan_id --json --no-witness
```

`manifest.json` also stores replay args and a shell command under `replay.argv` and `replay.shell`. When `--profile` or `--profile-id` is active, replay uses the capsule-local `profile.yaml` artifact so the handoff stays self-contained.

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

## Agent / CI Integration

For the full toolchain guide, see the [Agent Operator Guide](https://github.com/cmdrvl/.github/blob/main/profile/AGENT_PROMPT.md).

Both `shape` and `rvl` are designed to be consumed by agents and pipelines, not just humans.

### Self-describing contract

An agent can learn how to invoke `shape` without reading docs:

```bash
$ shape --describe | jq '.exit_codes'
{
  "0": { "meaning": "COMPATIBLE", "domain": "positive" },
  "1": { "meaning": "INCOMPATIBLE", "domain": "negative" },
  "2": { "meaning": "REFUSAL / CLI error", "domain": "error" }
}

$ shape --describe | jq '.pipeline'
{
  "upstream": [],
  "downstream": ["rvl", "compare", "verify", "assess"]
}
```

### Agent workflow: shape → rvl

```bash
# 1. Structural gate
shape old.csv new.csv --key id --json > shape.json
if [ $? -ne 0 ]; then
  # INCOMPATIBLE or REFUSAL — read .reasons or .refusal for why
  cat shape.json | jq '.reasons // .refusal'
  exit 1
fi

# 2. Numeric explanation (only if structurally compatible)
rvl old.csv new.csv --key id --json > rvl.json

# 3. Agent extracts the verdict
outcome=$(jq -r '.outcome' rvl.json)
if [ "$outcome" = "REAL_CHANGE" ]; then
  jq '.contributors[] | "\(.row_id).\(.column): \(.delta)"' rvl.json
fi
```

Everything an agent needs is in `--json` output: structured verdicts, exit codes for branching, and `--describe` for tool discovery.

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

<details>
<summary><strong>Reserved refusal codes</strong> (defined for schema stability, not emitted in v0)</summary>

| Code | Meaning | Next Step |
|------|---------|-----------|
| `E_INPUT_NOT_LOCKED` | Input not in any provided lockfile | Re-run with correct `--lock` or lock inputs first |
| `E_INPUT_DRIFT` | Input hash doesn't match locked member | Use the locked file; regenerate lock if expected |
| `E_TOO_LARGE` | Input exceeds `--max-rows` or `--max-bytes` | Increase limit or split input |

</details>

---

## Troubleshooting

### "E_EMPTY" — one or both files empty

Your file has a header row but no data rows. Check that the export actually produced data:

```bash
wc -l old.csv new.csv
```

### "E_DIALECT" — delimiter detection failed

Your file uses an uncommon delimiter or has inconsistent field counts. Force the delimiter:

```bash
shape old.csv new.csv --delimiter pipe      # for |
shape old.csv new.csv --delimiter 0x09      # for tab
shape old.csv new.csv --delimiter semicolon # for ;
```

### "E_HEADERS" — duplicate column names

Two or more columns share the same header name. Fix at the source, or rename duplicates before running shape.

### Key viability fails but the column looks unique

Check for trailing whitespace, invisible characters, or encoding issues in key values. shape trims ASCII whitespace, but non-ASCII whitespace (e.g., NBSP) is preserved.

### INCOMPATIBLE due to type shift — but the column looks numeric

A cell in the new file has a value that can't be parsed as a number (e.g., `#REF!`, a stray string, or locale-specific formatting). The `type_shifts` field in JSON shows exactly which columns changed.

---

## Limitations

| Limitation | Detail |
|------------|--------|
| **Structural only** | shape checks whether comparison is *possible*, not what changed. Use `rvl` for content diffs. |
| **Two files only** | No multi-file or directory comparison. |
| **In-memory** | Both files are loaded fully into memory. No streaming mode yet. |
| **No column filtering** | All common columns are checked. You can't exclude specific columns in v0. |
| **No content sampling** | shape doesn't look at data distributions or outliers — it checks structure only. |
| **Lock/size gates deferred** | `--lock`, `--max-rows`, and `--max-bytes` are parsed but do not gate runtime behavior in v0. `--profile` and `--profile-id` do affect check scoping. |

---

## FAQ

### Why "shape"?

It checks the *shape* of your data — schema, keys, row counts, types — before you compare content. If the shapes don't match, comparison is meaningless.

### How does shape relate to rvl?

`shape` validates structure. `rvl` explains numeric changes. Run `shape` first to confirm the files are comparable, then `rvl` to see what actually changed. They share delimiter detection and refusal patterns.

### What's the witness ledger?

Every `shape` comparison is appended to a local JSONL file (`~/.epistemic/witness.jsonl`, or `$EPISTEMIC_WITNESS`). This gives you an audit trail of every structural check. Suppress with `--no-witness`.

### Can I query past comparisons?

Yes, using witness subcommands. See [Witness Subcommands](#witness-subcommands) below.

### Can I use this in CI/CD?

Yes. Exit codes (0/1/2) and `--json` output are designed for automation. Gate on exit code, or parse the JSON for richer assertions.

### What about non-CSV formats (Parquet, Excel)?

Not supported. Convert to CSV first.

---

<details>
<summary><strong>Witness Subcommands</strong></summary>

`shape` records every comparison to an ambient witness ledger. You can query this ledger:

```bash
# Query by tool, date range, or outcome
shape witness query --tool shape --since 2026-01-01 --outcome COMPATIBLE --json

# Get the most recent comparison
shape witness last --json

# Count comparisons matching a filter
shape witness count --since 2026-02-01
```

### Subcommand Reference

```bash
shape witness query [--tool <name>] [--since <iso8601>] [--until <iso8601>] \
  [--outcome <COMPATIBLE|INCOMPATIBLE|REFUSAL>] [--input-hash <substring>] \
  [--limit <n>] [--json]

shape witness last [--json]

shape witness count [--tool <name>] [--since <iso8601>] [--until <iso8601>] \
  [--outcome <COMPATIBLE|INCOMPATIBLE|REFUSAL>] [--input-hash <substring>] [--json]
```

### Exit Codes (witness subcommands)

| Code | Meaning |
|------|---------|
| `0` | One or more matching records returned |
| `1` | No matches (or empty ledger for `last`) |
| `2` | CLI parse error or witness internal error |

### Ledger Location

- Default: `~/.epistemic/witness.jsonl`
- Override: set `EPISTEMIC_WITNESS` environment variable
- Malformed ledger lines are skipped; valid lines continue to be processed.

</details>

<details>
<summary><strong>JSON Output Reference</strong></summary>

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

</details>

<details>
<summary><strong>NTM Auto-Proceed (for multi-agent sessions)</strong></summary>

If you run multi-agent sessions and want periodic `proceed` nudges:

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

</details>

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
