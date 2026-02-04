all:
	cargo build --release
	nasm -f bin src/boot.asm -o boot.bin
	ld -m elf_i386 -nostdlib -static -T linker.ld \
		target/i686-king/release/libking.a \
		-o kernel.bin --oformat binary
	cat boot.bin kernel.bin > king.bin

run: all
	qemu-system-i386 -drive format=raw,file=king.bin