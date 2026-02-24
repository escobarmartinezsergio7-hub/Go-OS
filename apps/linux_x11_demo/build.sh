#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_BIN="${1:-${SCRIPT_DIR}/X11DEMO.BIN}"
BUILD_DIR="${SCRIPT_DIR}/.build"
OBJ_FILE="${BUILD_DIR}/x11_demo.o"
ASM_FILE="${SCRIPT_DIR}/x11_demo.asm"

if [[ -n "${NASM:-}" ]]; then
  NASM_BIN="${NASM}"
elif command -v nasm >/dev/null 2>&1; then
  NASM_BIN="$(command -v nasm)"
elif [[ -x /opt/homebrew/bin/nasm ]]; then
  NASM_BIN="/opt/homebrew/bin/nasm"
else
  NASM_BIN="nasm"
fi

HOST_TRIPLE="$(rustc -vV | awk '/^host: / { print $2 }')"
SYSROOT="$(rustc --print sysroot)"
RUST_LLD_DEFAULT="${SYSROOT}/lib/rustlib/${HOST_TRIPLE}/bin/rust-lld"
RUST_LLD_BIN="${RUST_LLD:-${RUST_LLD_DEFAULT}}"

if [[ ! -x "${NASM_BIN}" ]]; then
  echo "build error: nasm no encontrado (${NASM_BIN})." >&2
  exit 1
fi
if [[ ! -x "${RUST_LLD_BIN}" ]]; then
  echo "build error: rust-lld no encontrado (${RUST_LLD_BIN})." >&2
  exit 1
fi

mkdir -p "${BUILD_DIR}"

"${NASM_BIN}" -f elf64 "${ASM_FILE}" -o "${OBJ_FILE}"
"${RUST_LLD_BIN}" -flavor gnu -m elf_x86_64 -nostdlib -static -e _start -o "${OUT_BIN}" "${OBJ_FILE}"

echo "Built Linux ELF: ${OUT_BIN}"
