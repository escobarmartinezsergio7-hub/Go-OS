#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRIDGE_DIR="${ROOT_DIR}/tools/servo_host_bridge"

BIND_ADDR="${1:-0.0.0.0:37810}"
START_URL="${2:-https://example.com}"
WINDOW_SIZE="${SERVO_WINDOW_SIZE:-1280x720}"
SERVO_BIN_DEFAULT="/Users/mac/Desktop/servo/target/release/servo"
SERVO_BIN="${SERVO_BIN:-}"

if [[ -z "${SERVO_BIN}" ]]; then
  if [[ -x "${SERVO_BIN_DEFAULT}" ]]; then
    SERVO_BIN="${SERVO_BIN_DEFAULT}"
  else
    SERVO_BIN="servo"
  fi
fi

echo "Starting Servo host bridge..."
echo "  bind: ${BIND_ADDR}"
echo "  url : ${START_URL}"
echo "  size: ${WINDOW_SIZE}"
echo "  servo-bin: ${SERVO_BIN}"

cargo run --manifest-path "${BRIDGE_DIR}/Cargo.toml" -- \
  --bind "${BIND_ADDR}" \
  --url "${START_URL}" \
  --servo-bin "${SERVO_BIN}" \
  --size "${WINDOW_SIZE}"
