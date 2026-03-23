#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "Usage: $0 <USB_MOUNT_PATH> <CEF_ROOT> [BUILD_DIR]"
  exit 1
fi

USB_PATH="$1"
CEF_ROOT="$2"
BUILD_DIR="${3:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/build/cef_host_bridge}"
BIN_PATH="${BUILD_DIR}/cef_host_bridge"

if [[ ! -d "${USB_PATH}" ]]; then
  echo "ERROR: USB path not found: ${USB_PATH}"
  exit 2
fi
if [[ ! -d "${CEF_ROOT}" ]]; then
  echo "ERROR: CEF_ROOT not found: ${CEF_ROOT}"
  exit 3
fi
if [[ ! -x "${BIN_PATH}" ]]; then
  echo "ERROR: binary not found: ${BIN_PATH}"
  exit 4
fi

DEST="${USB_PATH%/}/REDUXOS_CEF"
mkdir -p "${DEST}"
mkdir -p "${DEST}/Release"
mkdir -p "${DEST}/Resources"

echo "Copy binary..."
cp "${BIN_PATH}" "${DEST}/"

if [[ -d "${CEF_ROOT}/Release" ]]; then
  echo "Copy Release runtime..."
  cp -R "${CEF_ROOT}/Release/." "${DEST}/Release/"
fi

if [[ -d "${CEF_ROOT}/Resources" ]]; then
  echo "Copy Resources..."
  cp -R "${CEF_ROOT}/Resources/." "${DEST}/Resources/"
fi

if [[ -d "${CEF_ROOT}/Release/Chromium Embedded Framework.framework" ]]; then
  echo "Copy macOS framework..."
  cp -R "${CEF_ROOT}/Release/Chromium Embedded Framework.framework" "${DEST}/Release/"
fi

echo "Done. Runtime package: ${DEST}"
