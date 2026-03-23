#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEFAULT_SERVO_ROOT="/Users/mac/Desktop/servo"
DEFAULT_DEST_ROOT="${ROOT_DIR}/SERVORT"
DEFAULT_DEST_NAME="SVRT0001.BIN"

show_help() {
  cat <<'EOF'
Usage:
  bash scripts/stage_servort_runtime.sh [options]

Options:
  --bin <path>         Ruta directa al binario Servo Linux ELF.
  --servo-root <dir>   Root del checkout Servo para autodetectar binario.
                       Default: /Users/mac/Desktop/servo
  --bundle-root <dir>  Directorio bundle que contenga resources/ (opcional).
  --dest-root <dir>    Directorio destino para ServoRT.
                       Default: ./SERVORT
  --dest-name <name>   Nombre destino del ejecutable.
                       Default: SVRT0001.BIN
  --esp-dir <dir>      Si se indica, copia tambien a <dir>/SERVORT/<dest-name>.
  -h, --help           Muestra esta ayuda.

Examples:
  bash scripts/stage_servort_runtime.sh
  bash scripts/stage_servort_runtime.sh --bin /Users/mac/Desktop/servo/target/release/servo
  bash scripts/stage_servort_runtime.sh --bin /tmp/servo/servo --bundle-root /tmp/servo
  bash scripts/stage_servort_runtime.sh --bin /tmp/servo --esp-dir build/esp
EOF
}

pick_latest_file() {
  local picked=""
  local picked_mtime=0
  local candidate mtime
  for candidate in "$@"; do
    [[ -f "${candidate}" ]] || continue
    if mtime="$(stat -f %m "${candidate}" 2>/dev/null)"; then
      :
    elif mtime="$(stat -c %Y "${candidate}" 2>/dev/null)"; then
      :
    else
      mtime=0
    fi
    if [[ -z "${picked}" || "${mtime}" -gt "${picked_mtime}" ]]; then
      picked="${candidate}"
      picked_mtime="${mtime}"
    fi
  done
  printf "%s" "${picked}"
}

pick_readelf_bin() {
  if command -v readelf >/dev/null 2>&1; then
    echo "readelf"
    return 0
  fi
  if command -v llvm-readelf >/dev/null 2>&1; then
    echo "llvm-readelf"
    return 0
  fi
  if command -v greadelf >/dev/null 2>&1; then
    echo "greadelf"
    return 0
  fi
  return 1
}

print_needed_libs() {
  local bin_path="$1"
  local readelf_bin
  if ! readelf_bin="$(pick_readelf_bin)"; then
    return 0
  fi
  local libs
  libs="$(
    "${readelf_bin}" -d "${bin_path}" 2>/dev/null \
      | awk '/NEEDED/ { gsub(/\[/, "", $5); gsub(/\]/, "", $5); print $5 }' \
      | awk 'NF > 0 { print }' \
      | sort -u
  )"
  if [[ -z "${libs}" ]]; then
    return 0
  fi
  echo "Dependencias dinamicas detectadas:"
  while IFS= read -r lib; do
    [[ -n "${lib}" ]] || continue
    echo "  - ${lib}"
  done <<<"${libs}"
  echo "Asegura que esas libs existan dentro de /LINUXRT (RTBASE.LST + runtime map)."
}

is_valid_linux_elf_x86_64() {
  local bin_path="$1"
  local info
  info="$(file -b "${bin_path}" 2>/dev/null || true)"
  if [[ -z "${info}" ]]; then
    echo "No se pudo inspeccionar formato con 'file': ${bin_path}" >&2
    return 1
  fi
  if ! grep -Eqi 'ELF' <<<"${info}"; then
    echo "Formato invalido: se esperaba ELF Linux y se obtuvo: ${info}" >&2
    return 1
  fi
  if ! grep -Eqi 'x86[-_ ]64|x86_64|amd64' <<<"${info}"; then
    echo "Arquitectura invalida: se esperaba x86_64 y se obtuvo: ${info}" >&2
    return 1
  fi
  return 0
}

SERVO_ROOT="${DEFAULT_SERVO_ROOT}"
SOURCE_BIN=""
BUNDLE_ROOT=""
DEST_ROOT="${DEFAULT_DEST_ROOT}"
DEST_NAME="${DEFAULT_DEST_NAME}"
ESP_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bin)
      SOURCE_BIN="${2:-}"
      shift 2
      ;;
    --servo-root)
      SERVO_ROOT="${2:-}"
      shift 2
      ;;
    --bundle-root)
      BUNDLE_ROOT="${2:-}"
      shift 2
      ;;
    --dest-root)
      DEST_ROOT="${2:-}"
      shift 2
      ;;
    --dest-name)
      DEST_NAME="${2:-}"
      shift 2
      ;;
    --esp-dir)
      ESP_DIR="${2:-}"
      shift 2
      ;;
    -h|--help)
      show_help
      exit 0
      ;;
    *)
      echo "Argumento no soportado: $1" >&2
      show_help >&2
      exit 1
      ;;
  esac
done

if [[ -z "${DEST_NAME}" ]]; then
  echo "Error: --dest-name no puede estar vacio." >&2
  exit 1
fi

if [[ -z "${SOURCE_BIN}" ]]; then
  if [[ ! -d "${SERVO_ROOT}" ]]; then
    echo "Error: no existe --servo-root: ${SERVO_ROOT}" >&2
    echo "Tip: usa --bin /ruta/al/servo Linux ELF." >&2
    exit 1
  fi

  declare -a CANDIDATES
  CANDIDATES=(
    "${SERVO_ROOT}/target/release/servo"
    "${SERVO_ROOT}/target/release/servoshell"
    "${SERVO_ROOT}/target/debug/servo"
    "${SERVO_ROOT}/target/debug/servoshell"
    "${SERVO_ROOT}/ports/servoshell/target/release/servo"
    "${SERVO_ROOT}/ports/servoshell/target/release/servoshell"
    "${SERVO_ROOT}/ports/servoshell/target/debug/servo"
    "${SERVO_ROOT}/ports/servoshell/target/debug/servoshell"
  )

  while IFS= read -r discovered; do
    [[ -n "${discovered}" ]] || continue
    CANDIDATES+=("${discovered}")
  done < <(
    find "${SERVO_ROOT}" \
      -type f \
      \( -name "servo" -o -name "servoshell" -o -name "servo-bin" \) \
      -path "*/target/*" 2>/dev/null \
      | head -n 80
  )

  SOURCE_BIN="$(pick_latest_file "${CANDIDATES[@]}")"
  if [[ -z "${SOURCE_BIN}" ]]; then
    echo "Error: no se encontro binario Servo en ${SERVO_ROOT}" >&2
    echo "Busque nombres: servo, servoshell dentro de target/." >&2
    echo "Tip: pasa --bin /ruta/servo para seleccion manual." >&2
    exit 1
  fi
fi

if [[ ! -f "${SOURCE_BIN}" ]]; then
  echo "Error: --bin no existe: ${SOURCE_BIN}" >&2
  exit 1
fi

if ! is_valid_linux_elf_x86_64 "${SOURCE_BIN}"; then
  echo "Este runtime solo acepta binarios Linux ELF x86_64 para LinuxRT." >&2
  echo "Si compilaste Servo en macOS/Windows, genera la build Linux y vuelve a stagear." >&2
  exit 1
fi

mkdir -p "${DEST_ROOT}"
DEST_BIN="${DEST_ROOT%/}/${DEST_NAME}"
cp -f "${SOURCE_BIN}" "${DEST_BIN}"
chmod +x "${DEST_BIN}"

if [[ -z "${BUNDLE_ROOT}" ]]; then
  source_dir="$(cd "$(dirname "${SOURCE_BIN}")" && pwd)"
  if [[ -d "${source_dir}/resources" ]]; then
    BUNDLE_ROOT="${source_dir}"
  fi
fi

if [[ -n "${BUNDLE_ROOT}" && -d "${BUNDLE_ROOT}/resources" ]]; then
  rm -rf "${DEST_ROOT%/}/resources"
  cp -R "${BUNDLE_ROOT}/resources" "${DEST_ROOT%/}/resources"
fi

ESP_BIN=""
if [[ -n "${ESP_DIR}" ]]; then
  mkdir -p "${ESP_DIR%/}/SERVORT"
  ESP_BIN="${ESP_DIR%/}/SERVORT/${DEST_NAME}"
  cp -f "${DEST_BIN}" "${ESP_BIN}"
  chmod +x "${ESP_BIN}"
  if [[ -n "${BUNDLE_ROOT}" && -d "${BUNDLE_ROOT}/resources" ]]; then
    rm -rf "${ESP_DIR%/}/SERVORT/resources"
    cp -R "${BUNDLE_ROOT}/resources" "${ESP_DIR%/}/SERVORT/resources"
  fi
fi

echo "ServoRT stage OK."
echo "  source: ${SOURCE_BIN}"
echo "  local : ${DEST_BIN}"
if [[ -n "${ESP_BIN}" ]]; then
  echo "  esp   : ${ESP_BIN}"
fi
if [[ -n "${BUNDLE_ROOT}" && -d "${BUNDLE_ROOT}/resources" ]]; then
  echo "  assets: ${DEST_ROOT%/}/resources"
fi
echo "Kernel target path sugerido: /SERVORT/${DEST_NAME}"
print_needed_libs "${DEST_BIN}"
