#!/usr/bin/env bash
set -euo pipefail

ESP_DIR="${1:-build/esp}"
QEMU_BIN="${QEMU:-qemu-system-x86_64}"
BUILD_DIR="$(cd "${ESP_DIR}"/.. && pwd)"
ESP_IMG="${BUILD_DIR}/esp.img"
NVME_IMG="${BUILD_DIR}/disk_nvme.img"
ESP_IMG_SIZE_MB="${ESP_IMG_SIZE_MB:-64}"
NVME_IMG_SIZE_MB="${NVME_IMG_SIZE_MB:-1024}"

ensure_raw_image() {
  local path="$1"
  local size_mb="$2"

  if [ -f "${path}" ]; then
    return 0
  fi

  echo "[info] Creating disk image: ${path} (${size_mb} MiB)"
  mkdir -p "$(dirname "${path}")"

  if command -v truncate >/dev/null 2>&1; then
    truncate -s "${size_mb}M" "${path}"
  else
    dd if=/dev/zero of="${path}" bs=1m count="${size_mb}" status=none
  fi
}

find_first() {
  for p in "$@"; do
    if [ -f "$p" ]; then
      echo "$p"
      return 0
    fi
  done
  return 1
}

BREW_QEMU_PREFIX=""
if command -v brew >/dev/null 2>&1; then
  BREW_QEMU_PREFIX="$(brew --prefix qemu 2>/dev/null || true)"
fi

OVMF_CODE="$(find_first \
  /usr/share/OVMF/OVMF_CODE.fd \
  /usr/share/OVMF/OVMF_CODE_4M.fd \
  /usr/share/edk2/ovmf/OVMF_CODE.fd \
  /usr/share/edk2/x64/OVMF_CODE.fd \
  /opt/homebrew/share/qemu/edk2-x86_64-code.fd \
  /opt/homebrew/share/qemu/edk2-x86_64-secure-code.fd \
  /usr/local/share/qemu/edk2-x86_64-code.fd \
  /usr/local/share/qemu/edk2-x86_64-secure-code.fd \
  ${BREW_QEMU_PREFIX:+${BREW_QEMU_PREFIX}/share/qemu/edk2-x86_64-code.fd} \
  ${BREW_QEMU_PREFIX:+${BREW_QEMU_PREFIX}/share/qemu/edk2-x86_64-secure-code.fd} \
  2>/dev/null || true)"

OVMF_VARS_TEMPLATE="$(find_first \
  /usr/share/OVMF/OVMF_VARS.fd \
  /usr/share/OVMF/OVMF_VARS_4M.fd \
  /usr/share/edk2/ovmf/OVMF_VARS.fd \
  /usr/share/edk2/x64/OVMF_VARS.fd \
  /opt/homebrew/share/qemu/edk2-x86_64-vars.fd \
  /opt/homebrew/share/qemu/edk2-i386-vars.fd \
  /usr/local/share/qemu/edk2-x86_64-vars.fd \
  /usr/local/share/qemu/edk2-i386-vars.fd \
  ${BREW_QEMU_PREFIX:+${BREW_QEMU_PREFIX}/share/qemu/edk2-x86_64-vars.fd} \
  ${BREW_QEMU_PREFIX:+${BREW_QEMU_PREFIX}/share/qemu/edk2-i386-vars.fd} \
  2>/dev/null || true)"

if [ -z "${OVMF_CODE}" ] || [ -z "${OVMF_VARS_TEMPLATE}" ]; then
  echo "[error] OVMF firmware not found."
  echo "Install OVMF (Linux) or QEMU with edk2 firmware (macOS Homebrew)."
  exit 1
fi

VARS_FILE="${BUILD_DIR}/OVMF_VARS.fd"
if [ ! -f "${VARS_FILE}" ]; then
  cp "${OVMF_VARS_TEMPLATE}" "${VARS_FILE}"
fi

ensure_raw_image "${ESP_IMG}" "${ESP_IMG_SIZE_MB}"
ensure_raw_image "${NVME_IMG}" "${NVME_IMG_SIZE_MB}"

# Create startup.nsh to force boot
mkdir -p "${ESP_DIR}"
echo "\EFI\BOOT\BOOTX64.EFI" > "${ESP_DIR}/startup.nsh"

"${QEMU_BIN}" \
  -machine q35 \
  -m 1024 \
  -drive "if=pflash,format=raw,readonly=on,file=${OVMF_CODE}" \
  -drive "if=pflash,format=raw,file=${VARS_FILE}" \
  -device virtio-blk-pci,drive=hd0,disable-modern=on,disable-legacy=off \
  -drive "if=none,id=hd0,format=raw,file=${ESP_IMG}" \
  -drive "format=raw,file=fat:rw:${ESP_DIR}" \
  -device virtio-net-pci,netdev=net0,disable-modern=on,disable-legacy=off \
  -netdev "user,id=net0" \
  -device virtio-keyboard-pci,disable-modern=on,disable-legacy=off \
  -device nvme,drive=nvme0,serial=1234,physical_block_size=4096,logical_block_size=4096 \
  -drive "if=none,id=nvme0,format=raw,file=${NVME_IMG}" \
  -device qemu-xhci,id=xhci \
  -device usb-tablet,bus=xhci.0 \
  -device usb-mouse,bus=xhci.0 \
  -device intel-hda -device hda-duplex \
  -serial stdio
