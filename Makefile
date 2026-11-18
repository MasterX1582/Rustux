# linux-rust: run inside WSL (Ubuntu). `make run` builds the kernel and boots
# it in QEMU; the kernel prints over COM1 and exits QEMU via isa-debug-exit.

ESP      := esp
OVMF     ?= /usr/share/ovmf/OVMF.fd
QEMU     ?= qemu-system-x86_64
TARGET   := x86_64-unknown-uefi
KERNEL   := target/$(TARGET)/release/kernel.efi
BOOTX64  := $(ESP)/EFI/BOOT/BOOTX64.EFI

.PHONY: build run clean

build: $(BOOTX64)

$(KERNEL): kernel/src/*.rs kernel/Cargo.toml Cargo.toml
	cargo build --release --target $(TARGET) -p kernel

$(BOOTX64): $(KERNEL)
	mkdir -p $(dir $@)
	cp $< $@

run: build
	$(QEMU) \
		-machine q35 \
		-bios $(OVMF) \
		-drive file=fat:rw:$(ESP),format=raw \
		-serial stdio -display none \
		-device isa-debug-exit,iobase=0xf4,iosize=0x04 \
		-no-reboot; \
	status=$$?; \
	if [ $$status -eq 33 ]; then echo "boot test: PASS (qemu exit 33)"; \
	else echo "boot test: FAIL (qemu exit $$status)"; exit 1; fi

clean:
	cargo clean
	rm -rf $(ESP)
