#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
SRC_DIR="${ROOT_DIR}/third_party/cpp-doom"
BUILD_DIR="${SRC_DIR}/build-redux"
OUT_BIN="${1:-${SCRIPT_DIR}/CPPDOOM.BIN}"

if [[ ! -d "${SRC_DIR}" ]]; then
  echo "build error: repo no encontrado en ${SRC_DIR}" >&2
  exit 1
fi
if [[ "$(uname -s)" != "Linux" ]]; then
  echo "build error: cpp-doom debe compilarse en Linux para generar ELF64 ejecutable en Go OS." >&2
  echo "Sugerencia: usa una VM/host Linux y vuelve a ejecutar este script." >&2
  exit 1
fi
if ! command -v cmake >/dev/null 2>&1; then
  echo "build error: cmake no encontrado." >&2
  exit 1
fi

JOBS=4
if command -v nproc >/dev/null 2>&1; then
  JOBS="$(nproc)"
fi

cmake \
  -S "${SRC_DIR}" \
  -B "${BUILD_DIR}" \
  -DCMAKE_BUILD_TYPE=Release \
  -DDOOMPP_ENABLE_ASAN=OFF

cmake --build "${BUILD_DIR}" --config Release -j"${JOBS}"

BIN_CANDIDATES=(
  "${BUILD_DIR}/doom++"
  "${BUILD_DIR}/Release/doom++"
)

BIN_SRC=""
for candidate in "${BIN_CANDIDATES[@]}"; do
  if [[ -f "${candidate}" ]]; then
    BIN_SRC="${candidate}"
    break
  fi
done

if [[ -z "${BIN_SRC}" ]]; then
  echo "build error: no se encontro binario doom++ en ${BUILD_DIR}" >&2
  exit 1
fi

mkdir -p "$(dirname "${OUT_BIN}")"
cp -f "${BIN_SRC}" "${OUT_BIN}"
chmod +x "${OUT_BIN}"

echo "Built CPP-DOOM ELF: ${OUT_BIN}"
