#![no_std]
#![no_main]

use core::arch::{asm, naked_asm};
use core::panic::PanicInfo;

const DEFAULT_COLOR: u16 = 0x0F00;
const DIR_SECTOR: u32 = 2048; 

// ATA Ports
const ATA_DATA: u16 = 0x1F0;
const ATA_SECTOR_COUNT: u16 = 0x1F2;
const ATA_LBA_LOW: u16 = 0x1F3;
const ATA_LBA_MID: u16 = 0x1F4;
const ATA_LBA_HIGH: u16 = 0x1F5;
const ATA_DRIVE_SELECT: u16 = 0x1F6;
const ATA_COMMAND: u16 = 0x1F7;
const ATA_CONTROL: u16 = 0x3F6;

// --- STRUCTURES ---

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct FileEntry {
    name: [u8; 8],
    start_sector: u32,
    size: u32,
    active: u8,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct IdtEntry { low: u16, sel: u16, res: u8, flags: u8, high: u16 }
#[repr(C, packed)]
struct IdtPtr { limit: u16, base: u32 }

// --- STATIC STORAGE ---
static mut CURSOR_X: isize = 0;
static mut CURSOR_Y: isize = 1;
static mut COMMAND_BUFFER: [u8; 64] = [0; 64];
static mut COMMAND_LEN: usize = 0;
static mut TICKS: u32 = 0;
static mut DISK_TRANSFER_BUFFER: [u16; 256] = [0; 256];
static mut IDT: [IdtEntry; 256] = [IdtEntry { low: 0, sel: 0, res: 0, flags: 0, high: 0 }; 256];

// --- DISK DRIVER (HARDENED) ---

unsafe fn io_wait() {
    // Standard 400ns delay via reading the control register
    for _ in 0..4 { asm!("in al, dx", out("al") _, in("dx") ATA_CONTROL); }
}

unsafe fn wait_for_not_busy() {
    loop {
        let status: u8;
        asm!("in al, dx", out("al") status, in("dx") ATA_COMMAND);
        if (status & 0x80) == 0 { break; } 
    }
}

unsafe fn wait_for_drq() -> bool {
    let mut timeout = 100000;
    while timeout > 0 {
        let status: u8;
        asm!("in al, dx", out("al") status, in("dx") ATA_COMMAND);
        if (status & 0x08) != 0 { return true; }
        if (status & 0x01) != 0 { return false; } // Error bit
        timeout -= 1;
    }
    false
}

unsafe fn write_sector(lba: u32, buffer: *const u16) {
    wait_for_not_busy();
    asm!("out dx, al", in("dx") ATA_DRIVE_SELECT, in("al") (0xE0 | ((lba >> 24) & 0x0F)) as u8);
    io_wait();
    asm!("out dx, al", in("dx") ATA_SECTOR_COUNT, in("al") 1u8);
    asm!("out dx, al", in("dx") ATA_LBA_LOW, in("al") (lba & 0xFF) as u8);
    asm!("out dx, al", in("dx") ATA_LBA_MID, in("al") ((lba >> 8) & 0xFF) as u8);
    asm!("out dx, al", in("dx") ATA_LBA_HIGH, in("al") ((lba >> 16) & 0xFF) as u8);
    asm!("out dx, al", in("dx") ATA_COMMAND, in("al") 0x30u8); 

    if wait_for_drq() {
        for i in 0..256 {
            asm!("out dx, ax", in("dx") ATA_DATA, in("ax") *buffer.offset(i));
        }
    }
    
    asm!("out dx, al", in("dx") ATA_COMMAND, in("al") 0xE7u8); // Cache Flush
    wait_for_not_busy();
}

unsafe fn read_sector(lba: u32, buffer: *mut u16) {
    wait_for_not_busy();
    asm!("out dx, al", in("dx") ATA_DRIVE_SELECT, in("al") (0xE0 | ((lba >> 24) & 0x0F)) as u8);
    io_wait();
    asm!("out dx, al", in("dx") ATA_SECTOR_COUNT, in("al") 1u8);
    asm!("out dx, al", in("dx") ATA_LBA_LOW, in("al") (lba & 0xFF) as u8);
    asm!("out dx, al", in("dx") ATA_LBA_MID, in("al") ((lba >> 8) & 0xFF) as u8);
    asm!("out dx, al", in("dx") ATA_LBA_HIGH, in("al") ((lba >> 16) & 0xFF) as u8);
    asm!("out dx, al", in("dx") ATA_COMMAND, in("al") 0x20u8); 

    if wait_for_drq() {
        for i in 0..256 {
            asm!("in ax, dx", out("ax") *buffer.offset(i), in("dx") ATA_DATA);
        }
    }
}

// --- UTILITIES ---

unsafe fn putchar_attr(c: u8, attr: u16) {
    let vga = 0xb8000 as *mut u16;
    if c == b'\n' { CURSOR_X = 0; CURSOR_Y += 1; }
    else if c == b'\x08' {
        if CURSOR_X > 2 { CURSOR_X -= 1; *vga.offset(CURSOR_Y * 80 + CURSOR_X) = DEFAULT_COLOR | b' ' as u16; }
    } else {
        *vga.offset(CURSOR_Y * 80 + CURSOR_X) = attr | c as u16;
        CURSOR_X += 1;
    }
    if CURSOR_X >= 80 { CURSOR_X = 0; CURSOR_Y += 1; }
    if CURSOR_Y >= 25 { scroll(); }
    update_hardware_cursor();
}

unsafe fn putchar(c: u8) { putchar_attr(c, DEFAULT_COLOR); }
unsafe fn print_str(s: &[u8]) { for &b in s { if b != 0 { putchar(b); } } }
unsafe fn print_color(s: &[u8], attr: u16) { for &b in s { putchar_attr(b, attr); } }

unsafe fn print_num(mut n: u32, attr: u16) {
    if n == 0 { putchar_attr(b'0', attr); return; }
    let mut buf = [0u8; 10]; let mut i = 10;
    while n > 0 { i -= 1; buf[i] = (n % 10) as u8 + b'0'; n /= 10; }
    print_color(&buf[i..], attr);
}

unsafe fn scroll() {
    let vga = 0xb8000 as *mut u16;
    for y in 2..25 {
        for x in 0..80 { *vga.offset((y - 1) * 80 + x) = *vga.offset(y * 80 + x); }
    }
    for x in 0..80 { *vga.offset(24 * 80 + x) = DEFAULT_COLOR | b' ' as u16; }
    CURSOR_Y = 24;
}

unsafe fn update_hardware_cursor() {
    let pos = (CURSOR_Y * 80 + CURSOR_X) as u16;
    asm!("out dx, al", in("dx") 0x3D4u16, in("al") 0x0Fu8);
    asm!("out dx, al", in("dx") 0x3D5u16, in("al") (pos & 0xFF) as u8);
    asm!("out dx, al", in("dx") 0x3D4u16, in("al") 0x0Eu8);
    asm!("out dx, al", in("dx") 0x3D5u16, in("al") ((pos >> 8) & 0xFF) as u8);
}

unsafe fn str_eq(buf: &[u8], cmd: &[u8]) -> bool {
    if buf.len() != cmd.len() { return false; }
    for i in 0..buf.len() { if buf[i] != cmd[i] { return false; } }
    true
}

// --- FETCH ---

unsafe fn fetch() {
    let crown_clr = 0x0E00; let label_clr = 0x0B00; let value_clr = 0x0F00;
    print_color(b"          o          ", crown_clr);
    print_color(b"  OS:     ", label_clr); print_color(b"King OS v0.7.5\n", value_clr);
    print_color(b"       o^/|\\^o       ", crown_clr);
    print_color(b"  DISK:   ", label_clr); print_color(b"ATA LBA-2048\n", value_clr);
    print_color(b"    o_^|\\/*\\/|^_o    ", crown_clr);
    print_color(b"  UPTIME: ", label_clr); print_num(TICKS / 100, value_clr); print_color(b"s\n", value_clr);
    print_color(b"   o\\*`'.\\|/.'`*/o   ", crown_clr);
    print_color(b"  SHELL:  ", label_clr); print_color(b"KingShell\n", value_clr);
    print_color(b"    \\\\\\\\\\\\|//////    ", crown_clr);
    print_color(b"  MEM:    ", label_clr); print_color(b"640KB Base\n", value_clr);
    print_color(b"     {><><@><><}      ", crown_clr); putchar(b'\n');
    print_color(b"     `\"\"\"\"\"\"\"\"\"`     ", crown_clr); putchar(b'\n');
}

// --- COMMAND LOGIC ---

unsafe fn execute_command() {
    let cmd = &COMMAND_BUFFER[..COMMAND_LEN];
    if COMMAND_LEN == 0 { return; }

    if str_eq(cmd, b"FETCH") {
        fetch();
    } else if str_eq(cmd, b"HELP") {
        print_str(b"FETCH, LS, TOUCH, FORMAT, CLEAR, REBOOT\n");
    } else if str_eq(cmd, b"FORMAT") {
        for i in 0..256 { DISK_TRANSFER_BUFFER[i] = 0; }
        write_sector(DIR_SECTOR, DISK_TRANSFER_BUFFER.as_ptr());
        print_color(b"FORMATTED DIR\n", 0x0A00);
    } else if str_eq(cmd, b"LS") {
        read_sector(DIR_SECTOR, DISK_TRANSFER_BUFFER.as_mut_ptr());
        let entries = DISK_TRANSFER_BUFFER.as_ptr() as *const FileEntry;
        let mut found = false;
        for i in 0..16 {
            let entry = &*entries.offset(i as isize);
            if entry.active == 1 {
                print_str(&entry.name);
                print_str(b" [READY]\n");
                found = true;
            }
        }
        if !found { print_str(b"DIR EMPTY\n"); }
    } else if str_eq(cmd, b"TOUCH") {
        read_sector(DIR_SECTOR, DISK_TRANSFER_BUFFER.as_mut_ptr());
        let entries = DISK_TRANSFER_BUFFER.as_mut_ptr() as *mut FileEntry;
        let mut created = false;

        for i in 0..16 {
            let entry = &mut *entries.offset(i as isize);
            if entry.active == 0 {
                entry.name = *b"KINGFILE";
                entry.start_sector = 2050 + i as u32;
                entry.size = 512;
                entry.active = 1;
                created = true;
                break;
            }
        }

        if created {
            write_sector(DIR_SECTOR, DISK_TRANSFER_BUFFER.as_ptr());
            print_color(b"FILE CREATED\n", 0x0A00);
        } else {
            print_color(b"DIR FULL\n", 0x0C00);
        }
    } else if str_eq(cmd, b"CLEAR") {
        let vga = 0xb8000 as *mut u16;
        for i in 80..2000 { *vga.offset(i) = DEFAULT_COLOR | b' ' as u16; }
        CURSOR_X = 0; CURSOR_Y = 1;
    } else if str_eq(cmd, b"REBOOT") {
        let ptr = IdtPtr { limit: 0, base: 0 };
        asm!("lidt [{}]", in(reg) &ptr);
        asm!("int 3");
    } else {
        print_str(b"UNKNOWN: "); print_str(cmd); putchar(b'\n');
    }

    COMMAND_LEN = 0;
}

// --- INTERRUPTS ---

fn scancode_to_ascii(scancode: u8) -> u8 {
    match scancode {
        0x1E => b'A', 0x30 => b'B', 0x2E => b'C', 0x20 => b'D', 0x12 => b'E',
        0x21 => b'F', 0x22 => b'G', 0x23 => b'H', 0x17 => b'I', 0x24 => b'J',
        0x25 => b'K', 0x26 => b'L', 0x32 => b'M', 0x31 => b'N', 0x18 => b'O',
        0x19 => b'P', 0x10 => b'Q', 0x13 => b'R', 0x1F => b'S', 0x14 => b'T',
        0x16 => b'U', 0x2F => b'V', 0x11 => b'W', 0x2D => b'X', 0x15 => b'Y',
        0x2C => b'Z', 0x39 => b' ', 0x1C => b'\n', 0x0E => b'\x08',
        0x02..=0x0B => b"1234567890"[(scancode - 0x02) as usize],
        _ => 0,
    }
}

#[no_mangle] pub unsafe extern "C" fn timer_handler() { TICKS += 1; asm!("mov al, 0x20", "out 0x20, al"); }
#[no_mangle] pub unsafe extern "C" fn keyboard_handler() {
    let scancode: u8; asm!("in al, 0x60", out("al") scancode);
    if scancode < 0x80 {
        let ascii = scancode_to_ascii(scancode);
        if ascii == b'\n' { putchar(b'\n'); execute_command(); print_str(b"> "); }
        else if ascii == b'\x08' { if COMMAND_LEN > 0 { COMMAND_LEN -= 1; putchar(b'\x08'); } }
        else if ascii != 0 && COMMAND_LEN < 64 { COMMAND_BUFFER[COMMAND_LEN] = ascii; COMMAND_LEN += 1; putchar(ascii); }
    }
    asm!("mov al, 0x20", "out 0x20, al");
}

#[unsafe(naked)] #[no_mangle] pub unsafe extern "C" fn timer_wrapper() { naked_asm!(".code32", "pushad", "call timer_handler", "popad", "iretd"); }
#[unsafe(naked)] #[no_mangle] pub unsafe extern "C" fn kb_wrapper() { naked_asm!(".code32", "pushad", "call keyboard_handler", "popad", "iretd"); }

// --- ENTRY POINT ---

#[no_mangle]
pub extern "C" fn stage2_entry() -> ! {
    unsafe {
        let vga = 0xb8000 as *mut u16;
        for i in 0..2000 { *vga.offset(i) = DEFAULT_COLOR | b' ' as u16; }
        let h = b"--- KING OS v0.0.1 ---";
        for (i, &b) in h.iter().enumerate() { *vga.offset(i as isize) = 0x4F00 | b as u16; }

        // 1. HARD RESET ATA CONTROLLER
        // Writing 0x04 to Control Register resets the bus
        asm!("out dx, al", in("dx") ATA_CONTROL, in("al") 0x04u8);
        io_wait();
        asm!("out dx, al", in("dx") ATA_CONTROL, in("al") 0x00u8);
        io_wait();

        let t_addr = timer_wrapper as u32; let k_addr = kb_wrapper as u32;
        IDT[32] = IdtEntry { low: (t_addr & 0xFFFF) as u16, sel: 0x08, res: 0, flags: 0x8E, high: (t_addr >> 16) as u16 };
        IDT[33] = IdtEntry { low: (k_addr & 0xFFFF) as u16, sel: 0x08, res: 0, flags: 0x8E, high: (k_addr >> 16) as u16 };
        let ptr = IdtPtr { limit: 2047, base: &IDT as *const _ as u32 };
        asm!("lidt [{}]", in(reg) &ptr);

        asm!("mov al, 0x11", "out 0x20, al", "out 0xA0, al");
        asm!("mov al, 0x20", "out 0x21, al", "mov al, 0x28", "out 0xA1, al");
        asm!("mov al, 0x04", "out 0x21, al", "mov al, 0x02", "out 0xA1, al");
        asm!("mov al, 0x01", "out 0x21, al", "out 0xA1, al");
        asm!("mov al, 0xFC", "out 0x21, al", "mov al, 0xFF", "out 0xA1, al");

        print_str(b"\n> ");
        asm!("sti");
        update_hardware_cursor();
    }
    loop { unsafe { asm!("hlt"); } }
}

#[panic_handler] fn panic(_info: &PanicInfo) -> ! { loop {} }