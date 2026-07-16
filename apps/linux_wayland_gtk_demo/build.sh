#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_BIN="${1:-${SCRIPT_DIR}/GTKWLDMO.BIN}"
SRC_FILE="${SCRIPT_DIR}/main.c"

CC_BIN="${CC:-cc}"
PKG_CONFIG_BIN="${PKG_CONFIG:-pkg-config}"
READELF_BIN="${READELF:-readelf}"

if ! command -v "${CC_BIN}" >/dev/null 2>&1; then
  echo "build error: compilador no encontrado (${CC_BIN})." >&2
  exit 1
fi
if ! command -v "${PKG_CONFIG_BIN}" >/dev/null 2>&1; then
  echo "build error: pkg-config no encontrado (${PKG_CONFIG_BIN})." >&2
  exit 1
fi
if ! "${PKG_CONFIG_BIN}" --exists gtk+-3.0; then
  echo "build error: faltan headers/libs gtk+-3.0." >&2
  echo "tip: sudo apt install build-essential pkg-config libgtk-3-dev" >&2
  exit 1
fi

GTK_CFLAGS="$(${PKG_CONFIG_BIN} --cflags gtk+-3.0)"
GTK_LIBS="$(${PKG_CONFIG_BIN} --libs gtk+-3.0)"

"${CC_BIN}" \
  -O2 -pipe -fPIE -D_GNU_SOURCE \
  ${GTK_CFLAGS} \
  "${SRC_FILE}" \
  -o "${OUT_BIN}" \
  ${GTK_LIBS} \
  -pie \
  -Wl,--dynamic-linker=/lib64/ld-linux-x86-64.so.2 \
  -Wl,-z,relro,-z,now

echo "Built Linux ELF (GTK Wayland): ${OUT_BIN}"
if command -v "${READELF_BIN}" >/dev/null 2>&1; then
  echo "---- ELF type ----"
  "${READELF_BIN}" -h "${OUT_BIN}" | grep -E "Type:|Machine:" || true
  echo "---- PT_INTERP ----"
  "${READELF_BIN}" -l "${OUT_BIN}" | grep -A1 INTERP || true
fi
