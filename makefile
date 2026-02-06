all:
	cargo build --release
	nasm -f bin src/boot.asm -o boot.bin
	ld -m elf_i386 -nostdlib -static -T linker.ld \
		target/i686-king/release/libking.a \
		-o kernel.bin --oformat binary
	
	# 1. Create the 2MB file ONLY if it doesn't exist
	@if [ ! -f king.bin ]; then dd if=/dev/zero of=king.bin bs=1M count=2; fi
	
	# 2. Patch the bootloader and kernel into the existing file
	# conv=notrunc is the "Magic Sauce" that saves your MAIN.RS
	dd if=boot.bin of=king.bin conv=notrunc
	dd if=kernel.bin of=king.bin seek=1 conv=notrunc

run: all
	qemu-system-i386 -drive file=king.bin,format=raw,index=0,media=disk,if=ide,cache=directsync,aio=threads,file.locking=off