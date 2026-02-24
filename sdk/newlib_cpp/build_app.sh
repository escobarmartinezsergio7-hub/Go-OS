#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

SRC_FILE="${1:-${SCRIPT_DIR}/examples/hello_cpp.cpp}"
OUT_FILE="${2:-${REPO_ROOT}/build/newlib_cpp/NEWLIBAPP.BIN}"
OBJ_DIR="${REPO_ROOT}/build/newlib_cpp/obj"

TOOLCHAIN_PREFIX="${NEWLIB_PREFIX:-x86_64-elf-}"
CXX_BIN="${CXX:-${TOOLCHAIN_PREFIX}g++}"
CC_BIN="${CC:-${TOOLCHAIN_PREFIX}gcc}"
READELF_BIN="${READELF:-${TOOLCHAIN_PREFIX}readelf}"

if ! command -v "${CXX_BIN}" >/dev/null 2>&1; then
  echo "Missing compiler: ${CXX_BIN}" >&2
  exit 1
fi

if ! command -v "${CC_BIN}" >/dev/null 2>&1; then
  echo "Missing compiler: ${CC_BIN}" >&2
  exit 1
fi

if ! command -v "${READELF_BIN}" >/dev/null 2>&1; then
  if command -v llvm-readelf >/dev/null 2>&1; then
    READELF_BIN="llvm-readelf"
  elif command -v greadelf >/dev/null 2>&1; then
    READELF_BIN="greadelf"
  elif command -v readelf >/dev/null 2>&1; then
    READELF_BIN="readelf"
  else
    echo "Missing readelf: ${READELF_BIN} (or llvm-readelf/readelf)" >&2
    exit 1
  fi
fi

if [[ ! -f "${SRC_FILE}" ]]; then
  echo "Source file not found: ${SRC_FILE}" >&2
  exit 1
fi

mkdir -p "${OBJ_DIR}" "$(dirname "${OUT_FILE}")"

CRT_OBJ="${OBJ_DIR}/crt0.o"
SYS_OBJ="${OBJ_DIR}/newlib_syscalls.o"
APP_OBJ="${OBJ_DIR}/app.o"

COMMON_CXXFLAGS=(
  -O2
  -pipe
  -fno-pie
  -no-pie
  -ffunction-sections
  -fdata-sections
)

LINK_FLAGS=(
  -static
  -nostartfiles
  -Wl,-e,_start
  -Wl,--gc-sections
  -Wl,--build-id=none
  -Wl,-z,max-page-size=4096
)

echo "[newlib-cpp] compiling crt0..."
"${CC_BIN}" -c "${SCRIPT_DIR}/crt0.S" -o "${CRT_OBJ}"

echo "[newlib-cpp] compiling syscall stubs..."
"${CXX_BIN}" "${COMMON_CXXFLAGS[@]}" -c "${SCRIPT_DIR}/newlib_syscalls.cpp" -o "${SYS_OBJ}"

echo "[newlib-cpp] compiling app source..."
"${CXX_BIN}" "${COMMON_CXXFLAGS[@]}" -c "${SRC_FILE}" -o "${APP_OBJ}"

echo "[newlib-cpp] linking static ELF..."
"${CXX_BIN}" "${LINK_FLAGS[@]}" "${CRT_OBJ}" "${SYS_OBJ}" "${APP_OBJ}" -o "${OUT_FILE}"

echo "[newlib-cpp] validating ELF profile..."
ELF_TYPE="$("${READELF_BIN}" -h "${OUT_FILE}" | awk '/Type:/{print $2; exit}')"
ELF_MACHINE="$("${READELF_BIN}" -h "${OUT_FILE}" | awk '/Machine:/{print $2; exit}')"
HAS_INTERP="$("${READELF_BIN}" -l "${OUT_FILE}" | grep -c ' INTERP ' || true)"
HAS_DYNAMIC="$("${READELF_BIN}" -l "${OUT_FILE}" | grep -c ' DYNAMIC ' || true)"

echo "  type=${ELF_TYPE} machine=${ELF_MACHINE} interp=${HAS_INTERP} dynamic=${HAS_DYNAMIC}"

if [[ "${ELF_TYPE}" != "EXEC"* ]]; then
  echo "[newlib-cpp] ERROR: expected ET_EXEC (use -fno-pie -no-pie)." >&2
  exit 2
fi

if [[ "${HAS_INTERP}" != "0" || "${HAS_DYNAMIC}" != "0" ]]; then
  echo "[newlib-cpp] ERROR: expected static ELF without PT_INTERP/PT_DYNAMIC." >&2
  exit 3
fi

echo "[newlib-cpp] OK -> ${OUT_FILE}"
echo "[newlib-cpp] ReduxOS check: linux inspect /$(basename "${OUT_FILE}")"
