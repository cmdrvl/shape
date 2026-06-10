# shape Agent Playbook

## First Calls

```bash
shape --robot-triage
shape capabilities --json
shape robot-docs guide
shape --describe
```

## Compare Gate

```bash
shape old.csv new.csv --key id --json --no-witness
```

Use exit code `0` for compatible, `1` for incompatible, and `2` for refusal or process-level errors. In `--json` compare mode, domain outcomes stay on stdout.

## Repair Mode

`shape doctor --fix` is intentionally unavailable. It exits `2`, leaves stdout empty, and writes stderr suggestions for `shape --robot-triage`, `shape capabilities --json`, and `shape robot-docs guide`.

