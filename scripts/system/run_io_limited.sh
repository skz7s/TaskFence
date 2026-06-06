#!/usr/bin/env bash

set -euo pipefail

SCRIPT_NAME="$(basename "$0")"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

IO_SCOPE_MODE="${CODEX_HELPER_IO_SCOPE_MODE:-auto}"
IO_DEVICE="${CODEX_HELPER_IO_DEVICE:-auto}"
IO_READ_BANDWIDTH_MAX="${CODEX_HELPER_IO_READ_BANDWIDTH_MAX:-50M}"
IO_WRITE_BANDWIDTH_MAX="${CODEX_HELPER_IO_WRITE_BANDWIDTH_MAX:-10M}"
IO_WEIGHT="${CODEX_HELPER_IO_WEIGHT:-50}"
IO_UNIT_PREFIX="${CODEX_HELPER_IO_UNIT_PREFIX:-codex-helper-io}"
IO_ALLOW_FALLBACK="${CODEX_HELPER_IO_ALLOW_IONICE_FALLBACK:-0}"
ORIGINAL_UID="${CODEX_HELPER_IO_ORIGINAL_UID:-${SUDO_UID:-}}"
ORIGINAL_GID="${CODEX_HELPER_IO_ORIGINAL_GID:-${SUDO_GID:-}}"
ORIGINAL_USER="${CODEX_HELPER_IO_ORIGINAL_USER:-${SUDO_USER:-}}"
ORIGINAL_HOME="${CODEX_HELPER_IO_ORIGINAL_HOME:-}"
ORIGINAL_PATH="${CODEX_HELPER_IO_ORIGINAL_PATH:-${PATH}}"

usage() {
  cat <<'EOF'
Usage: scripts/system/run_io_limited.sh [--] command [args...]

Run a command inside a systemd scope with I/O controls. This wrapper uses a
system-level transient scope by default because user-level scopes may not have
the cgroup v2 io controller enabled.

Environment:
  CODEX_HELPER_IO_SCOPE_MODE              auto | system | user | off (default: auto)
  CODEX_HELPER_IO_DEVICE                  auto | off | /dev/... (default: auto)
  CODEX_HELPER_IO_READ_BANDWIDTH_MAX      systemd byte rate (default: 50M)
  CODEX_HELPER_IO_WRITE_BANDWIDTH_MAX     systemd byte rate (default: 10M)
  CODEX_HELPER_IO_WEIGHT                  1-10000 (default: 50)
  CODEX_HELPER_IO_ALLOW_IONICE_FALLBACK   1 to fall back to ionice/nice
EOF
}

die() {
  printf '[%s] error: %s\n' "${SCRIPT_NAME}" "$*" >&2
  exit 1
}

warn() {
  printf '[%s] warning: %s\n' "${SCRIPT_NAME}" "$*" >&2
}

validate_rate() {
  local name="$1"
  local value="$2"

  [[ "${value}" =~ ^[0-9]+([KMGTP]?)$ ]] || die "${name} must be a systemd byte rate like 10M or 500K: ${value}"
}

resolve_io_device() {
  local source=""

  if [[ "${IO_DEVICE}" == "off" || "${IO_DEVICE}" == "none" ]]; then
    printf ''
    return
  fi

  if [[ "${IO_DEVICE}" == "auto" ]]; then
    if command -v findmnt >/dev/null 2>&1; then
      source="$(findmnt -T "${ROOT_DIR}" -no SOURCE 2>/dev/null || true)"
      source="${source%%$'\n'*}"
      source="${source%%[*}"
    fi
    if [[ -n "${source}" && "${source}" == /dev/* && -e "${source}" ]]; then
      printf '%s' "${source}"
      return
    fi
    printf '%s' "${ROOT_DIR}"
    return
  fi

  [[ "${IO_DEVICE}" == /* ]] || die "CODEX_HELPER_IO_DEVICE must be auto, off, none, or an absolute path: ${IO_DEVICE}"
  [[ "${IO_DEVICE}" != *[[:space:]]* ]] || die "CODEX_HELPER_IO_DEVICE with spaces is not supported: ${IO_DEVICE}"
  printf '%s' "${IO_DEVICE}"
}

supports_systemd_user_io_controller() {
  local scope_pid="$1"
  local cgroup_path=""
  local scope_path=""

  cgroup_path="$(sed -n 's/^0:://p' "/proc/${scope_pid}/cgroup" 2>/dev/null | head -n 1)"
  [[ -n "${cgroup_path}" ]] || return 1
  scope_path="/sys/fs/cgroup${cgroup_path}"
  [[ -f "${scope_path}/io.max" && -f "${scope_path}/io.weight" ]]
}

resolve_executable() {
  local cmd="$1"
  local exe=""

  if [[ "${cmd}" == */* ]]; then
    printf '%s' "${cmd}"
    return
  fi
  exe="$(type -P -- "${cmd}" 2>/dev/null || true)"
  [[ -n "${exe}" ]] || die "command not found: ${cmd}"
  printf '%s' "${exe}"
}

run_with_ionice_fallback() {
  warn "falling back to ionice/nice; this is not a hard bandwidth cap"
  if command -v ionice >/dev/null 2>&1; then
    exec ionice -c 3 nice -n 10 "$@"
  fi
  exec nice -n 10 "$@"
}

user_slice_has_io_limits() {
  local uid=""
  local io_max_path=""
  local io_max=""

  uid="${ORIGINAL_UID:-$(id -u)}"
  io_max_path="/sys/fs/cgroup/user.slice/user-${uid}.slice/io.max"
  [[ -r "${io_max_path}" ]] || return 1
  io_max="$(cat "${io_max_path}" 2>/dev/null || true)"
  [[ "${io_max}" =~ (rbps|wbps|riops|wiops)=[0-9]+ ]]
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ "${1:-}" == "--" ]]; then
  shift
fi

[[ "$#" -gt 0 ]] || die "missing command"
COMMAND_EXE="$(resolve_executable "$1")"
shift

case "${IO_SCOPE_MODE}" in
  auto)
    if user_slice_has_io_limits; then
      exec "${COMMAND_EXE}" "$@"
    fi
    IO_SCOPE_MODE="system"
    ;;
  off|none)
    exec "${COMMAND_EXE}" "$@"
    ;;
  system|user)
    ;;
  *)
    die "CODEX_HELPER_IO_SCOPE_MODE must be auto, system, user, or off: ${IO_SCOPE_MODE}"
    ;;
esac

validate_rate "CODEX_HELPER_IO_READ_BANDWIDTH_MAX" "${IO_READ_BANDWIDTH_MAX}"
validate_rate "CODEX_HELPER_IO_WRITE_BANDWIDTH_MAX" "${IO_WRITE_BANDWIDTH_MAX}"
[[ "${IO_WEIGHT}" =~ ^[0-9]+$ ]] || die "CODEX_HELPER_IO_WEIGHT must be numeric: ${IO_WEIGHT}"
((IO_WEIGHT >= 1 && IO_WEIGHT <= 10000)) || die "CODEX_HELPER_IO_WEIGHT must be between 1 and 10000: ${IO_WEIGHT}"

IO_DEVICE_RESOLVED="$(resolve_io_device)"

if [[ "${IO_SCOPE_MODE}" == "user" ]]; then
  if [[ -n "${IO_DEVICE_RESOLVED}" ]]; then
    warn "user scopes may not expose cgroup io.max/io.weight; prefer CODEX_HELPER_IO_SCOPE_MODE=system"
  fi
  systemd_args=(systemd-run --user --scope --collect --quiet --same-dir)
else
  if [[ "$(id -u)" -eq 0 ]]; then
    systemd_args=(systemd-run --scope --collect --quiet --same-dir)
    if [[ -n "${ORIGINAL_UID}" ]]; then
      systemd_args+=(--uid "${ORIGINAL_UID}")
    fi
    if [[ -n "${ORIGINAL_GID}" ]]; then
      systemd_args+=(--gid "${ORIGINAL_GID}")
    fi
  else
    if ! command -v sudo >/dev/null 2>&1; then
      [[ "${IO_ALLOW_FALLBACK}" == "1" ]] && run_with_ionice_fallback "${COMMAND_EXE}" "$@"
      die "system-level I/O scope requires sudo, but sudo is unavailable"
    fi
    if ! sudo -n true >/dev/null 2>&1; then
      [[ "${IO_ALLOW_FALLBACK}" == "1" ]] && run_with_ionice_fallback "${COMMAND_EXE}" "$@"
      die "system-level I/O scope requires sudo. Re-run with sudo access or set CODEX_HELPER_IO_SCOPE_MODE=off."
    fi
    ORIGINAL_USER="$(id -un)"
    ORIGINAL_HOME="${HOME}"
    ORIGINAL_PATH="${PATH}"
    exec sudo \
      CODEX_HELPER_IO_SCOPE_MODE=system \
      CODEX_HELPER_IO_DEVICE="${IO_DEVICE}" \
      CODEX_HELPER_IO_READ_BANDWIDTH_MAX="${IO_READ_BANDWIDTH_MAX}" \
      CODEX_HELPER_IO_WRITE_BANDWIDTH_MAX="${IO_WRITE_BANDWIDTH_MAX}" \
      CODEX_HELPER_IO_WEIGHT="${IO_WEIGHT}" \
      CODEX_HELPER_IO_UNIT_PREFIX="${IO_UNIT_PREFIX}" \
      CODEX_HELPER_IO_ORIGINAL_UID="$(id -u)" \
      CODEX_HELPER_IO_ORIGINAL_GID="$(id -g)" \
      CODEX_HELPER_IO_ORIGINAL_USER="${ORIGINAL_USER}" \
      CODEX_HELPER_IO_ORIGINAL_HOME="${ORIGINAL_HOME}" \
      CODEX_HELPER_IO_ORIGINAL_PATH="${ORIGINAL_PATH}" \
      "${BASH_SOURCE[0]}" -- "${COMMAND_EXE}" "$@"
  fi
fi

unit_name="${IO_UNIT_PREFIX}-$(date +%s)-$$"
systemd_args+=(--unit "${unit_name}")
systemd_args+=(-p IOAccounting=yes -p "IOWeight=${IO_WEIGHT}")
if [[ -n "${ORIGINAL_PATH}" ]]; then
  systemd_args+=(--setenv "PATH=${ORIGINAL_PATH}")
fi
if [[ -n "${ORIGINAL_HOME}" ]]; then
  systemd_args+=(--setenv "HOME=${ORIGINAL_HOME}")
fi
if [[ -n "${ORIGINAL_USER}" ]]; then
  systemd_args+=(--setenv "USER=${ORIGINAL_USER}" --setenv "LOGNAME=${ORIGINAL_USER}")
fi
if [[ -n "${IO_DEVICE_RESOLVED}" ]]; then
  systemd_args+=(
    -p "IOReadBandwidthMax=${IO_DEVICE_RESOLVED} ${IO_READ_BANDWIDTH_MAX}"
    -p "IOWriteBandwidthMax=${IO_DEVICE_RESOLVED} ${IO_WRITE_BANDWIDTH_MAX}"
  )
fi

exec "${systemd_args[@]}" -- "${COMMAND_EXE}" "$@"
