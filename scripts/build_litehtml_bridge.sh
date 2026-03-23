#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LITEHTML_DIR="${ROOT_DIR}/kernel/third_party/litehtml"
UPSTREAM_DIR="${LITEHTML_DIR}/upstream"
BUILD_DIR="${LITEHTML_DIR}/build-win64"
BRIDGE_SRC="${LITEHTML_DIR}/bridge/litehtml_bridge.cpp"
LIB_DIR="${LITEHTML_DIR}/lib"
OUT_LIB="${LIB_DIR}/liblitehtmlbridge.a"

CMAKE_BIN="${CMAKE_BIN:-cmake}"
NINJA_BIN="${NINJA_BIN:-ninja}"
CC="${CC:-x86_64-w64-mingw32-gcc}"
CXX="${CXX:-x86_64-w64-mingw32-g++}"
LLVM_AR="${LLVM_AR:-/opt/homebrew/opt/llvm/bin/llvm-ar}"
NM_BIN="${NM_BIN:-x86_64-w64-mingw32-nm}"
OBJCOPY_BIN="${OBJCOPY_BIN:-x86_64-w64-mingw32-objcopy}"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

require_cmd "${CMAKE_BIN}"
require_cmd "${NINJA_BIN}"
require_cmd "${CC}"
require_cmd "${CXX}"
if [ ! -x "${LLVM_AR}" ]; then
  if command -v llvm-ar >/dev/null 2>&1; then
    LLVM_AR="$(command -v llvm-ar)"
  else
    echo "Missing llvm-ar (set LLVM_AR=/path/to/llvm-ar)" >&2
    exit 1
  fi
fi
require_cmd "${NM_BIN}"
require_cmd "${OBJCOPY_BIN}"

if [ ! -d "${UPSTREAM_DIR}" ]; then
  echo "[litehtml] upstream missing; syncing..."
  bash "${ROOT_DIR}/scripts/sync_litehtml_upstream.sh"
fi

if [ ! -f "${BRIDGE_SRC}" ]; then
  echo "Bridge source not found: ${BRIDGE_SRC}" >&2
  exit 1
fi

mkdir -p "${LIB_DIR}"

echo "[litehtml] configuring CMake build..."
"${CMAKE_BIN}" -S "${UPSTREAM_DIR}" -B "${BUILD_DIR}" -G Ninja \
  -DCMAKE_SYSTEM_NAME=Windows \
  -DCMAKE_C_COMPILER="${CC}" \
  -DCMAKE_CXX_COMPILER="${CXX}" \
  -DBUILD_SHARED_LIBS=OFF \
  -DLITEHTML_BUILD_TESTING=OFF

echo "[litehtml] building gumbo + litehtml..."
"${CMAKE_BIN}" --build "${BUILD_DIR}" --target gumbo litehtml -j8

BRIDGE_OBJ="${BUILD_DIR}/litehtml_bridge.o"

echo "[litehtml] compiling bridge..."
"${CXX}" -std=c++17 -O2 -DNDEBUG -fno-exceptions -fno-rtti \
  -I"${UPSTREAM_DIR}/include" \
  -I"${UPSTREAM_DIR}/include/litehtml" \
  -I"${UPSTREAM_DIR}/src" \
  -I"${UPSTREAM_DIR}/src/gumbo" \
  -c "${BRIDGE_SRC}" -o "${BRIDGE_OBJ}"

if [ ! -f "${BUILD_DIR}/liblitehtml.a" ]; then
  echo "Missing ${BUILD_DIR}/liblitehtml.a" >&2
  exit 1
fi
if [ ! -f "${BUILD_DIR}/src/gumbo/libgumbo.a" ]; then
  echo "Missing ${BUILD_DIR}/src/gumbo/libgumbo.a" >&2
  exit 1
fi

tmp_merge="$(mktemp -d)"
cleanup() {
  rm -rf "${tmp_merge}"
}
trap cleanup EXIT

echo "[litehtml] merging static archives..."
(
  cd "${tmp_merge}"
  rm -f "${OUT_LIB}"
  "${LLVM_AR}" x "${BUILD_DIR}/liblitehtml.a"
  "${LLVM_AR}" x "${BUILD_DIR}/src/gumbo/libgumbo.a"
  cp "${BRIDGE_OBJ}" ./
  # GNU COFF objects can carry absolute helper symbols like
  # ".weak.__cxa_pure_virtual.*" in many units; rust-lld treats these as
  # duplicate definitions. Strip those aliases before repacking.
  for obj in ./*.o ./*.obj; do
    [ -e "${obj}" ] || continue
    while IFS= read -r weak_sym; do
      [ -n "${weak_sym}" ] || continue
      "${OBJCOPY_BIN}" --strip-symbol "${weak_sym}" "${obj}" || true
    done < <("${NM_BIN}" "${obj}" 2>/dev/null | awk '/\\.weak\\.__cxa_pure_virtual/{print $3}')
  done
  "${LLVM_AR}" rcs "${OUT_LIB}" ./*.o ./*.obj
)

echo "[litehtml] built ${OUT_LIB}"
ls -lh "${OUT_LIB}"
