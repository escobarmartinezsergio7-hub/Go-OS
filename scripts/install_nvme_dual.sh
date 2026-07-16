#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

DISK=""
BOOT_SIZE_MIB=16384
EFI_SOURCE="${ROOT_DIR}/build/esp/EFI/BOOT/BOOTX64.EFI"
LINUXRT_SOURCE="${ROOT_DIR}/build/esp/LINUXRT"
LINUX_GUEST_SOURCE="${ROOT_DIR}/build/esp/EFI/LINUX"
LABEL="ZENOX OS"
DATA_LABEL="ZENOX DATA"
ASSUME_YES=0
DRY_RUN=0

usage() {
  cat <<USAGE
Usage:
  bash scripts/install_nvme_dual.sh --disk /dev/nvme0n1 [options]

Creates a two-partition internal layout:
  p1: FAT32 EFI/boot/system partition
  p2: exFAT large-data partition

Options:
  --disk             Internal NVMe disk to erase and partition (required)
  --boot-size-mib   FAT32 boot/system partition size in MiB (default: 16384)
  --efi-source      Path to BOOTX64.EFI (default: build/esp/EFI/BOOT/BOOTX64.EFI)
  --linuxrt-source  Path to local LINUXRT folder (default: build/esp/LINUXRT)
  --linux-guest-source Path to local EFI/LINUX folder (default: build/esp/EFI/LINUX)
  --label           FAT32 label (default: ZENOX OS)
  --data-label      exFAT data label (default: ZENOX DATA)
  --yes, -y         Skip interactive confirmation
  --dry-run         Print actions without changing disks
  --help, -h        Show this help

Notes:
  - Linux only.
  - Requires parted, lsblk, findmnt, mkfs.fat, and mkfs.exfat/mkexfatfs.
  - This operation destroys all data on the selected disk.
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

while [ $# -gt 0 ]; do
  case "$1" in
    --disk)
      DISK="${2:-}"
      shift 2
      ;;
    --boot-size-mib)
      BOOT_SIZE_MIB="${2:-}"
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
    --data-label)
      DATA_LABEL="${2:-}"
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

if [ "$(uname -s)" != "Linux" ]; then
  echo "[error] This installer currently supports Linux only." >&2
  exit 1
fi

need_cmd lsblk
need_cmd findmnt
need_cmd parted
need_cmd sync

if [ -z "$DISK" ]; then
  echo "[error] --disk is required." >&2
  usage
  exit 1
fi

if [[ ! "$DISK" =~ ^/dev/nvme[0-9]+n[0-9]+$ ]]; then
  echo "[error] Refusing non-NVMe disk path: $DISK" >&2
  echo "        Expected format: /dev/nvme0n1" >&2
  exit 1
fi

if ! [[ "$BOOT_SIZE_MIB" =~ ^[0-9]+$ ]] || [ "$BOOT_SIZE_MIB" -lt 256 ]; then
  echo "[error] --boot-size-mib must be an integer >= 256." >&2
  exit 1
fi

if [ ! -b "$DISK" ]; then
  echo "[error] Not a block device: $DISK" >&2
  exit 1
fi

disk_type="$(lsblk -ndo TYPE "$DISK" 2>/dev/null | head -n1 | tr -d '[:space:]')"
if [ "$disk_type" != "disk" ]; then
  echo "[error] Target is not a whole disk: $DISK" >&2
  exit 1
fi

rm_flag="$(lsblk -ndo RM "$DISK" 2>/dev/null | head -n1 | tr -d '[:space:]')"
if [ "$rm_flag" = "1" ]; then
  echo "[error] Refusing removable media. Expected internal NVMe disk." >&2
  exit 1
fi

root_source="$(findmnt -n -o SOURCE / 2>/dev/null || true)"
if [ -n "$root_source" ]; then
  root_parent="$(lsblk -no PKNAME "$root_source" 2>/dev/null | head -n1 | tr -d '[:space:]' || true)"
  if [ "$root_source" = "$DISK" ] || [ "/dev/$root_parent" = "$DISK" ]; then
    echo "[error] Refusing to repartition the disk that contains /: $DISK" >&2
    exit 1
  fi
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

BOOT_PART="${DISK}p1"
DATA_PART="${DISK}p2"
BOOT_END_MIB=$((BOOT_SIZE_MIB + 1))

echo "Target disk      : $DISK"
echo "Boot partition   : $BOOT_PART (${BOOT_SIZE_MIB} MiB, FAT32, $LABEL)"
echo "Data partition   : $DATA_PART (remaining space, exFAT, $DATA_LABEL)"
echo "EFI source       : $EFI_SOURCE"
echo "LINUXRT source   : $LINUXRT_SOURCE"
echo
echo "WARNING: this will ERASE ALL DATA on $DISK"

if [ "$ASSUME_YES" -ne 1 ]; then
  confirm_phrase="ERASE-DISK $DISK"
  read -r -p "Type exactly '$confirm_phrase' to continue: " confirm
  if [ "$confirm" != "$confirm_phrase" ]; then
    echo "Aborted."
    exit 1
  fi
fi

echo "Creating GPT dual-partition layout ..."
as_root parted -s "$DISK" mklabel gpt
as_root parted -s "$DISK" mkpart "$LABEL" fat32 1MiB "${BOOT_END_MIB}MiB"
as_root parted -s "$DISK" set 1 esp on
as_root parted -s "$DISK" mkpart "$DATA_LABEL"  exfat "${BOOT_END_MIB}MiB" 100%

if command -v partprobe >/dev/null 2>&1; then
  as_root partprobe "$DISK" || true
fi
if command -v udevadm >/dev/null 2>&1; then
  run_cmd udevadm settle || true
fi

run_cmd sync

echo "Installing Zenox OS onto dual layout ..."
cmd=(
  bash "${SCRIPT_DIR}/install_nvme.sh"
  --partition "$BOOT_PART"
  --data-partition "$DATA_PART"
  --efi-source "$EFI_SOURCE"
  --linuxrt-source "$LINUXRT_SOURCE"
  --linux-guest-source "$LINUX_GUEST_SOURCE"
  --label "$LABEL"
  --data-label "$DATA_LABEL"
)
if [ "$ASSUME_YES" -eq 1 ]; then
  cmd+=(--yes)
fi
if [ "$DRY_RUN" -eq 1 ]; then
  cmd+=(--dry-run)
fi

run_cmd "${cmd[@]}"
