#!/usr/bin/env bash
set -euo pipefail

TOOL="${TOOL:-target/debug/shape}"

set +e
stdout="$("$TOOL" doctor --fix 2>/dev/null)"
status=$?
set -e

if [ "$status" -ne 2 ]; then
  echo "expected doctor --fix to exit 2, got $status" >&2
  exit 1
fi

if [ -n "$stdout" ]; then
  echo "expected doctor --fix to leave stdout empty" >&2
  exit 1
fi

set +e
stderr="$("$TOOL" doctor --fix 2>&1 >/dev/null)"
set -e

grep -F "shape doctor --fix is unavailable" <<<"$stderr" >/dev/null
grep -F "shape --robot-triage" <<<"$stderr" >/dev/null
grep -F "shape capabilities --json" <<<"$stderr" >/dev/null
grep -F "shape robot-docs guide" <<<"$stderr" >/dev/null

