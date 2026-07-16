#!/usr/bin/env bash
set -euo pipefail

# Stage prebuilt Wayland demo binaries (GTK + Qt) into a mounted Zenox data volume
# without compiling on host Linux.

VOLUME_ROOT="${1:-/Volumes/ZENOX DATA}"
WORK_ROOT="${2:-/tmp/wayland-demo-stage}"
UBUNTU_MIRROR="${UBUNTU_MIRROR:-https://archive.ubuntu.com/ubuntu}"
CURL_OPTS=(
  "--retry" "3"
  "--retry-delay" "1"
  "--connect-timeout" "15"
  "--max-time" "120"
)

SUITES=(
  "jammy-updates"
  "jammy-security"
  "jammy"
)

COMPONENTS=(
  "main"
  "universe"
)

PACKAGES=(
  "gtk-3-examples"
  "qtbase5-examples"
)

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1" >&2
    exit 1
  }
}

find_package_filename() {
  local pkg="$1"
  local idx found
  for idx in "${INDEX_FILES[@]}"; do
    found="$(
      awk -v pkg="$pkg" '
        BEGIN { RS=""; FS="\n"; }
        {
          p=""; f="";
          for (i = 1; i <= NF; i++) {
            if ($i ~ /^Package: /) { p = substr($i, 10); }
            else if ($i ~ /^Filename: /) { f = substr($i, 11); }
          }
          if (p == pkg && f != "") {
            print f;
            exit;
          }
        }
      ' "$idx"
    )"
    if [[ -n "$found" ]]; then
      printf "%s" "$found"
      return 0
    fi
  done
  return 1
}

extract_deb_payload() {
  local deb="$1"
  local out_dir="$2"
  local ar_dir="${out_dir}/ar"
  local root_dir="${out_dir}/root"

  rm -rf "$out_dir"
  mkdir -p "$ar_dir" "$root_dir"

  (
    cd "$ar_dir"
    ar x "$deb"
  )

  local data_tar
  data_tar="$(find "$ar_dir" -maxdepth 1 -type f -name 'data.tar*' | head -n 1)"
  if [[ -z "$data_tar" ]]; then
    return 1
  fi

  tar -xf "$data_tar" -C "$root_dir" || return 1
  printf "%s" "$root_dir"
}

pick_first_executable() {
  local root="$1"
  shift
  local rel candidate
  for rel in "$@"; do
    candidate="${root}/${rel}"
    if [[ -f "$candidate" && -x "$candidate" ]]; then
      printf "%s" "$candidate"
      return 0
    fi
  done
  return 1
}

need_cmd curl
need_cmd awk
need_cmd xz
need_cmd ar
need_cmd tar
need_cmd find
need_cmd cp
need_cmd basename
need_cmd chmod

if [[ ! -d "$VOLUME_ROOT" ]]; then
  echo "Volume not found: $VOLUME_ROOT" >&2
  exit 1
fi

rm -rf "$WORK_ROOT"
mkdir -p "$WORK_ROOT/index" "$WORK_ROOT/debs" "$WORK_ROOT/unpack"

declare -a INDEX_FILES
for suite in "${SUITES[@]}"; do
  for comp in "${COMPONENTS[@]}"; do
    idx_base="${WORK_ROOT}/index/${suite}_${comp}.Packages"
    idx_url="${UBUNTU_MIRROR}/dists/${suite}/${comp}/binary-amd64/Packages.xz"
    echo "Index: $idx_url"
    if curl -fsSL "${CURL_OPTS[@]}" "$idx_url" -o "${idx_base}.xz"; then
      xz -dc "${idx_base}.xz" > "${idx_base}"
      INDEX_FILES+=("${idx_base}")
    fi
  done
done

if [[ ${#INDEX_FILES[@]} -eq 0 ]]; then
  echo "No package indexes downloaded." >&2
  exit 1
fi

echo "Downloading demo packages..."
for pkg in "${PACKAGES[@]}"; do
  rel_path="$(find_package_filename "$pkg" || true)"
  if [[ -z "$rel_path" ]]; then
    echo "WARN: package not found in indexes: $pkg"
    continue
  fi
  deb_url="${UBUNTU_MIRROR}/${rel_path}"
  deb_out="${WORK_ROOT}/debs/${pkg}.deb"
  echo "  - $pkg"
  if ! curl -fsSL "${CURL_OPTS[@]}" "$deb_url" -o "$deb_out"; then
    echo "WARN: failed to download $pkg ($deb_url)"
    rm -f "$deb_out"
  fi
done

GTK_DEB="${WORK_ROOT}/debs/gtk-3-examples.deb"
QT_DEB="${WORK_ROOT}/debs/qtbase5-examples.deb"
if [[ ! -f "$GTK_DEB" ]]; then
  echo "Missing gtk-3-examples.deb" >&2
  exit 1
fi
if [[ ! -f "$QT_DEB" ]]; then
  echo "Missing qtbase5-examples.deb" >&2
  exit 1
fi

gtk_root="$(extract_deb_payload "$GTK_DEB" "${WORK_ROOT}/unpack/gtk-3-examples" || true)"
if [[ -z "$gtk_root" ]]; then
  echo "Could not extract gtk-3-examples payload." >&2
  exit 1
fi

qt_root="$(extract_deb_payload "$QT_DEB" "${WORK_ROOT}/unpack/qtbase5-examples" || true)"
if [[ -z "$qt_root" ]]; then
  echo "Could not extract qtbase5-examples payload." >&2
  exit 1
fi

gtk_bin="$(pick_first_executable "$gtk_root" "usr/bin/gtk3-demo" "usr/bin/gtk4-demo" || true)"
if [[ -z "$gtk_bin" ]]; then
  echo "No GTK demo executable found in gtk-3-examples package." >&2
  exit 1
fi

qt_bin="$(pick_first_executable \
  "$qt_root" \
  "usr/lib/x86_64-linux-gnu/qt5/examples/widgets/widgets/analogclock/analogclock" \
  "usr/lib/x86_64-linux-gnu/qt5/examples/widgets/widgets/charactermap/charactermap" \
  "usr/lib/x86_64-linux-gnu/qt5/examples/widgets/widgets/calculator/calculator" \
  || true)"
if [[ -z "$qt_bin" ]]; then
  echo "No Qt widgets demo executable found in qtbase5-examples package." >&2
  exit 1
fi

cp -f "$gtk_bin" "${VOLUME_ROOT}/GTKWLDMO.BIN"
chmod +x "${VOLUME_ROOT}/GTKWLDMO.BIN"
cp -f "$qt_bin" "${VOLUME_ROOT}/QTWLDMO.BIN"
chmod +x "${VOLUME_ROOT}/QTWLDMO.BIN"

if [[ -f "${VOLUME_ROOT}/LINUXRT/RTBASE.LST" ]]; then
  echo "Runtime manifest found: ${VOLUME_ROOT}/LINUXRT/RTBASE.LST"
else
  echo "WARN: ${VOLUME_ROOT}/LINUXRT/RTBASE.LST not found. Linux runtime may be incomplete."
fi

cat > "${VOLUME_ROOT}/WAYLAND_DEMOS.TXT" <<'TXT'
Wayland demos listos:
- /GTKWLDMO.BIN (GTK)
- /QTWLDMO.BIN  (Qt)

Prueba directa (sin instalar):
linux inspect /GTKWLDMO.BIN
linux runloop startx /GTKWLDMO.BIN
linux inspect /QTWLDMO.BIN
linux runloop startx /QTWLDMO.BIN

If Qt fails to connect:
linux bridge open
linux runloop startx /QTWLDMO.BIN
TXT

echo "Done."
echo "Staged:"
echo "  ${VOLUME_ROOT}/GTKWLDMO.BIN"
echo "  ${VOLUME_ROOT}/QTWLDMO.BIN"
echo "  ${VOLUME_ROOT}/WAYLAND_DEMOS.TXT"
