#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_BIN="${1:-${SCRIPT_DIR}/QTWLDMO.BIN}"
SRC_FILE="${SCRIPT_DIR}/main.cpp"

CXX_BIN="${CXX:-c++}"
PKG_CONFIG_BIN="${PKG_CONFIG:-pkg-config}"
READELF_BIN="${READELF:-readelf}"

if ! command -v "${CXX_BIN}" >/dev/null 2>&1; then
  echo "build error: compilador C++ no encontrado (${CXX_BIN})." >&2
  exit 1
fi
if ! command -v "${PKG_CONFIG_BIN}" >/dev/null 2>&1; then
  echo "build error: pkg-config no encontrado (${PKG_CONFIG_BIN})." >&2
  exit 1
fi
if ! "${PKG_CONFIG_BIN}" --exists Qt5Widgets; then
  echo "build error: faltan headers/libs Qt5Widgets." >&2
  echo "tip: sudo apt install build-essential pkg-config qtbase5-dev qtwayland5" >&2
  exit 1
fi

QT_CFLAGS="$(${PKG_CONFIG_BIN} --cflags Qt5Widgets)"
QT_LIBS="$(${PKG_CONFIG_BIN} --libs Qt5Widgets)"

"${CXX_BIN}" \
  -std=c++17 -O2 -pipe -fPIC -fPIE \
  ${QT_CFLAGS} \
  "${SRC_FILE}" \
  -o "${OUT_BIN}" \
  ${QT_LIBS} \
  -pie \
  -Wl,--dynamic-linker=/lib64/ld-linux-x86-64.so.2 \
  -Wl,-z,relro,-z,now

echo "Built Linux ELF (Qt Wayland): ${OUT_BIN}"
if command -v "${READELF_BIN}" >/dev/null 2>&1; then
  echo "---- ELF type ----"
  "${READELF_BIN}" -h "${OUT_BIN}" | grep -E "Type:|Machine:" || true
  echo "---- PT_INTERP ----"
  "${READELF_BIN}" -l "${OUT_BIN}" | grep -A1 INTERP || true
fi
