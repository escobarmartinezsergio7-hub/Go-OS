#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVO_ROOT="${1:-${SERVO_ROOT:-$HOME/Desktop/servo}}"
DEST_DIR="${ROOT_DIR}/kernel/third_party/servo/lib"

if [[ ! -d "${SERVO_ROOT}" ]]; then
  echo "ERROR: no existe el repo Servo en: ${SERVO_ROOT}"
  echo "Tip: pasa la ruta manual: scripts/link_servo_desktop.sh /ruta/a/servo"
  exit 1
fi

CANDIDATES=()
while IFS= read -r candidate; do
  CANDIDATES+=("${candidate}")
done < <(
  find "${SERVO_ROOT}" -type f \( -name "libsimpleservo.a" -o -name "simpleservo.lib" \) 2>/dev/null
)

if [[ ${#CANDIDATES[@]} -eq 0 ]]; then
  mapfile_fallback=()
  while IFS= read -r candidate; do
    mapfile_fallback+=("${candidate}")
  done < <(
    find "${SERVO_ROOT}" -type f \( -name "libservoshell*" -o -name "servo" -o -name "servo.exe" \) 2>/dev/null
  )

  if [[ ${#mapfile_fallback[@]} -gt 0 ]]; then
    echo "ERROR: no encontre libsimpleservo.a/simpleservo.lib dentro de ${SERVO_ROOT}"
    echo "Detecte artefactos de servoshell, pero no el bridge requerido:"
    for item in "${mapfile_fallback[@]:0:6}"; do
      echo "  - ${item}"
    done
    echo "Tu kernel espera simbolos C:"
    echo "  simpleservo_bridge_is_ready / simpleservo_bridge_render_text"
    echo "Necesitas un adapter libsimpleservo que exporte esos simbolos."
    exit 2
  fi

  echo "ERROR: no encontre libsimpleservo.a/simpleservo.lib dentro de ${SERVO_ROOT}"
  echo "Compila primero la libreria bridge en tu checkout de Servo."
  exit 1
fi

PICKED=""
PICKED_MTIME=0
for candidate in "${CANDIDATES[@]}"; do
  if mtime="$(stat -f %m "${candidate}" 2>/dev/null)"; then
    :
  else
    mtime="$(stat -c %Y "${candidate}")"
  fi
  if [[ -z "${PICKED}" || "${mtime}" -gt "${PICKED_MTIME}" ]]; then
    PICKED="${candidate}"
    PICKED_MTIME="${mtime}"
  fi
done

mkdir -p "${DEST_DIR}"
dest_name="$(basename "${PICKED}")"
cp -f "${PICKED}" "${DEST_DIR}/${dest_name}"

echo "Servo bridge copiado:"
echo "  src: ${PICKED}"
echo "  dst: ${DEST_DIR}/${dest_name}"
echo
echo "Build sugerido (UEFI + Servo externo):"
echo "  cargo build --manifest-path kernel/Cargo.toml --target x86_64-unknown-uefi --features \"servo_bridge,servo_external\""
echo "Opcional:"
echo "  SERVO_LIB_DIR=${DEST_DIR}"
