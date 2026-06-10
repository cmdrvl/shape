#!/usr/bin/env bash
set -euo pipefail

TOOL="${TOOL:-target/debug/shape}"

health="$("$TOOL" --json doctor health)"
printf '%s\n' "$health" | jq -e '
  .schema_version == "shape.doctor.v1" and
  .summary.status == "healthy" and
  .capabilities.agent_surfaces.capabilities.command == "shape capabilities --json"
' >/dev/null

