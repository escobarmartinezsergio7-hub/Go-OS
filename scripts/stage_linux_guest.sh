#!/usr/bin/env bash
set -euo pipefail

ESP_DIR="build/esp"
LINUX_TREE=""
EFI_INPUT=""
DO_BUILD=0
KERNEL_OUT="${KERNEL_OUT:-/tmp/linux-guest-build}"
JOBS="${JOBS:-}"

usage() {
  cat <<'USAGE'
Usage:
  bash scripts/stage_linux_guest.sh [options]

Options:
  --esp-dir <dir>      ESP root directory (default: build/esp)
  --linux-tree <dir>   Linux source tree (e.g. /Users/mac/Downloads/linux-6.19.3)
  --efi-input <file>   Prebuilt Linux EFI image (or bzImage EFI-stub) to stage
  --build              Build bzImage from --linux-tree before staging
  --help, -h           Show this help

Notes:
  - Output path is always: <esp-dir>/EFI/LINUX/BOOTX64.EFI
  - --build is intended for Linux hosts. On macOS, host-tool incompatibilities
    in upstream Linux build scripts usually block a clean bzImage build.
USAGE
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "[error] missing required command: $1" >&2
    exit 1
  }
}

while [ $# -gt 0 ]; do
  case "$1" in
    --esp-dir)
      ESP_DIR="${2:-}"
      shift 2
      ;;
    --linux-tree)
      LINUX_TREE="${2:-}"
      shift 2
      ;;
    --efi-input)
      EFI_INPUT="${2:-}"
      shift 2
      ;;
    --build)
      DO_BUILD=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "[error] unknown option: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [ -z "$LINUX_TREE" ] && [ -n "${LINUX_GUEST_TREE:-}" ]; then
  LINUX_TREE="${LINUX_GUEST_TREE}"
fi
if [ -z "$EFI_INPUT" ] && [ -n "${LINUX_GUEST_EFI_INPUT:-}" ]; then
  EFI_INPUT="${LINUX_GUEST_EFI_INPUT}"
fi

if [ "$DO_BUILD" -eq 1 ]; then
  if [ -z "$LINUX_TREE" ]; then
    echo "[error] --build requires --linux-tree" >&2
    exit 1
  fi
  if [ ! -d "$LINUX_TREE" ]; then
    echo "[error] Linux tree not found: $LINUX_TREE" >&2
    exit 1
  fi

  if [ "$(uname -s)" != "Linux" ]; then
    echo "[error] --build is only supported on Linux host for now." >&2
    echo "        On macOS, build Linux in a Linux VM/container and then run:" >&2
    echo "        make linux-guest-stage LINUX_GUEST_EFI_INPUT=/path/to/bzImage" >&2
    exit 2
  fi

  need_cmd make
  need_cmd sed
  need_cmd cp

  if [ -z "$JOBS" ]; then
    JOBS="$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)"
  fi

  mkdir -p "$KERNEL_OUT"
  make -C "$LINUX_TREE" O="$KERNEL_OUT" ARCH=x86_64 defconfig
  "$LINUX_TREE"/scripts/config --file "$KERNEL_OUT/.config" \
    --enable EFI \
    --enable EFI_STUB \
    --enable BLK_DEV_INITRD \
    --disable WERROR \
    --disable DEBUG_INFO_BTF
  make -C "$LINUX_TREE" O="$KERNEL_OUT" ARCH=x86_64 olddefconfig
  make -C "$LINUX_TREE" O="$KERNEL_OUT" ARCH=x86_64 -j"$JOBS" bzImage
  EFI_INPUT="$KERNEL_OUT/arch/x86/boot/bzImage"
fi

if [ -z "$EFI_INPUT" ]; then
  if [ -n "$LINUX_TREE" ]; then
    for candidate in \
      "$LINUX_TREE/arch/x86/boot/bzImage" \
      "$LINUX_TREE/arch/x86/boot/vmlinuz" \
      "$LINUX_TREE/arch/x86/boot/bzImage.efi"
    do
      if [ -f "$candidate" ]; then
        EFI_INPUT="$candidate"
        break
      fi
    done
  fi
fi

if [ -z "$EFI_INPUT" ]; then
  echo "[error] no Linux EFI candidate found." >&2
  echo "        Use --efi-input <file> or --build with a Linux host." >&2
  exit 1
fi

if [ ! -f "$EFI_INPUT" ]; then
  echo "[error] EFI input not found: $EFI_INPUT" >&2
  exit 1
fi

DEST_DIR="${ESP_DIR}/EFI/LINUX"
DEST_EFI="${DEST_DIR}/BOOTX64.EFI"
mkdir -p "$DEST_DIR"
cp "$EFI_INPUT" "$DEST_EFI"

echo "[ok] Linux guest staged:"
echo "     source: $EFI_INPUT"
echo "     target: $DEST_EFI"
