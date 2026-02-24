#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

BIND_ADDR="${1:-0.0.0.0:37810}"
START_URL="${2:-https://www.google.com}"

echo "Starting WebKit host bridge (Wry)..."
echo "  bind: ${BIND_ADDR}"
echo "  url : ${START_URL}"

bash "${ROOT_DIR}/scripts/run_wry_host_bridge.sh" "${BIND_ADDR}" "${START_URL}"
