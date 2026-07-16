#!/usr/bin/env bash
# build_iso.sh — Genera una imagen ISO UEFI booteable a partir del ESP de build.
#
# La ISO resultante contiene una EFI System Partition (FAT32) embebida con la
# estructura estándar EFI/BOOT/BOOTX64.EFI, compatible con arranque UEFI en
# múltiples dispositivos (laptops, PCs, servidores, VMs).
#
# Uso: bash scripts/build_iso.sh [build/esp] [build/goos.iso]
set -euo pipefail

ESP_DIR="${1:-build/esp}"
ISO_OUT="${2:-build/goos.iso}"

if [ ! -f "$ESP_DIR/EFI/BOOT/BOOTX64.EFI" ]; then
  echo "ERROR: $ESP_DIR/EFI/BOOT/BOOTX64.EFI not found. Run 'make uefi' first."
  exit 1
fi

# ---------------------------------------------------------------------------
# Detect tools
# ---------------------------------------------------------------------------
ISO_TOOL=""
if command -v xorriso &>/dev/null; then
  ISO_TOOL="xorriso"
elif command -v mkisofs &>/dev/null; then
  ISO_TOOL="mkisofs"
elif command -v genisoimage &>/dev/null; then
  ISO_TOOL="genisoimage"
else
  echo "ERROR: Need xorriso, mkisofs, or genisoimage to create the ISO."
  echo "  macOS:  brew install xorriso"
  echo "  Linux:  sudo apt install xorriso   (or genisoimage)"
  exit 1
fi

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

# ---------------------------------------------------------------------------
# 1. Build the EFI System Partition FAT image (efiboot.img)
#    This image is what UEFI firmware actually reads from the ISO.
#    It MUST contain EFI/BOOT/BOOTX64.EFI at minimum for the UEFI default
#    boot path (removable media), which guarantees multi-device compatibility.
# ---------------------------------------------------------------------------
echo ">> Creating EFI System Partition image (FAT)..."

ESP_SIZE_KB=$(du -sk "$ESP_DIR" | awk '{print $1}')
# Pad to at least 4 MiB and add 128 MiB margin for FAT metadata + 20% overhead
FAT_SIZE_KB=$(( ESP_SIZE_KB + ESP_SIZE_KB / 5 + 131072 ))
if [ "$FAT_SIZE_KB" -lt 4096 ]; then
  FAT_SIZE_KB=4096
fi

FAT_IMG="$WORK_DIR/efiboot.img"

if command -v mkfs.fat &>/dev/null; then
  # Linux path — direct loop mount
  dd if=/dev/zero of="$FAT_IMG" bs=1024 count="$FAT_SIZE_KB" 2>/dev/null
  mkfs.fat -F 32 -n "EFISYS" "$FAT_IMG" >/dev/null
  MNT="$WORK_DIR/mnt"
  mkdir -p "$MNT"
  sudo mount -o loop "$FAT_IMG" "$MNT"
  cp -r "$ESP_DIR"/* "$MNT"/
  sudo umount "$MNT"
elif command -v hdiutil &>/dev/null; then
  # macOS path — hdiutil + raw conversion
  FAT_SIZE_MB=$(( FAT_SIZE_KB / 1024 + 1 ))
  hdiutil create -size "${FAT_SIZE_MB}m" -fs "MS-DOS FAT32" -layout NONE \
    -volname "EFISYS" "$WORK_DIR/efiboot.dmg" >/dev/null 2>&1
  MOUNT_OUT=$(hdiutil attach "$WORK_DIR/efiboot.dmg" -nobrowse 2>/dev/null)
  MOUNT_POINT=$(echo "$MOUNT_OUT" | grep -o '/Volumes/.*' | head -1)
  if [ -z "$MOUNT_POINT" ]; then
    echo "ERROR: Could not mount FAT image."
    exit 1
  fi
  cp -r "$ESP_DIR"/* "$MOUNT_POINT"/
  sync
  hdiutil detach "$MOUNT_POINT" >/dev/null 2>&1
  # Convert to raw disk image
  hdiutil convert "$WORK_DIR/efiboot.dmg" -format UDTO -o "$WORK_DIR/efiboot" >/dev/null 2>&1
  mv "$WORK_DIR/efiboot.cdr" "$FAT_IMG"
else
  echo "ERROR: Need mkfs.fat (Linux) or hdiutil (macOS) to create FAT image."
  exit 1
fi

echo "   EFI image: $(du -h "$FAT_IMG" | awk '{print $1}')"

# ---------------------------------------------------------------------------
# 2. Build ISO9660 image with embedded EFI boot partition
#
#    Key UEFI boot flags:
#    - The FAT image is registered as El Torito "EFI System Partition" entry
#    - -no-emul-boot tells firmware to treat the image as a raw disk
#    - -isohybrid-gpt-basdat (xorriso) marks the partition as EFI in a GPT
#      protective MBR, enabling direct dd-to-USB boot on UEFI systems
#    - The ISO also contains the files directly for inspection/extraction
# ---------------------------------------------------------------------------
echo ">> Building ISO with EFI boot partition..."

# Stage ISO filesystem — include ESP contents + efiboot.img
ISO_STAGE="$WORK_DIR/iso"
mkdir -p "$ISO_STAGE"
cp -r "$ESP_DIR"/* "$ISO_STAGE"/
cp "$FAT_IMG" "$ISO_STAGE/efiboot.img"

mkdir -p "$(dirname "$ISO_OUT")"

if [ "$ISO_TOOL" = "xorriso" ]; then
  xorriso -as mkisofs \
    -o "$ISO_OUT" \
    -iso-level 3 \
    -J -joliet-long \
    -V "ZENOXOS" \
    -A "Zenox OS UEFI Boot Disc" \
    -eltorito-alt-boot \
    -e efiboot.img \
    -no-emul-boot \
    -isohybrid-gpt-basdat \
    "$ISO_STAGE" 2>/dev/null
else
  # mkisofs / genisoimage
  $ISO_TOOL \
    -o "$ISO_OUT" \
    -iso-level 3 \
    -J -joliet-long \
    -V "ZENOXOS" \
    -A "Zenox OS UEFI Boot Disc" \
    -eltorito-alt-boot \
    -e efiboot.img \
    -no-emul-boot \
    "$ISO_STAGE" 2>/dev/null
fi

echo ""
echo "========================================="
echo "  ISO UEFI booteable creada con éxito"
echo "  Archivo: $ISO_OUT"
echo "  Tamaño:  $(du -h "$ISO_OUT" | awk '{print $1}')"
echo "========================================="
echo ""
echo "Contenido de la EFI System Partition embebida:"
echo "  EFI/BOOT/BOOTX64.EFI  (arranque UEFI estándar)"
if [ -d "$ESP_DIR/LINUXRT" ]; then
  echo "  LINUXRT/               (Linux runtime)"
fi
if [ -d "$ESP_DIR/SERVORT" ]; then
  echo "  SERVORT/               (Servo runtime)"
fi
echo ""
echo "Opciones para grabar:"
echo "  Rufus (Windows):  Selecciona la ISO > GPT > UEFI (no CSM) > Empezar"
echo "  Etcher:           Selecciona la ISO > Selecciona USB > Flash"
echo "  dd:               sudo dd if=$ISO_OUT of=/dev/sdX bs=4M status=progress && sync"
