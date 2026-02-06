# AGENTS.md — shape

> Guidelines for AI coding agents working in this Rust codebase.

---

## RULE 0 - THE FUNDAMENTAL OVERRIDE PREROGATIVE

If I tell you to do something, even if it goes against what follows below, YOU MUST LISTEN TO ME. I AM IN CHARGE, NOT YOU.

---

## RULE NUMBER 1: NO FILE DELETION

**YOU ARE NEVER ALLOWED TO DELETE A FILE WITHOUT EXPRESS PERMISSION.** Even a new file that you yourself created, such as a test code file. You have a horrible track record of deleting critically important files or otherwise throwing away tons of expensive work. As a result, you have permanently lost any and all rights to determine that a file or folder should be deleted.

**YOU MUST ALWAYS ASK AND RECEIVE CLEAR, WRITTEN PERMISSION BEFORE EVER DELETING A FILE OR FOLDER OF ANY KIND.**

---

## Irreversible Git & Filesystem Actions — DO NOT EVER BREAK GLASS

> **Note:** This project never needs destructive commands during normal development. Treat them as forbidden unless explicitly authorized.

1. **Absolutely forbidden commands:** `git reset --hard`, `git clean -fd`, `rm -rf`, or any command that can delete or overwrite code/data must never be run unless the user explicitly provides the exact command and states, in the same message, that they understand and want the irreversible consequences.
2. **No guessing:** If there is any uncertainty about what a command might delete or overwrite, stop immediately and ask the user for specific approval. "I think it's safe" is never acceptable.
3. **Safer alternatives first:** When cleanup or rollbacks are needed, request permission to use non-destructive options (`git status`, `git diff`, `git stash`, copying to backups) before ever considering a destructive command.
4. **Mandatory explicit plan:** Even after explicit user authorization, restate the command verbatim, list exactly what will be affected, and wait for a confirmation that your understanding is correct. Only then may you execute it—if anything remains ambiguous, refuse and escalate.
5. **Document the confirmation:** When running any approved destructive command, record (in the session notes / final response) the exact user text that authorized it, the command actually run, and the execution time. If that record is absent, the operation did not happen.

---

## Git Branch: ONLY Use `main`, NEVER `master`

**The default branch is `main`. The `master` branch exists only for legacy URL compatibility.**

- **All work happens on `main`** — commits, PRs, feature branches all merge to `main`
- **Never reference `master` in code or docs** — if you see `master` anywhere, it's a bug that needs fixing
- **The `master` branch must stay synchronized with `main`** — after pushing to `main`, also push to `master`:
  ```bash
  git push origin main:master
  ```

---

## Toolchain: Rust & Cargo

We only use **Cargo** in this project, NEVER any other package manager.

- **Edition:** Rust 2024 (if `rust-toolchain.toml` exists, follow it)
- **Dependency versions:** Explicit versions for stability
- **Configuration:** Cargo.toml only
- **Unsafe code:** Forbidden (`#![forbid(unsafe_code)]`)

### Release Profile

The release build optimizes for binary size:

```toml
[profile.release]
opt-level = "z"     # Optimize for size (lean binary for distribution)
lto = true          # Link-time optimization
codegen-units = 1   # Single codegen unit for better optimization
panic = "abort"     # Smaller binary, no unwinding overhead
strip = true        # Remove debug symbols
```

---

## Code Editing Discipline

### No Script-Based Changes

**NEVER** run a script that processes/changes code files in this repo. Brittle regex-based transformations create far more problems than they solve.

- **Always make code changes manually**, even when there are many instances
- For many simple changes: use parallel subagents
- For subtle/complex changes: do them methodically yourself

### No File Proliferation

If you want to change something or add a feature, **revise existing code files in place**.

**NEVER** create variations like:
- `mainV2.rs`
- `main_improved.rs`
- `main_enhanced.rs`

New files are reserved for **genuinely new functionality** that makes zero sense to include in any existing file. The bar for creating new files is **incredibly high**.

---

## Backwards Compatibility

We do not care about backwards compatibility—we're in early development with no users. We want to do things the **RIGHT** way with **NO TECH DEBT**.

- Never create "compatibility shims"
- Never create wrapper functions for deprecated APIs
- Just fix the code directly

---

## Output Style

shape has two output modes:

- **Human (default):** Emit exactly one outcome: `COMPATIBLE`, `INCOMPATIBLE`, or `REFUSAL`.
  - `COMPATIBLE` / `INCOMPATIBLE` go to stdout; `REFUSAL` goes to stderr.
- **`--json`:** Emit exactly one JSON object on stdout for all outcomes; stderr is reserved for process-level failures only.

Follow the exact headers, wording, and schema in `docs/PLAN.md` — no extra banners or ad-hoc text.

---

## Compiler Checks (CRITICAL)

**After any substantive code changes, you MUST verify no errors were introduced:**

```bash
# Check for compiler errors and warnings
cargo check --all-targets

# Check for clippy lints
cargo clippy --all-targets -- -D warnings

# Verify formatting
cargo fmt --check
```

If you see errors, **carefully understand and resolve each issue**. Read sufficient context to fix them the RIGHT way.

---

## Testing

### Unit Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run a specific test by name (when present)
cargo test <test_name>
```

If no tests exist yet, say so explicitly in the final response and skip running them.

---

## CI/CD Pipeline

Keep CI expectations aligned with `.github/workflows` once they exist.

Default local checks (when code is present):

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

---

## Release Process

When fixes are ready for release, follow this process:

### 1. Verify CI Passes Locally

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

### 2. Commit Changes

```bash
git add -A
git commit -m "fix: description of fixes

- List specific fixes
- Include any breaking changes"
```

### 3. Push and Trigger Release

```bash
git push origin main
git push origin main:master  # Keep master in sync
```

---

## shape — This Project

**This is the project you're working on.** shape is the structural comparability gate for the epistemic spine — it deterministically answers "can these two CSV datasets be compared at all?"

### Source of Truth

- `docs/PLAN.md` is the spec for CLI behavior, checks, refusal codes, and output formatting. Follow it verbatim.

### Core Behavior (v0)

- Accept two CSV files as positional args.
- Run four structural checks: schema overlap, key viability, row granularity, type consistency.
- Emit exactly one outcome: `COMPATIBLE`, `INCOMPATIBLE`, or `REFUSAL`.

### Relationship to rvl

shape is the **gate** that runs before rvl. It answers "can these be compared?" while rvl answers "what actually changed?" Both follow the same patterns: same CSV parsing, same delimiter detection, same refusal system, same identifier encoding, same exit code conventions. Code should be structurally similar to rvl where applicable.

### Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI entry (delegates to `lib::run()`) |
| `src/orchestrator.rs` | Top-level pipeline: parse → check → output |
| `src/cli/` | Argument parsing, exit codes, output mode routing |
| `src/csv/` | Parsing, dialect detection (shared patterns with rvl) |
| `src/checks/` | The four structural checks |
| `src/output/` | Human and JSON output formatting |
| `src/refusal/` | Refusal codes and error handling |

### Performance Goals

- Stream CSV headers and scan rows without loading full files into memory when possible.
- Key viability requires a full scan of the key column; schema overlap only needs headers.

---

## CSV Parsing Notes

- Parsing, delimiter detection, and refusal reasons must follow `docs/PLAN.md`.
- Reuse the same CSV parsing conventions as rvl (RFC4180, `sep=` directive, auto-detection, ASCII-trim, `u8:`/`hex:` encoding).
- Never silently reinterpret data; refuse with a concrete next step.

---

## Third-Party Library Usage

If you aren't 100% sure how to use a third-party library, **SEARCH ONLINE** to find the latest documentation and current best practices.

---

## Beads (br) — Dependency-Aware Issue Tracking

Beads provides a lightweight, dependency-aware issue database and CLI (`br` - beads_rust) for selecting "ready work," setting priorities, and tracking status.

**Important:** `br` is non-invasive—it NEVER runs git commands automatically. You must manually commit changes after `br sync --flush-only`.

### Essential Commands

```bash
br ready              # Show issues ready to work (no blockers)
br list --status=open # All open issues
br show <id>          # Full issue details with dependencies
br create --title="..." --type=task --priority=2
br update <id> --status=in_progress
br close <id> --reason "Completed"
br sync --flush-only  # Export to JSONL (NO git operations)
```

---

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   br sync --flush-only    # Export beads to JSONL (no git ops)
   git add .beads/         # Stage beads changes
   git add <other files>   # Stage code changes
   git commit -m "..."     # Commit everything
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Verify** - All changes committed AND pushed

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds

---

## Multi-Agent Coordination Notes

When working alongside other agents:

- **Never stash, revert, or overwrite other agents' work**
- Treat unexpected changes in the working tree as if you made them
- If you see changes you didn't make in `git status`, those are from other agents working concurrently—commit them together with your changes
- This is normal and happens frequently in multi-agent environments

### CRITICAL: Never Ask About Unexpected Changes

**NEVER stop working to ask about unexpected changes in the working tree.** The answer is always the same: those are changes created by other agents working on the project concurrently.

**The rule is simple:** You NEVER, under ANY circumstance, stash, revert, overwrite, or otherwise disturb the work of other agents. Treat those changes identically to changes you made yourself.
