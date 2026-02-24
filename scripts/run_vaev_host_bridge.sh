#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE_SCRIPT="${ROOT_DIR}/tools/vaev_host_bridge/vaev_host_bridge.rb"

BIND_ADDR="${1:-0.0.0.0:37810}"
START_URL="${2:-https://www.google.com}"
VAEV_DIR_ARG="${3:-}"
VAEV_DIR_DEFAULT="/Users/mac/Documents/vaev"

if [[ -n "${VAEV_DIR_ARG}" ]]; then
  VAEV_DIR="${VAEV_DIR_ARG}"
elif [[ -n "${VAEV_DIR:-}" ]]; then
  VAEV_DIR="${VAEV_DIR}"
else
  VAEV_DIR="${VAEV_DIR_DEFAULT}"
fi

echo "Starting Vaev host bridge..."
echo "  bind: ${BIND_ADDR}"
echo "  vaev: ${VAEV_DIR}"
echo "  url : ${START_URL}"

ruby "${BRIDGE_SCRIPT}" --bind "${BIND_ADDR}" --url "${START_URL}" --vaev-dir "${VAEV_DIR}"
