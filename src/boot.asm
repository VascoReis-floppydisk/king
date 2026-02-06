[bits 16]
[org 0x7c00]

KERNEL_OFFSET equ 0x8000

start:
    ; 1. Standardize the environment
    cli                     ; Clear interrupts during setup
    xor ax, ax              ; Zero out AX
    mov ds, ax
    mov es, ax              ; CRITICAL: Ensure ES is 0 for BIOS read
    mov ss, ax
    mov sp, 0x7c00          ; Put stack safely below bootloader
    mov [BOOT_DRIVE], dl    ; Store boot drive provided by BIOS
    sti

    call load_kernel
    call switch_to_pm
    jmp $

%include "src/gdt.asm"

load_kernel:
    mov ah, 0x02            ; BIOS Read sectors
    mov al, 64              ; Load 64 sectors
    mov ch, 0x00            ; Cylinder 0
    mov dh, 0x00            ; Head 0
    mov cl, 0x02            ; Sector 2
    mov dl, [BOOT_DRIVE]
    mov bx, KERNEL_OFFSET   ; Destination 0x0000:0x8000
    int 0x13
    jc disk_error           ; Jump if carry flag set
    ret

disk_error:
    mov ah, 0x0e
    mov al, 'D'             ; 'D' for Disk Error
    int 0x10
    jmp $

switch_to_pm:
    cli
    lgdt [gdt_descriptor]
    mov eax, cr0
    or eax, 0x1             ; Set bit 0 of CR0 (Protected Mode)
    mov cr0, eax
    jmp 0x08:init_pm        ; Far jump to 32-bit segment

[bits 32]
init_pm:
    mov ax, 0x10            ; Data segment selector (0x10 is GDT data)
    mov ds, ax
    mov ss, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    mov ebp, 0x90000        ; Set up a high stack
    mov esp, ebp

    call KERNEL_OFFSET      ; Jump to your Rust code
    jmp $

BOOT_DRIVE db 0
times 510-($-$$) db 0
dw 0xaa55