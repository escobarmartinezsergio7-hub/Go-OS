#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_DIR="${ROOT_DIR}/tools/cef_host_bridge"
BUILD_DIR="${ROOT_DIR}/build/cef_host_bridge"

if [[ -z "${CEF_ROOT:-}" ]]; then
  echo "ERROR: define CEF_ROOT con la ruta de CEF."
  echo "Ejemplo:"
  echo "  export CEF_ROOT=\$HOME/Downloads/cef_binary_xxx"
  exit 1
fi

BIND_ADDR="${1:-0.0.0.0:37810}"
START_URL="${2:-https://www.google.com}"

echo "Configuring CEF host bridge..."
cmake -S "${PROJECT_DIR}" -B "${BUILD_DIR}" -DCEF_ROOT="${CEF_ROOT}"

echo "Building CEF host bridge..."
cmake --build "${BUILD_DIR}" -j

BIN="${BUILD_DIR}/cef_host_bridge"
if [[ ! -x "${BIN}" ]]; then
  echo "ERROR: binary not found: ${BIN}"
  exit 2
fi

echo "Running: ${BIN} --bind ${BIND_ADDR} --url ${START_URL}"
"${BIN}" --bind "${BIND_ADDR}" --url "${START_URL}"
