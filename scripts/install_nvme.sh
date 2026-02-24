#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

PARTITION=""
EFI_SOURCE="${ROOT_DIR}/build/esp/EFI/BOOT/BOOTX64.EFI"
LINUXRT_SOURCE="${ROOT_DIR}/build/esp/LINUXRT"
LINUX_GUEST_SOURCE="${ROOT_DIR}/build/esp/EFI/LINUX"
LABEL="REDUXEFI"
MOUNT_DIR=""
ASSUME_YES=0
DRY_RUN=0
TEMP_MOUNT_DIR=0
MOUNTED_BY_SCRIPT=0
LINUXRT_INSTALLED=0
LINUX_GUEST_INSTALLED=0

usage() {
  cat <<USAGE
Usage:
  bash scripts/install_nvme.sh --partition /dev/nvme0n1pX [options]

Options:
  --partition, -p   NVMe partition to erase and install to (required)
  --efi-source      Path to BOOTX64.EFI (default: build/esp/EFI/BOOT/BOOTX64.EFI)
  --linuxrt-source  Path to local LINUXRT folder (default: build/esp/LINUXRT)
  --linux-guest-source Path to local EFI/LINUX folder (default: build/esp/EFI/LINUX)
  --label           FAT32 label (default: REDUXEFI)
  --mount-point     Mount directory (default: auto temp dir)
  --yes, -y         Skip interactive confirmation
  --dry-run         Print actions without changing disks
  --help, -h        Show this help

Notes:
  - Linux only (uses lsblk/findmnt/mount/mkfs.fat).
  - The target must be an existing internal NVMe partition.
  - This operation destroys all data in the selected partition.
USAGE
}

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[error] Missing required command: $1" >&2
    exit 1
  fi
}

run_cmd() {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '+ '
    printf '%q ' "$@"
    printf '\n'
    return 0
  fi
  "$@"
}

as_root() {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '+ '
    if [ "${EUID:-$(id -u)}" -ne 0 ]; then
      printf '%q ' sudo
    fi
    printf '%q ' "$@"
    printf '\n'
    return 0
  fi

  if [ "${EUID:-$(id -u)}" -eq 0 ]; then
    "$@"
  else
    sudo "$@"
  fi
}

cleanup() {
  if [ "$MOUNTED_BY_SCRIPT" -eq 1 ] && [ -n "$MOUNT_DIR" ]; then
    as_root umount "$MOUNT_DIR" || true
  fi

  if [ "$TEMP_MOUNT_DIR" -eq 1 ] && [ -n "$MOUNT_DIR" ] && [ -d "$MOUNT_DIR" ]; then
    run_cmd rmdir "$MOUNT_DIR" || true
  fi
}
trap cleanup EXIT

while [ $# -gt 0 ]; do
  case "$1" in
    --partition|-p)
      PARTITION="${2:-}"
      shift 2
      ;;
    --efi-source)
      EFI_SOURCE="${2:-}"
      shift 2
      ;;
    --linuxrt-source)
      LINUXRT_SOURCE="${2:-}"
      shift 2
      ;;
    --linux-guest-source)
      LINUX_GUEST_SOURCE="${2:-}"
      shift 2
      ;;
    --label)
      LABEL="${2:-}"
      shift 2
      ;;
    --mount-point)
      MOUNT_DIR="${2:-}"
      shift 2
      ;;
    --yes|-y)
      ASSUME_YES=1
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "[error] Unknown option: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [ -z "$PARTITION" ]; then
  echo "[error] --partition is required." >&2
  usage
  exit 1
fi

if [ "$(uname -s)" != "Linux" ]; then
  echo "[error] This installer currently supports Linux only." >&2
  exit 1
fi

need_cmd lsblk
need_cmd findmnt
need_cmd mount
need_cmd umount
need_cmd sync

if command -v mkfs.fat >/dev/null 2>&1; then
  MKFS_TOOL="mkfs.fat"
elif command -v mkfs.vfat >/dev/null 2>&1; then
  MKFS_TOOL="mkfs.vfat"
else
  echo "[error] Missing mkfs.fat/mkfs.vfat. Install dosfstools." >&2
  exit 1
fi

if [ ! -f "$EFI_SOURCE" ]; then
  echo "[error] EFI file not found: $EFI_SOURCE" >&2
  echo "Build first: make uefi" >&2
  exit 1
fi

if [ ! -d "$LINUXRT_SOURCE" ]; then
  echo "[error] LINUXRT source not found: $LINUXRT_SOURCE" >&2
  echo "        Build first: make uefi (must generate build/esp/LINUXRT)" >&2
  exit 1
fi

if [[ ! "$PARTITION" =~ ^/dev/nvme[0-9]+n[0-9]+p[0-9]+$ ]]; then
  echo "[error] Refusing non-NVMe partition path: $PARTITION" >&2
  echo "        Expected format: /dev/nvme0n1p4" >&2
  exit 1
fi

if [ ! -b "$PARTITION" ]; then
  echo "[error] Not a block device: $PARTITION" >&2
  exit 1
fi

part_type="$(lsblk -no TYPE "$PARTITION" 2>/dev/null | head -n1 | tr -d '[:space:]')"
if [ "$part_type" != "part" ]; then
  echo "[error] Target is not a partition: $PARTITION" >&2
  exit 1
fi

parent_disk="$(lsblk -no PKNAME "$PARTITION" 2>/dev/null | head -n1 | tr -d '[:space:]')"
if [ -z "$parent_disk" ] || [[ ! "$parent_disk" =~ ^nvme[0-9]+n[0-9]+$ ]]; then
  echo "[error] Could not validate NVMe parent disk for: $PARTITION" >&2
  exit 1
fi

rm_flag="$(lsblk -ndo RM "/dev/${parent_disk}" 2>/dev/null | head -n1 | tr -d '[:space:]')"
if [ "$rm_flag" = "1" ]; then
  echo "[error] Refusing removable NVMe media. Expected internal disk." >&2
  exit 1
fi

root_source="$(findmnt -n -o SOURCE / 2>/dev/null || true)"
if [ -n "$root_source" ] && [ "$root_source" = "$PARTITION" ]; then
  echo "[error] Refusing to format currently mounted root partition: $PARTITION" >&2
  exit 1
fi

mounted_targets="$(findmnt -rn -o TARGET --source "$PARTITION" 2>/dev/null || true)"
if [ -n "$mounted_targets" ]; then
  while IFS= read -r target; do
    case "$target" in
      /|/boot|/boot/efi)
        echo "[error] Refusing to touch active boot/system mount: $target" >&2
        exit 1
        ;;
    esac
  done <<< "$mounted_targets"
fi

echo "Target partition : $PARTITION"
echo "Parent NVMe disk : /dev/${parent_disk}"
echo "EFI source       : $EFI_SOURCE"
echo "LINUXRT source   : $LINUXRT_SOURCE"
if [ -d "$LINUX_GUEST_SOURCE" ]; then
  echo "Linux guest src  : $LINUX_GUEST_SOURCE"
else
  echo "Linux guest src  : (not found, skipping)"
fi
echo "FAT32 label      : $LABEL"
echo
echo "WARNING: this will ERASE all data on $PARTITION"

if [ "$ASSUME_YES" -ne 1 ]; then
  read -r -p "Type exactly 'ERASE $PARTITION' to continue: " confirm
  if [ "$confirm" != "ERASE $PARTITION" ]; then
    echo "Aborted."
    exit 1
  fi
fi

if [ -n "$mounted_targets" ]; then
  echo "Unmounting existing mounts for $PARTITION ..."
  as_root umount "$PARTITION"
fi

if [ -z "$MOUNT_DIR" ]; then
  MOUNT_DIR="$(mktemp -d /tmp/reduxos-installer.XXXXXX)"
  TEMP_MOUNT_DIR=1
else
  run_cmd mkdir -p "$MOUNT_DIR"
fi

echo "Formatting $PARTITION as FAT32 ..."
as_root "$MKFS_TOOL" -F 32 -n "$LABEL" "$PARTITION"

echo "Mounting partition to $MOUNT_DIR ..."
as_root mount "$PARTITION" "$MOUNT_DIR"
MOUNTED_BY_SCRIPT=1

echo "Installing EFI payload ..."
as_root mkdir -p "$MOUNT_DIR/EFI/BOOT"
as_root cp "$EFI_SOURCE" "$MOUNT_DIR/EFI/BOOT/BOOTX64.EFI"

if [ "$DRY_RUN" -eq 1 ]; then
  echo "+ write startup.nsh -> $MOUNT_DIR/startup.nsh"
else
  printf '\\EFI\\BOOT\\BOOTX64.EFI\n' | as_root tee "$MOUNT_DIR/startup.nsh" >/dev/null
fi

if [ "$DRY_RUN" -eq 1 ]; then
  echo "+ write REDUXOS.INI -> $MOUNT_DIR/REDUXOS.INI"
else
  cat <<'EOF' | as_root tee "$MOUNT_DIR/REDUXOS.INI" >/dev/null
[reduxos]
installed=1
autoboot=gui
boot_efi=\EFI\BOOT\BOOTX64.EFI
EOF
fi

if [ "$DRY_RUN" -eq 1 ]; then
  echo "+ write README.TXT -> $MOUNT_DIR/README.TXT"
else
  cat <<'EOF' | as_root tee "$MOUNT_DIR/README.TXT" >/dev/null
ReduxOS installed on internal NVMe partition.
Boot file: \EFI\BOOT\BOOTX64.EFI
EOF
fi

echo "Installing LINUXRT payload ..."
as_root mkdir -p "$MOUNT_DIR/LINUXRT"
if [ "$DRY_RUN" -eq 1 ]; then
  echo "+ cp -a $LINUXRT_SOURCE/. $MOUNT_DIR/LINUXRT/"
else
  as_root cp -a "$LINUXRT_SOURCE/." "$MOUNT_DIR/LINUXRT/"
fi
LINUXRT_INSTALLED=1

if [ -d "$LINUX_GUEST_SOURCE" ]; then
  echo "Installing Linux guest EFI payload ..."
  as_root mkdir -p "$MOUNT_DIR/EFI/LINUX"
  if [ "$DRY_RUN" -eq 1 ]; then
    echo "+ cp -a $LINUX_GUEST_SOURCE/. $MOUNT_DIR/EFI/LINUX/"
  else
    as_root cp -a "$LINUX_GUEST_SOURCE/." "$MOUNT_DIR/EFI/LINUX/"
  fi
  LINUX_GUEST_INSTALLED=1
fi

as_root sync
as_root umount "$MOUNT_DIR"
MOUNTED_BY_SCRIPT=0

if [ "$TEMP_MOUNT_DIR" -eq 1 ]; then
  run_cmd rmdir "$MOUNT_DIR"
fi

echo "Install complete."
echo "UEFI file installed at: $PARTITION:/EFI/BOOT/BOOTX64.EFI"
if [ "$LINUXRT_INSTALLED" -eq 1 ]; then
  echo "LINUXRT installed from: $LINUXRT_SOURCE"
fi
if [ "$LINUX_GUEST_INSTALLED" -eq 1 ]; then
  echo "Linux guest EFI installed from: $LINUX_GUEST_SOURCE"
fi
