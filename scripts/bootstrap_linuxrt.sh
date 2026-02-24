#!/usr/bin/env bash
set -euo pipefail

# Bootstrap a base Linux runtime into /LINUXRT on a mounted REDUXOS volume.
# - Downloads Ubuntu .deb packages (amd64)
# - Extracts shared libraries and loader
# - Stages 8.3 filenames under /LINUXRT/{LIB,LIB64,USR/LIB,USR/LIB64}
# - Writes mapping manifest /LINUXRT/RTBASE.LST

VOLUME_ROOT="${1:-/Volumes/REDUXOS}"
RUNTIME_ROOT="${VOLUME_ROOT}/LINUXRT"
WORK_ROOT="${2:-/tmp/linuxrt-bootstrap}"
UBUNTU_MIRROR="${UBUNTU_MIRROR:-https://archive.ubuntu.com/ubuntu}"

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
  "libc6"
  "libgcc-s1"
  "libstdc++6"
  "zlib1g"
  "libffi8"
  "libpcre2-8-0"
  "libglib2.0-0"
  "libgdk-pixbuf-2.0-0"
  "libgtk-3-0"
  "libdbus-1-3"
  "libatk1.0-0"
  "libatk-bridge2.0-0"
  "libcups2"
  "libpango-1.0-0"
  "libcairo2"
  "libx11-6"
  "libx11-xcb1"
  "libxcomposite1"
  "libxdamage1"
  "libxext6"
  "libxfixes3"
  "libxrandr2"
  "libgbm1"
  "libexpat1"
  "libxcb1"
  "libxkbcommon0"
  "libudev1"
  "libasound2"
  "libatspi2.0-0"
  "libnspr4"
  "libnss3"
  "libxrender1"
  "libfontconfig1"
  "libfreetype6"
  "libpng16-16"
  "libharfbuzz0b"
  "libgraphite2-3"
  "libthai0"
  "libfribidi0"
  "libepoxy0"
  "libwayland-client0"
  "libwayland-cursor0"
  "libwayland-egl1"
  "libdrm2"
  "libxinerama1"
  "libxi6"
  "libxcursor1"
  "libxss1"
  "libxtst6"
  "libxshmfence1"
)

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1" >&2
    exit 1
  }
}

sanitize_component() {
  local text="$1"
  local max_len="$2"
  local fallback="$3"
  local out=""
  local i ch up

  for ((i = 0; i < ${#text}; i++)); do
    ch="${text:i:1}"
    case "$ch" in
      [[:alnum:]_-])
        up="$(printf "%s" "$ch" | tr '[:lower:]' '[:upper:]')"
        out+="$up"
        ;;
    esac
    if [[ ${#out} -ge ${max_len} ]]; then
      break
    fi
  done

  if [[ -z "$out" ]]; then
    for ((i = 0; i < ${#fallback}; i++)); do
      ch="${fallback:i:1}"
      case "$ch" in
        [[:alnum:]_-])
          up="$(printf "%s" "$ch" | tr '[:lower:]' '[:upper:]')"
          out+="$up"
          ;;
      esac
      if [[ ${#out} -ge ${max_len} ]]; then
        break
      fi
    done
  fi

  if [[ -z "$out" ]]; then
    out="X"
  fi

  printf "%s" "$out"
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

pick_runtime_bucket() {
  local rel_lower="$1"
  if [[ "$rel_lower" == usr/lib64/* ]]; then
    printf "%s" "${RUNTIME_ROOT}/USR/LIB64"
  elif [[ "$rel_lower" == usr/lib/* ]]; then
    printf "%s" "${RUNTIME_ROOT}/USR/LIB"
  elif [[ "$rel_lower" == lib64/* ]]; then
    printf "%s" "${RUNTIME_ROOT}/LIB64"
  else
    printf "%s" "${RUNTIME_ROOT}/LIB"
  fi
}

is_runtime_candidate() {
  local rel_lower="$1"
  local leaf
  leaf="$(basename "$rel_lower")"

  case "$rel_lower" in
    lib/*|lib64/*|usr/lib/*|usr/lib64/*) ;;
    *) return 1 ;;
  esac

  case "$leaf" in
    *.so|*.so.*|ld-linux*|ld-musl*) return 0 ;;
    *) return 1 ;;
  esac
}

need_cmd curl
need_cmd awk
need_cmd xz
need_cmd ar
need_cmd tar
need_cmd find
need_cmd tr
need_cmd grep
need_cmd sort
need_cmd wc
need_cmd basename
need_cmd cp

if [[ ! -d "$VOLUME_ROOT" ]]; then
  echo "Volume not found: $VOLUME_ROOT" >&2
  exit 1
fi

mkdir -p "$RUNTIME_ROOT/LIB" "$RUNTIME_ROOT/LIB64" "$RUNTIME_ROOT/USR/LIB" "$RUNTIME_ROOT/USR/LIB64"

rm -rf "$WORK_ROOT"
mkdir -p "$WORK_ROOT/index" "$WORK_ROOT/debs" "$WORK_ROOT/unpack"

declare -a INDEX_FILES
for suite in "${SUITES[@]}"; do
  for comp in "${COMPONENTS[@]}"; do
    idx_base="${WORK_ROOT}/index/${suite}_${comp}.Packages"
    idx_url="${UBUNTU_MIRROR}/dists/${suite}/${comp}/binary-amd64/Packages.xz"
    echo "Index: $idx_url"
    if curl -fsSL "$idx_url" -o "${idx_base}.xz"; then
      xz -dc "${idx_base}.xz" > "${idx_base}"
      INDEX_FILES+=("${idx_base}")
    fi
  done
done

if [[ ${#INDEX_FILES[@]} -eq 0 ]]; then
  echo "No package indexes downloaded." >&2
  exit 1
fi

echo "Downloading runtime packages..."
for pkg in "${PACKAGES[@]}"; do
  rel_path="$(find_package_filename "$pkg" || true)"
  if [[ -z "$rel_path" ]]; then
    echo "WARN: package not found in indexes: $pkg"
    continue
  fi
  deb_url="${UBUNTU_MIRROR}/${rel_path}"
  deb_out="${WORK_ROOT}/debs/${pkg}.deb"
  echo "  - $pkg"
  if ! curl -fsSL "$deb_url" -o "$deb_out"; then
    echo "WARN: failed to download $pkg ($deb_url)"
    rm -f "$deb_out"
  fi
done

MANIFEST_TMP="${WORK_ROOT}/RTBASE.LST"
SEEN_TMP="${WORK_ROOT}/seen_paths.txt"
: > "$SEEN_TMP"
echo "LINUXRT INSTALL" > "$MANIFEST_TMP"

counter=0
staged=0

for deb in "$WORK_ROOT"/debs/*.deb; do
  [[ -f "$deb" ]] || continue
  deb_name="$(basename "$deb" .deb)"
  pkg_dir="${WORK_ROOT}/unpack/${deb_name}"
  ar_dir="${pkg_dir}/ar"
  root_dir="${pkg_dir}/root"
  rm -rf "$pkg_dir"
  mkdir -p "$ar_dir" "$root_dir"

  (
    cd "$ar_dir"
    ar x "$deb"
  )

  data_tar="$(find "$ar_dir" -maxdepth 1 -type f -name 'data.tar*' | head -n 1)"
  if [[ -z "$data_tar" ]]; then
    echo "WARN: no data.tar* in ${deb_name}.deb"
    continue
  fi

  if ! tar -xf "$data_tar" -C "$root_dir"; then
    echo "WARN: could not extract payload from ${deb_name}.deb"
    continue
  fi

  while IFS= read -r -d '' path_abs; do
    rel="${path_abs#${root_dir}/}"
    rel_lower="$(printf "%s" "$rel" | tr '[:upper:]' '[:lower:]')"

    if ! is_runtime_candidate "$rel_lower"; then
      continue
    fi

    if grep -Fqx "$rel_lower" "$SEEN_TMP"; then
      continue
    fi

    leaf="$(basename "$rel")"
    if [[ "$leaf" == *.* ]]; then
      ext_src="${leaf##*.}"
    else
      ext_src="BIN"
    fi
    ext="$(sanitize_component "$ext_src" 3 "BIN")"
    stem="$(sanitize_component "$(printf "RTB%04d" $((counter % 10000)))" 8 "RTBASE")"
    short_name="${stem}.${ext}"

    dst_dir="$(pick_runtime_bucket "$rel_lower")"
    mkdir -p "$dst_dir"

    if cp -fL "$path_abs" "${dst_dir}/${short_name}" 2>/dev/null; then
      printf "%s\n" "$rel_lower" >> "$SEEN_TMP"
      counter=$((counter + 1))
      staged=$((staged + 1))
      printf "%04d %s <- %s\n" "$counter" "$short_name" "$rel_lower" >> "$MANIFEST_TMP"
    fi
  done < <(find "$root_dir" \( -type f -o -type l \) -print0)
done

if [[ "$staged" -eq 0 ]]; then
  echo "No runtime libraries were staged." >&2
  exit 1
fi

cp -f "$MANIFEST_TMP" "${RUNTIME_ROOT}/RTBASE.LST"

echo "Done."
echo "Runtime staged files: $staged"
echo "Manifest: ${RUNTIME_ROOT}/RTBASE.LST"
echo "You can now boot REDUXOS and retry the shortcut."
