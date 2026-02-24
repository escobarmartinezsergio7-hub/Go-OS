#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE_DIR="${ROOT_DIR}/tools/wry_host_bridge"

BIND_ADDR="${1:-0.0.0.0:37810}"
START_URL="${2:-https://example.com}"

echo "Starting WebKit host bridge (Wry runtime)..."
echo "  bind: ${BIND_ADDR}"
echo "  url : ${START_URL}"

cargo run --manifest-path "${BRIDGE_DIR}/Cargo.toml" -- --bind "${BIND_ADDR}" --url "${START_URL}"
