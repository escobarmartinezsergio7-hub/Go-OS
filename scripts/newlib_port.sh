#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SDK_DIR="${REPO_ROOT}/sdk/newlib_cpp"
BUILD_SCRIPT="${SDK_DIR}/build_app.sh"

usage() {
  cat <<'EOF'
Usage:
  bash scripts/newlib_port.sh scaffold <app_name> [dest_dir]
  bash scripts/newlib_port.sh build <main.cpp> [out_file]
  bash scripts/newlib_port.sh doctor <elf_file>

Examples:
  bash scripts/newlib_port.sh scaffold myapp
  bash scripts/newlib_port.sh build apps/newlib/myapp/main.cpp build/newlib_cpp/MYAPP.BIN
  bash scripts/newlib_port.sh doctor build/newlib_cpp/MYAPP.BIN
EOF
}

pick_readelf() {
  if [[ -n "${READELF:-}" ]] && command -v "${READELF}" >/dev/null 2>&1; then
    echo "${READELF}"
    return 0
  fi
  if command -v x86_64-elf-readelf >/dev/null 2>&1; then
    echo "x86_64-elf-readelf"
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
  if command -v readelf >/dev/null 2>&1; then
    echo "readelf"
    return 0
  fi
  return 1
}

command="${1:-}"
if [[ -z "${command}" ]]; then
  usage
  exit 0
fi
shift || true

case "${command}" in
  scaffold)
    app_name="${1:-}"
    if [[ -z "${app_name}" ]]; then
      echo "Missing app_name." >&2
      usage
      exit 1
    fi
    dest_dir="${2:-${REPO_ROOT}/apps/newlib/${app_name}}"
    mkdir -p "${dest_dir}"
    main_file="${dest_dir}/main.cpp"
    build_file="${dest_dir}/build.sh"

    cat > "${main_file}" <<'CPP'
#include <iostream>
#include <string>

int main(int argc, char** argv) {
    std::cout << "newlib C++ app in ReduxOS profile\n";
    std::cout << "argc=" << argc << "\n";
    if (argc > 0 && argv[0]) {
        std::cout << "argv0=" << argv[0] << "\n";
    }
    std::string msg = "replace this with your app";
    std::cout << msg << "\n";
    return 0;
}
CPP

    cat > "${build_file}" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="${SCRIPT_DIR}"
while [[ "${ROOT_DIR}" != "/" && ! -d "${ROOT_DIR}/sdk/newlib_cpp" ]]; do
    ROOT_DIR="$(dirname "${ROOT_DIR}")"
done
if [[ ! -d "${ROOT_DIR}/sdk/newlib_cpp" ]]; then
    echo "No se encontro raiz del repo (sdk/newlib_cpp)." >&2
    exit 1
fi
SRC_FILE="${SCRIPT_DIR}/main.cpp"
OUT_FILE="${ROOT_DIR}/build/newlib_cpp/APP.BIN"
bash "${ROOT_DIR}/scripts/newlib_port.sh" build "${SRC_FILE}" "${OUT_FILE}"
SH
    chmod +x "${build_file}"
    echo "Scaffold created at: ${dest_dir}"
    echo "Next: bash ${build_file}"
    ;;

  build)
    src_file="${1:-${SDK_DIR}/examples/hello_cpp.cpp}"
    out_file="${2:-${REPO_ROOT}/build/newlib_cpp/NEWLIBAPP.BIN}"
    bash "${BUILD_SCRIPT}" "${src_file}" "${out_file}"
    ;;

  doctor)
    elf_file="${1:-}"
    if [[ -z "${elf_file}" ]]; then
      echo "Missing elf_file." >&2
      usage
      exit 1
    fi
    if [[ ! -f "${elf_file}" ]]; then
      echo "File not found: ${elf_file}" >&2
      exit 1
    fi
    if ! re_bin="$(pick_readelf)"; then
      echo "Missing readelf (set READELF or install llvm-readelf/readelf/x86_64-elf-readelf)." >&2
      exit 1
    fi

    if ! "${re_bin}" -h "${elf_file}" >/tmp/newlib_port_readelf.$$ 2>/tmp/newlib_port_readelf_err.$$; then
      echo "newlib port doctor: ${elf_file}"
      echo "  FAIL: no es un ELF valido para readelf."
      rm -f /tmp/newlib_port_readelf.$$ /tmp/newlib_port_readelf_err.$$
      exit 2
    fi

    type_line="$(awk '/Type:/{print $2; exit}' /tmp/newlib_port_readelf.$$)"
    machine_line="$(awk -F: '/Machine:/{gsub(/^[[:space:]]+/, "", $2); print $2; exit}' /tmp/newlib_port_readelf.$$)"
    interp_count="$("${re_bin}" -l "${elf_file}" | grep -c ' INTERP ' || true)"
    dynamic_count="$("${re_bin}" -l "${elf_file}" | grep -c ' DYNAMIC ' || true)"
    tls_count="$("${re_bin}" -l "${elf_file}" | grep -c ' TLS ' || true)"
    rm -f /tmp/newlib_port_readelf.$$ /tmp/newlib_port_readelf_err.$$

    echo "newlib port doctor: ${elf_file}"
    echo "  Type=${type_line}"
    echo "  Machine=${machine_line}"
    echo "  PT_INTERP=${interp_count}"
    echo "  PT_DYNAMIC=${dynamic_count}"
    echo "  PT_TLS=${tls_count}"

    fail=0
    machine_lc="$(printf '%s' "${machine_line}" | tr '[:upper:]' '[:lower:]')"
    if [[ "${machine_lc}" != *"x86-64"* && "${machine_lc}" != *"x86_64"* ]]; then
      echo "  FAIL: expected x86_64 machine."
      fail=1
    fi
    if [[ "${type_line}" != "EXEC"* ]]; then
      echo "  FAIL: expected ET_EXEC (use -fno-pie -no-pie)."
      fail=1
    fi
    if [[ "${interp_count}" != "0" ]]; then
      echo "  FAIL: PT_INTERP present (must be static)."
      fail=1
    fi
    if [[ "${dynamic_count}" != "0" ]]; then
      echo "  FAIL: PT_DYNAMIC present (must be static)."
      fail=1
    fi
    if [[ "${tls_count}" != "0" ]]; then
      echo "  WARN: PT_TLS detected (fase1 ReduxOS no soporta TLS static)."
    fi

    if [[ "${fail}" -ne 0 ]]; then
      exit 2
    fi
    echo "  OK: perfil newlib C++ estatico compatible con fase1."
    ;;

  *)
    usage
    exit 1
    ;;
esac
