#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
STATE_DIR="${STATE_DIR:-${PROJECT_ROOT}/.ntm}"
PID_FILE="${STATE_DIR}/proceed-nudge.pid"
CFG_FILE="${STATE_DIR}/proceed-nudge.env"
LOG_FILE="${STATE_DIR}/proceed-nudge.log"
DEFAULT_MSG_FILE="${STATE_DIR}/proceed-nudge-message.txt"

SESSION=""
AGENT_TYPE="codex"
INTERVAL="10m"
MODE="overnight"
OVERNIGHT_START=20
OVERNIGHT_END=8
BR_WORKDIR="${PROJECT_ROOT}"
MSG_FILE="${DEFAULT_MSG_FILE}"
DELAY_MS=250

usage() {
  cat <<'EOF'
Usage:
  scripts/ntm_proceed_ctl.sh start --session <name> [options]
  scripts/ntm_proceed_ctl.sh stop
  scripts/ntm_proceed_ctl.sh status
  scripts/ntm_proceed_ctl.sh once

Options:
  --session <name>          NTM session name (required for first start)
  --type <agent_type>       Agent type for ntm --type (default: codex)
  --interval <duration>     Send interval: <n>s|<n>m|<n>h|<n> (seconds) (default: 10m)
  --mode <overnight|always> Send mode (default: overnight)
  --overnight-start <hour>  Overnight window start hour, 0-23 (default: 20)
  --overnight-end <hour>    Overnight window end hour, 0-23 (default: 8)
  --workdir <path>          Path where br commands run (default: repo root)
  --message-file <path>     Nudge message file (default: .ntm/proceed-nudge-message.txt)
  --delay-ms <n>            Delay between pane sends (default: 250)

Behavior:
  - This loop is OFF by default.
  - In overnight mode, sends only during the configured overnight window.
  - Sends only when there are open or in-progress beads.
EOF
}

log_line() {
  local level="$1"
  local msg="$2"
  local ts
  ts="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  mkdir -p "${STATE_DIR}"
  printf '[%s] [%s] %s\n' "${ts}" "${level}" "${msg}" >> "${LOG_FILE}"
}

fatal() {
  echo "error: $*" >&2
  exit 1
}

ensure_state() {
  mkdir -p "${STATE_DIR}"
  if [[ ! -f "${DEFAULT_MSG_FILE}" ]]; then
    cat > "${DEFAULT_MSG_FILE}" <<'EOF'
Proceed nudge:
- Keep moving on your current bead.
- If blocked or idle, run `br ready`, pick an unblocked bead, mark it `in_progress`, and continue.
- Reserve only specific files you are actively editing (never `/**`).
EOF
  fi
}

is_hour() {
  local hour="$1"
  [[ "${hour}" =~ ^[0-9]+$ ]] && (( hour >= 0 && hour <= 23 ))
}

is_mode() {
  local mode="$1"
  [[ "${mode}" == "overnight" || "${mode}" == "always" ]]
}

parse_interval_seconds() {
  local raw="$1"
  if [[ "${raw}" =~ ^([0-9]+)([smh]?)$ ]]; then
    local value="${BASH_REMATCH[1]}"
    local unit="${BASH_REMATCH[2]}"
    case "${unit}" in
      ""|"s") echo "${value}" ;;
      "m") echo $(( value * 60 )) ;;
      "h") echo $(( value * 3600 )) ;;
      *) return 1 ;;
    esac
    return 0
  fi
  return 1
}

is_running() {
  if [[ ! -f "${PID_FILE}" ]]; then
    return 1
  fi
  local pid
  pid="$(cat "${PID_FILE}" 2>/dev/null || true)"
  [[ -n "${pid}" ]] || return 1
  kill -0 "${pid}" 2>/dev/null
}

write_config() {
  local cfg_tmp
  cfg_tmp="${CFG_FILE}.tmp"
  {
    printf 'SESSION=%q\n' "${SESSION}"
    printf 'AGENT_TYPE=%q\n' "${AGENT_TYPE}"
    printf 'INTERVAL=%q\n' "${INTERVAL}"
    printf 'MODE=%q\n' "${MODE}"
    printf 'OVERNIGHT_START=%q\n' "${OVERNIGHT_START}"
    printf 'OVERNIGHT_END=%q\n' "${OVERNIGHT_END}"
    printf 'BR_WORKDIR=%q\n' "${BR_WORKDIR}"
    printf 'MSG_FILE=%q\n' "${MSG_FILE}"
    printf 'DELAY_MS=%q\n' "${DELAY_MS}"
  } > "${cfg_tmp}"
  mv "${cfg_tmp}" "${CFG_FILE}"
}

load_config() {
  [[ -f "${CFG_FILE}" ]] || return 1
  # shellcheck disable=SC1090
  source "${CFG_FILE}"
}

parse_common_flags() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --session)
        SESSION="$2"
        shift 2
        ;;
      --type)
        AGENT_TYPE="$2"
        shift 2
        ;;
      --interval)
        INTERVAL="$2"
        shift 2
        ;;
      --mode)
        MODE="$2"
        shift 2
        ;;
      --overnight-start)
        OVERNIGHT_START="$2"
        shift 2
        ;;
      --overnight-end)
        OVERNIGHT_END="$2"
        shift 2
        ;;
      --workdir)
        BR_WORKDIR="$2"
        shift 2
        ;;
      --message-file)
        MSG_FILE="$2"
        shift 2
        ;;
      --delay-ms)
        DELAY_MS="$2"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        fatal "unknown option: $1"
        ;;
    esac
  done
}

validate_config() {
  [[ -n "${SESSION}" ]] || fatal "--session is required"
  [[ -n "${AGENT_TYPE}" ]] || fatal "--type cannot be empty"
  [[ -n "${BR_WORKDIR}" ]] || fatal "--workdir cannot be empty"
  [[ -d "${BR_WORKDIR}" ]] || fatal "--workdir does not exist: ${BR_WORKDIR}"
  [[ -f "${MSG_FILE}" ]] || fatal "--message-file does not exist: ${MSG_FILE}"
  is_mode "${MODE}" || fatal "--mode must be 'overnight' or 'always'"
  is_hour "${OVERNIGHT_START}" || fatal "--overnight-start must be 0-23"
  is_hour "${OVERNIGHT_END}" || fatal "--overnight-end must be 0-23"
  parse_interval_seconds "${INTERVAL}" >/dev/null || fatal "--interval must be <n>s|<n>m|<n>h|<n>"
  [[ "${DELAY_MS}" =~ ^[0-9]+$ ]] || fatal "--delay-ms must be an integer"
}

in_overnight_window() {
  local hour
  hour="$(date +%H)"
  hour="${hour#0}"
  [[ -n "${hour}" ]] || hour=0

  if (( OVERNIGHT_START < OVERNIGHT_END )); then
    (( hour >= OVERNIGHT_START && hour < OVERNIGHT_END ))
  else
    (( hour >= OVERNIGHT_START || hour < OVERNIGHT_END ))
  fi
}

active_bead_count() {
  local count
  if ! count="$(cd "${BR_WORKDIR}" && br count --status open --status in_progress 2>/dev/null)"; then
    echo "0"
    return
  fi

  if [[ "${count}" =~ ^[0-9]+$ ]]; then
    echo "${count}"
  else
    echo "0"
  fi
}

send_nudge_once() {
  if [[ "${MODE}" == "overnight" ]] && ! in_overnight_window; then
    log_line "SKIP" "outside overnight window (${OVERNIGHT_START}-${OVERNIGHT_END})"
    return 0
  fi

  local bead_count
  bead_count="$(active_bead_count)"
  if [[ "${bead_count}" == "0" ]]; then
    log_line "SKIP" "no open or in_progress beads"
    return 0
  fi

  log_line "SEND" "session=${SESSION} type=${AGENT_TYPE} beads=${bead_count}"
  if ! ntm --robot-send="${SESSION}" --type="${AGENT_TYPE}" --msg-file="${MSG_FILE}" --delay-ms="${DELAY_MS}" --json >> "${LOG_FILE}" 2>&1; then
    log_line "WARN" "ntm --robot-send failed"
  fi
}

run_loop() {
  ensure_state
  load_config || fatal "missing config: ${CFG_FILE}"
  validate_config

  local interval_seconds
  interval_seconds="$(parse_interval_seconds "${INTERVAL}")"
  log_line "INFO" "loop started pid=$$ session=${SESSION} mode=${MODE} interval=${INTERVAL}"

  trap 'log_line "INFO" "loop exiting pid=$$"; exit 0' INT TERM

  while true; do
    send_nudge_once
    sleep "${interval_seconds}"
  done
}

cmd_start() {
  ensure_state
  if load_config; then
    true
  fi

  parse_common_flags "$@"
  validate_config
  write_config

  if is_running; then
    local current_pid
    current_pid="$(cat "${PID_FILE}")"
    echo "already running (pid ${current_pid})"
    exit 0
  fi

  nohup "$0" run-loop >/dev/null 2>&1 &
  local new_pid=$!
  echo "${new_pid}" > "${PID_FILE}"
  sleep 1
  if kill -0 "${new_pid}" 2>/dev/null; then
    echo "started proceed loop (pid ${new_pid})"
  else
    rm -f "${PID_FILE}"
    fatal "failed to start proceed loop"
  fi
}

cmd_stop() {
  ensure_state
  if ! is_running; then
    rm -f "${PID_FILE}"
    echo "already stopped"
    exit 0
  fi

  local pid
  pid="$(cat "${PID_FILE}")"
  kill "${pid}" 2>/dev/null || true
  sleep 1
  if kill -0 "${pid}" 2>/dev/null; then
    kill -9 "${pid}" 2>/dev/null || true
  fi
  rm -f "${PID_FILE}"
  log_line "INFO" "loop stopped pid=${pid}"
  echo "stopped proceed loop"
}

cmd_status() {
  ensure_state
  if load_config; then
    true
  fi

  local running="no"
  local pid="-"
  if is_running; then
    running="yes"
    pid="$(cat "${PID_FILE}")"
  fi

  local beads
  beads="$(active_bead_count)"

  echo "running: ${running}"
  echo "pid: ${pid}"
  echo "session: ${SESSION:-unset}"
  echo "type: ${AGENT_TYPE:-unset}"
  echo "interval: ${INTERVAL:-unset}"
  echo "mode: ${MODE:-unset}"
  echo "overnight_window: ${OVERNIGHT_START:-unset}-${OVERNIGHT_END:-unset}"
  echo "workdir: ${BR_WORKDIR:-unset}"
  echo "message_file: ${MSG_FILE:-unset}"
  echo "active_beads(open+in_progress): ${beads}"
  echo "log_file: ${LOG_FILE}"
}

cmd_once() {
  ensure_state
  load_config || true
  parse_common_flags "$@"
  validate_config
  write_config
  send_nudge_once
  echo "single nudge check complete"
}

main() {
  local cmd="${1:-}"
  shift || true

  case "${cmd}" in
    start)
      cmd_start "$@"
      ;;
    stop)
      cmd_stop
      ;;
    status)
      cmd_status
      ;;
    once)
      cmd_once "$@"
      ;;
    run-loop)
      run_loop
      ;;
    -h|--help|"")
      usage
      ;;
    *)
      fatal "unknown command: ${cmd}"
      ;;
  esac
}

main "$@"
