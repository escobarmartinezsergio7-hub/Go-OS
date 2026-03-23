BUILD_DIR := build
ESP_DIR := $(BUILD_DIR)/esp
UEFI_TARGET := x86_64-unknown-uefi
KERNEL_MANIFEST := kernel/Cargo.toml
KERNEL_NAME := redux_kernel
UEFI_BIN := kernel/target/$(UEFI_TARGET)/debug/$(KERNEL_NAME).efi
BOOT_EFI := $(ESP_DIR)/EFI/BOOT/BOOTX64.EFI
GRUB_EFI_SRC := grub/grubx64.efi
GRUB_CFG_SRC := grub/grub-uefi.cfg
GRUB_DIR := $(ESP_DIR)/EFI/GRUB
GRUB_EFI_PATH := $(GRUB_DIR)/GRUBX64.EFI
GRUB_CFG_PATH := $(GRUB_DIR)/GRUB.CFG
LINUXRT_SRC := LINUXRT
LINUXRT_DST := $(ESP_DIR)/LINUXRT
SERVORT_SRC := SERVORT
SERVORT_DST := $(ESP_DIR)/SERVORT
LINUX_GUEST_TREE ?= /Users/mac/Downloads/linux-6.19.3
LINUX_GUEST_EFI_INPUT ?=
LINUX_GUEST_AUTO ?= 0
LINUX_GUEST_DST := $(ESP_DIR)/EFI/LINUX/BOOTX64.EFI
OPTIONAL_GRUB_INPUTS := $(wildcard $(GRUB_EFI_SRC) $(GRUB_CFG_SRC))
QEMU ?= qemu-system-x86_64
NVME_INSTALL_LABEL ?= REDUXEFI
RUST_SOURCES := $(shell find kernel/src -type f -name '*.rs')

# USB deploy paths (data partition + real EFI System Partition)
USB_DATA_VOL ?= /Volumes/GOOS
USB_EFI_VOL  ?= /Volumes/EFI
USB_EFI_DISK ?= disk6s1

all: uefi

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

$(UEFI_BIN): $(RUST_SOURCES) kernel/Cargo.toml kernel/.cargo/config.toml
	cargo build --manifest-path $(KERNEL_MANIFEST) --target $(UEFI_TARGET) --bin $(KERNEL_NAME)

$(BOOT_EFI): $(UEFI_BIN) $(OPTIONAL_GRUB_INPUTS) | $(BUILD_DIR)
	mkdir -p $(ESP_DIR)/EFI/BOOT
	mkdir -p $(ESP_DIR)/EFI/TOOLS
	mkdir -p $(GRUB_DIR)
	rm -f $(ESP_DIR)/EFI/TOOLS/DOOMAUTO.NSH
	cp $(UEFI_BIN) $(BOOT_EFI)
	@if [ -f "$(GRUB_EFI_SRC)" ]; then cp "$(GRUB_EFI_SRC)" "$(GRUB_EFI_PATH)"; fi
	@if [ -f "$(GRUB_CFG_SRC)" ]; then cp "$(GRUB_CFG_SRC)" "$(GRUB_CFG_PATH)"; fi
	@if [ -d "$(LINUXRT_SRC)" ]; then \
		rm -rf "$(LINUXRT_DST)"; \
		cp -R "$(LINUXRT_SRC)" "$(LINUXRT_DST)"; \
	fi
	@if [ -d "$(SERVORT_SRC)" ]; then \
		rm -rf "$(SERVORT_DST)"; \
		cp -R "$(SERVORT_SRC)" "$(SERVORT_DST)"; \
	fi
	@if [ "$(LINUX_GUEST_AUTO)" = "1" ]; then \
		bash scripts/stage_linux_guest.sh --esp-dir "$(ESP_DIR)" --linux-tree "$(LINUX_GUEST_TREE)" $(if $(LINUX_GUEST_EFI_INPUT),--efi-input "$(LINUX_GUEST_EFI_INPUT)",); \
	fi

uefi: $(BOOT_EFI)

litehtml-sync:
	@bash scripts/sync_litehtml_upstream.sh

litehtml-bridge-build:
	@bash scripts/build_litehtml_bridge.sh

servo-adapter-build:
	@bash scripts/build_libsimpleservo_adapter.sh

servort-stage:
	@bash scripts/stage_servort_runtime.sh $(if $(SERVO_BIN),--bin "$(SERVO_BIN)",--servo-root "$(if $(SERVO_ROOT),$(SERVO_ROOT),/Users/mac/Desktop/servo)") --dest-root "$(if $(SERVORT_DIR),$(SERVORT_DIR),$(SERVORT_SRC))" $(if $(SERVORT_NAME),--dest-name "$(SERVORT_NAME)")

servort-stage-esp:
	@bash scripts/stage_servort_runtime.sh $(if $(SERVO_BIN),--bin "$(SERVO_BIN)",--servo-root "$(if $(SERVO_ROOT),$(SERVO_ROOT),/Users/mac/Desktop/servo)") --dest-root "$(if $(SERVORT_DIR),$(SERVORT_DIR),$(SERVORT_SRC))" $(if $(SERVORT_NAME),--dest-name "$(SERVORT_NAME)") --esp-dir "$(if $(ESP_STAGE_DIR),$(ESP_STAGE_DIR),$(ESP_DIR))"

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

servo-host:
	@bash scripts/run_servo_host_bridge.sh "$(if $(BIND),$(BIND),127.0.0.1:37810)" "$(if $(URL),$(URL),https://www.google.com)"

ide:
	@HOST="$(if $(HOST),$(HOST),127.0.0.1)" \
	 PORT="$(if $(PORT),$(PORT),37999)" \
	 PROJECT="$(if $(PROJECT),$(PROJECT),my_app)" \
	 WORKSPACE="$(if $(WORKSPACE),$(WORKSPACE),$(PWD)/build/ide_workspace)" \
	 bash scripts/run_redux_ide.sh

# Deploy to USB data partition (GOOS)
deploy-data: uefi
	@if [ -d "$(USB_DATA_VOL)/EFI/BOOT" ]; then \
		cp $(BOOT_EFI) "$(USB_DATA_VOL)/EFI/BOOT/BOOTX64.EFI"; \
		echo "Deployed to $(USB_DATA_VOL)/EFI/BOOT/BOOTX64.EFI"; \
	else \
		echo "ERROR: $(USB_DATA_VOL)/EFI/BOOT not found. Is the USB mounted?"; \
		exit 1; \
	fi

# Deploy to real EFI System Partition (for strict UEFI laptops)
deploy-efi: uefi
	@EFI_PART=""; \
	if mount | grep -q "$(USB_EFI_VOL)"; then \
		EFI_PART="already_mounted"; \
	else \
		EFI_PART=$$(diskutil list | grep -i "EFI" | grep -i "disk" | head -1 | awk '{print $$NF}'); \
		if [ -n "$$EFI_PART" ]; then \
			echo "Auto-detected EFI partition: $$EFI_PART"; \
			diskutil mount $$EFI_PART || diskutil mount $(USB_EFI_DISK); \
		else \
			echo "Mounting EFI partition ($(USB_EFI_DISK))..."; \
			diskutil mount $(USB_EFI_DISK); \
		fi; \
	fi
	@mkdir -p "$(USB_EFI_VOL)/EFI/BOOT"
	@cp $(BOOT_EFI) "$(USB_EFI_VOL)/EFI/BOOT/BOOTX64.EFI"
	@echo "Deployed to $(USB_EFI_VOL)/EFI/BOOT/BOOTX64.EFI"

# Deploy to BOTH partitions (recommended)
deploy: deploy-data deploy-efi
	@echo "Deploy complete: kernel on both GOOS + EFI partitions."

clean:
	rm -rf $(BUILD_DIR)
	cargo clean --manifest-path $(KERNEL_MANIFEST)
	cargo clean --manifest-path sdk/reduxlang/Cargo.toml

.PHONY: all uefi litehtml-sync litehtml-bridge-build servo-adapter-build servort-stage servort-stage-esp linux-guest-stage linux-guest-build run install-nvme newlib-help newlib-scaffold newlib-build newlib-doctor wry-host servo-host ide deploy deploy-data deploy-efi clean
