BUILD_DIR := build
ESP_DIR := $(BUILD_DIR)/esp
UEFI_TARGET := x86_64-unknown-uefi
KERNEL_MANIFEST := kernel/Cargo.toml
KERNEL_NAME := redux_kernel
UEFI_BIN := kernel/target/$(UEFI_TARGET)/debug/$(KERNEL_NAME).efi
BOOT_EFI := $(ESP_DIR)/EFI/BOOT/BOOTX64.EFI
DOOM_AUTO_SCRIPT := scripts/DOOMAUTO.NSH
DOOM_AUTO_EFI_PATH := $(ESP_DIR)/EFI/TOOLS/DOOMAUTO.NSH
GRUB_EFI_SRC := grub/grubx64.efi
GRUB_CFG_SRC := grub/grub-uefi.cfg
GRUB_DIR := $(ESP_DIR)/EFI/GRUB
GRUB_EFI_PATH := $(GRUB_DIR)/GRUBX64.EFI
GRUB_CFG_PATH := $(GRUB_DIR)/GRUB.CFG
LINUXRT_SRC := LINUXRT
LINUXRT_DST := $(ESP_DIR)/LINUXRT
LINUX_GUEST_TREE ?= /Users/mac/Downloads/linux-6.19.3
LINUX_GUEST_EFI_INPUT ?=
LINUX_GUEST_AUTO ?= 0
LINUX_GUEST_DST := $(ESP_DIR)/EFI/LINUX/BOOTX64.EFI
OPTIONAL_GRUB_INPUTS := $(wildcard $(GRUB_EFI_SRC) $(GRUB_CFG_SRC))
QEMU ?= qemu-system-x86_64
NVME_INSTALL_LABEL ?= REDUXEFI
RUST_SOURCES := $(shell find kernel/src -type f -name '*.rs')

all: uefi

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

$(UEFI_BIN): $(RUST_SOURCES) kernel/Cargo.toml kernel/.cargo/config.toml
	cargo build --manifest-path $(KERNEL_MANIFEST) --target $(UEFI_TARGET) --bin $(KERNEL_NAME)

$(BOOT_EFI): $(UEFI_BIN) $(DOOM_AUTO_SCRIPT) $(OPTIONAL_GRUB_INPUTS) | $(BUILD_DIR)
	mkdir -p $(ESP_DIR)/EFI/BOOT
	mkdir -p $(ESP_DIR)/EFI/TOOLS
	mkdir -p $(GRUB_DIR)
	cp $(UEFI_BIN) $(BOOT_EFI)
	cp $(DOOM_AUTO_SCRIPT) $(DOOM_AUTO_EFI_PATH)
	@if [ -f "$(GRUB_EFI_SRC)" ]; then cp "$(GRUB_EFI_SRC)" "$(GRUB_EFI_PATH)"; fi
	@if [ -f "$(GRUB_CFG_SRC)" ]; then cp "$(GRUB_CFG_SRC)" "$(GRUB_CFG_PATH)"; fi
	@if [ -d "$(LINUXRT_SRC)" ]; then \
		rm -rf "$(LINUXRT_DST)"; \
		cp -R "$(LINUXRT_SRC)" "$(LINUXRT_DST)"; \
	fi
	@if [ "$(LINUX_GUEST_AUTO)" = "1" ]; then \
		bash scripts/stage_linux_guest.sh --esp-dir "$(ESP_DIR)" --linux-tree "$(LINUX_GUEST_TREE)" $(if $(LINUX_GUEST_EFI_INPUT),--efi-input "$(LINUX_GUEST_EFI_INPUT)",); \
	fi

uefi: $(BOOT_EFI)

linux-guest-stage: | $(BUILD_DIR)
	bash scripts/stage_linux_guest.sh --esp-dir "$(ESP_DIR)" --linux-tree "$(LINUX_GUEST_TREE)" $(if $(LINUX_GUEST_EFI_INPUT),--efi-input "$(LINUX_GUEST_EFI_INPUT)",)

linux-guest-build: | $(BUILD_DIR)
	bash scripts/stage_linux_guest.sh --build --esp-dir "$(ESP_DIR)" --linux-tree "$(LINUX_GUEST_TREE)" $(if $(LINUX_GUEST_EFI_INPUT),--efi-input "$(LINUX_GUEST_EFI_INPUT)",)

run: uefi
	QEMU="$(QEMU)" bash scripts/run_uefi.sh "$(ESP_DIR)"

install-nvme: uefi
	@if [ -z "$(PARTITION)" ]; then \
		echo "Usage: make install-nvme PARTITION=/dev/nvme0n1pX [NVME_INSTALL_LABEL=REDUXEFI]"; \
		exit 1; \
	fi
	bash scripts/install_nvme.sh --partition "$(PARTITION)" --efi-source "$(BOOT_EFI)" --linuxrt-source "$(LINUXRT_DST)" --label "$(NVME_INSTALL_LABEL)"

newlib-help:
	@bash scripts/newlib_port.sh

newlib-scaffold:
	@if [ -z "$(APP)" ]; then \
		echo "Usage: make newlib-scaffold APP=<app_name> [DEST=<path>]"; \
		exit 1; \
	fi
	@bash scripts/newlib_port.sh scaffold "$(APP)" "$(if $(DEST),$(DEST),)"

newlib-build:
	@if [ -z "$(SRC)" ]; then \
		echo "Usage: make newlib-build SRC=<main.cpp> [OUT=build/newlib_cpp/APP.BIN]"; \
		exit 1; \
	fi
	@bash scripts/newlib_port.sh build "$(SRC)" "$(if $(OUT),$(OUT),)"

newlib-doctor:
	@if [ -z "$(ELF)" ]; then \
		echo "Usage: make newlib-doctor ELF=<file.elf>"; \
		exit 1; \
	fi
	@bash scripts/newlib_port.sh doctor "$(ELF)"

wry-host:
	@bash scripts/run_wry_host_bridge.sh "$(if $(BIND),$(BIND),127.0.0.1:37810)" "$(if $(URL),$(URL),https://www.google.com)"

clean:
	rm -rf $(BUILD_DIR)
	cargo clean --manifest-path $(KERNEL_MANIFEST)
	cargo clean --manifest-path sdk/reduxlang/Cargo.toml

.PHONY: all uefi linux-guest-stage linux-guest-build run install-nvme newlib-help newlib-scaffold newlib-build newlib-doctor wry-host clean
