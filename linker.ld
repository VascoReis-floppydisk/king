/* This tells the linker that the first byte of the file should be our entry function */
ENTRY(stage2_entry)

SECTIONS
{
    /* The kernel will be loaded at 0x8000 by the bootloader */
    . = 0x8000;

    /* Combine all code sections */
    .text :
    {
        /* Ensure the entry point is at the very beginning of the binary */
        *(.text.stage2_entry)
        *(.text .text.*)
    }

    /* Read-only data (strings, constants) */
    .rodata :
    {
        *(.rodata .rodata.*)
    }

    /* Initialized data */
    .data :
    {
        *(.data .data.*)
    }

    /* Uninitialized data (zero-filled) */
    .bss :
    {
        *(.bss .bss.*)
        *(COMMON)
    }

    /* Discard sections we don't need for a bare-metal kernel */
    /DISCARD/ :
    {
        *(.comment)
        *(.eh_frame)
        *(.rel.eh_frame)
    }
}