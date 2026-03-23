#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE_DIR="${ROOT_DIR}/tools/libsimpleservo_adapter"
TARGET="${1:-x86_64-unknown-uefi}"
PROFILE="${PROFILE:-release}"
DEST_DIR="${ROOT_DIR}/kernel/third_party/servo/lib"

if [[ ! -f "${CRATE_DIR}/Cargo.toml" ]]; then
  echo "ERROR: no existe crate adapter en ${CRATE_DIR}"
  exit 1
fi

echo "Building libsimpleservo adapter..."
echo "  target:  ${TARGET}"
echo "  profile: ${PROFILE}"

cargo build --manifest-path "${CRATE_DIR}/Cargo.toml" --target "${TARGET}" --profile "${PROFILE}"

LIB_NAME="libsimpleservo.a"
SRC_LIB="${CRATE_DIR}/target/${TARGET}/${PROFILE}/${LIB_NAME}"
if [[ ! -f "${SRC_LIB}" ]]; then
  echo "ERROR: build termino pero no encontre ${SRC_LIB}"
  exit 2
fi

mkdir -p "${DEST_DIR}"
cp -f "${SRC_LIB}" "${DEST_DIR}/${LIB_NAME}"

echo "OK: libreria instalada"
echo "  src: ${SRC_LIB}"
echo "  dst: ${DEST_DIR}/${LIB_NAME}"
echo
echo "Compila kernel con enlace externo:"
echo "  cargo build --manifest-path kernel/Cargo.toml --target ${TARGET} --features \"servo_bridge,servo_external\""
