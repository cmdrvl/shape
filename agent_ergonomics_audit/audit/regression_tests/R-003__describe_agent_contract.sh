#!/usr/bin/env bash
set -euo pipefail

TOOL="${TOOL:-target/debug/shape}"

describe="$("$TOOL" --describe)"
printf '%s\n' "$describe" | jq -e '
  (.version == "0.7.0") and
  ((.invocation.usage | index("shape --robot-triage")) != null)
' >/dev/null
printf '%s\n' "$describe" | jq -e '
  .capabilities.agent_surfaces.robot_triage == "shape --robot-triage" and
  .capabilities.agent_surfaces.capabilities == "shape capabilities --json" and
  .capabilities.agent_surfaces.robot_docs == "shape robot-docs guide"
' >/dev/null
printf '%s\n' "$describe" | jq -e '
  ([.subcommands[].name] | index("capabilities")) != null and
  ([.subcommands[].name] | index("robot-docs")) != null and
  ([.subcommands[].name] | index("doctor")) != null
' >/dev/null
