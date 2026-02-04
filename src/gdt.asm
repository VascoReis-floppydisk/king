gdt_start:
    ; Null descriptor (8 bytes of zeros)
    dq 0x0

gdt_code: 
    ; Code segment: base=0x0, limit=0xfffff
    ; Type: present, privilege 0, code segment, executable, readable, 32-bit
    dw 0xffff    ; Limit 0-15
    dw 0x0       ; Base 0-15
    db 0x0       ; Base 16-23
    db 10011010b ; Flags (8 bits)
    db 11001111b ; Flags (4 bits) + Limit (4 bits)
    db 0x0       ; Base 24-31

gdt_data:
    ; Data segment: base=0x0, limit=0xfffff
    ; Type: same as code but writable and not executable
    dw 0xffff
    dw 0x0
    db 0x0
    db 10010010b
    db 11001111b
    db 0x0

gdt_end:

gdt_descriptor:
    dw gdt_end - gdt_start - 1
    dd gdt_start

; Constants for segment selectors
CODE_SEG equ gdt_code - gdt_start
DATA_SEG equ gdt_data - gdt_start