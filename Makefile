# Rustux: run inside WSL (Ubuntu). `make run` builds the kernel, packs it
# into a read-only FAT disk image, and boots it in QEMU; the kernel prints over
# COM1 and exits QEMU via isa-debug-exit.
#
# The ESP is a real disk image (esp.img), not a QEMU vvfat share: vvfat maps
# guest writes onto host files, which would let anything running in the VM
# overwrite esp/EFI/BOOT/BOOTX64.EFI in this directory.

ESP      := esp
ESP_IMG  := esp.img
OVMF     ?= /usr/share/ovmf/OVMF.fd
QEMU     ?= qemu-system-x86_64
TARGET   := x86_64-unknown-uefi
KERNEL   := target/$(TARGET)/release/kernel.efi

# demo_net gates the socket-link protocol demos (ipv8/ipv6/tcp
# selftests driven by the MAC-byte role check). Enabled for `run` and
# `demo` (the two-guest proof image); a plain `make build` produces a
# kernel with no demo code paths at all.
DEMO_RUSTFLAGS := --cfg demo_net

.PHONY: build run demo clean FORCE

build: $(ESP_IMG)

# `run`/`demo` propagate this to the $(KERNEL) recipe below; FORCE
# makes the kernel/esp recipes always re-run so cargo (not make's
# timestamps) decides — RUSTFLAGS changes invalidate the fingerprint.
run demo: RUSTFLAGS = $(DEMO_RUSTFLAGS)
export RUSTFLAGS

demo: $(ESP_IMG)
	@echo "demo image (demo_net) ready at $(ESP_IMG)"

FORCE:

$(KERNEL): kernel/src/*.rs kernel/Cargo.toml klib/src/*.rs klib/Cargo.toml Cargo.toml FORCE
	cargo build --release --target $(TARGET) -p kernel

$(ESP_IMG): $(KERNEL)
	dd if=/dev/zero of=$@ bs=1M count=64
	mformat -i $@ ::
	mmd -i $@ ::/EFI
	mmd -i $@ ::/EFI/BOOT
	mcopy -i $@ $(KERNEL) ::/EFI/BOOT/BOOTX64.EFI

run: build
	$(QEMU) \
		-machine q35 \
		-cpu qemu64,+rdrand \
		-bios $(OVMF) \
		-drive file=$(ESP_IMG),format=raw,if=ide \
		-snapshot \
		-serial stdio -display none \
		-device isa-debug-exit,iobase=0xf4,iosize=0x04 \
		-no-reboot; \
	status=$$?; \
	if [ $$status -eq 33 ]; then echo "boot test: PASS (qemu exit 33)"; \
	else echo "boot test: FAIL (qemu exit $$status)"; exit 1; fi

clean:
	cargo clean
	rm -rf $(ESP) $(ESP_IMG)
