#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

HOST="${HOST:-127.0.0.1}"
PORT="${PORT:-37999}"
PROJECT="${PROJECT:-my_app}"
WORKSPACE="${WORKSPACE:-${ROOT}/build/ide_workspace}"

exec ruby "${ROOT}/tools/redux_ide.rb" \
  --host "${HOST}" \
  --port "${PORT}" \
  --project "${PROJECT}" \
  --workspace "${WORKSPACE}"
