#!/usr/bin/env bash
set -euo pipefail

TOOL="${TOOL:-target/debug/shape}"

triage="$("$TOOL" --robot-triage)"
printf '%s\n' "$triage" | jq -e '
  .schema_version == "shape.doctor.v1" and
  .capabilities_url == "command:shape capabilities --json" and
  .capabilities.agent_surfaces.robot_triage.command == "shape --robot-triage"
' >/dev/null

capabilities="$("$TOOL" capabilities --json)"
printf '%s\n' "$capabilities" | jq -e '
  .schema_version == "shape.doctor.capabilities.v1" and
  .agent_surfaces.capabilities.command == "shape capabilities --json" and
  .agent_surfaces.robot_docs.command == "shape robot-docs guide" and
  .fix_mode.available == false
' >/dev/null

docs="$("$TOOL" robot-docs guide)"
grep -F "shape --robot-triage" <<<"$docs" >/dev/null
grep -F "shape capabilities --json" <<<"$docs" >/dev/null
grep -F "shape robot-docs guide" <<<"$docs" >/dev/null

