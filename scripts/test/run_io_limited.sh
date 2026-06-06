#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  exec "${ROOT_DIR}/scripts/system/run_io_limited.sh" --help
fi

if [[ "${1:-}" == "--" ]]; then
  shift
fi

exec "${ROOT_DIR}/scripts/system/run_io_limited.sh" -- "$@"
