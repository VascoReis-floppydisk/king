#![no_std]
#![no_main]

use core::arch::{asm, naked_asm};
use core::panic::PanicInfo;

const VGA_WIDTH: isize = 80;
const VGA_HEIGHT: isize = 25;
const DEFAULT_COLOR: u16 = 0x0F00;

static mut CURSOR_X: isize = 0;
static mut CURSOR_Y: isize = 1;
static mut COMMAND_BUFFER: [u8; 64] = [0; 64];
static mut COMMAND_LEN: usize = 0;

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct IdtEntry { low: u16, sel: u16, res: u8, flags: u8, high: u16 }
#[repr(C, packed)]
struct IdtPtr { limit: u16, base: u32 }
static mut IDT: [IdtEntry; 256] = [IdtEntry { low: 0, sel: 0, res: 0, flags: 0, high: 0 }; 256];

// --- UTILITIES ---

unsafe fn putchar_attr(c: u8, attr: u16) {
    let vga = 0xb8000 as *mut u16;
    if c == b'\n' { 
        CURSOR_X = 0; CURSOR_Y += 1; 
    } else if c == b'\x08' {
        if CURSOR_X > 2 { 
            CURSOR_X -= 1; 
            *vga.offset(CURSOR_Y * 80 + CURSOR_X) = DEFAULT_COLOR | b' ' as u16; 
        }
    } else {
        *vga.offset(CURSOR_Y * 80 + CURSOR_X) = attr | c as u16;
        CURSOR_X += 1;
    }
    if CURSOR_X >= 80 { CURSOR_X = 0; CURSOR_Y += 1; }
    if CURSOR_Y >= 25 { scroll(); }
    update_hardware_cursor();
}

unsafe fn putchar(c: u8) { putchar_attr(c, DEFAULT_COLOR); }

unsafe fn scroll() {
    let vga = 0xb8000 as *mut u16;
    for y in 2..25 {
        for x in 0..80 {
            *vga.offset((y - 1) * 80 + x) = *vga.offset(y * 80 + x);
        }
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

unsafe fn print_str(s: &[u8]) { for &b in s { putchar(b); } }

unsafe fn print_color(s: &[u8], attr: u16) {
    for &b in s { putchar_attr(b, attr); }
}

// --- FETCH COMMAND ---
unsafe fn fetch() {
    let crown_clr = 0x0E00; // Gold/Yellow
    let label_clr = 0x0B00; // Cyan
    let value_clr = 0x0F00; // White

    // Row 1
    print_color(b"          o          ", crown_clr);
    print_color(b"  OS:     ", label_clr); print_color(b"King OS\n", value_clr);

    // Row 2
    print_color(b"       o^/|\\^o       ", crown_clr);
    print_color(b"  KERNEL: ", label_clr); print_color(b"Rust-i386\n", value_clr);

    // Row 3
    print_color(b"    o_^|\\/*\\/|^_o    ", crown_clr);
    
    // CPU Query for Row 3 info
    let mut b: u32; let mut d: u32; let mut c: u32;
    asm!("cpuid", inout("eax") 0 => _, out("ebx") b, out("edx") d, out("ecx") c);
    print_color(b"  CPU:    ", label_clr);
    for &reg in &[b, d, c] {
        for i in 0..4 { putchar_attr((reg >> (i * 8)) as u8, value_clr); }
    }
    putchar(b'\n');

    // Row 4
    print_color(b"   o\\*`'.\\|/.'`*/o   ", crown_clr);
    print_color(b"  SHELL:  ", label_clr); print_color(b"KingShell\n", value_clr);

    // Row 5
    print_color(b"    \\\\\\\\\\\\|//////    ", crown_clr);
    print_color(b"  MEM:    ", label_clr); print_color(b"640KB Base\n", value_clr);

    // Row 6
    print_color(b"     {><><@><><}      ", crown_clr);
    print_color(b" UPTIME:  ", label_clr); print_color(b"Just Booted\n", value_clr);

    // Row 7
    print_color(b"     `\"\"\"\"\"\"\"\"\"`    ", crown_clr);
    putchar(b'\n');

    // Color Palette Bar (Neofetch style)
    putchar(b'\n');
    print_str(b"    ");
    for i in 0..8 {
        // Print two spaces with different background colors
        putchar_attr(b' ', (i << 12) | (i << 8));
        putchar_attr(b' ', (i << 12) | (i << 8));
    }
    putchar(b'\n');
}

// --- COMMAND PARSER ---

unsafe fn str_eq(buf: &[u8], cmd: &[u8]) -> bool {
    if buf.len() != cmd.len() { return false; }
    for i in 0..buf.len() { if buf[i] != cmd[i] { return false; } }
    true
}

unsafe fn execute_command() {
    let cmd = &COMMAND_BUFFER[..COMMAND_LEN];
    if str_eq(cmd, b"FETCH") { fetch(); }
    else if str_eq(cmd, b"HELP") { print_str(b"HELP, FETCH, HELLO, CLEAR, CPUID, REBOOT\n"); }
    else if str_eq(cmd, b"HELLO") { print_str(b"GREETINGS, KING!\n"); }
    else if str_eq(cmd, b"CLEAR") {
        let vga = 0xb8000 as *mut u16;
        for i in 80..(80 * 25) { *vga.offset(i) = DEFAULT_COLOR | b' ' as u16; }
        CURSOR_X = 0; CURSOR_Y = 1;
    } else if str_eq(cmd, b"CPUID") {
        let mut b: u32; let mut d: u32; let mut c: u32;
        asm!("cpuid", inout("eax") 0 => _, out("ebx") b, out("edx") d, out("ecx") c);
        for &reg in &[b, d, c] {
            for i in 0..4 { putchar((reg >> (i * 8)) as u8); }
        }
        putchar(b'\n');
    } else if str_eq(cmd, b"REBOOT") {
        let ptr = IdtPtr { limit: 0, base: 0 };
        asm!("lidt [{}]", in(reg) &ptr);
        asm!("int 3");
    } else if COMMAND_LEN > 0 {
        print_str(b"UNKNOWN: "); print_str(cmd); putchar(b'\n');
    }
    COMMAND_LEN = 0;
}

// --- KEYBOARD ---

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

#[no_mangle]
pub unsafe extern "C" fn keyboard_handler() {
    let scancode: u8;
    asm!("in al, 0x60", out("al") scancode);
    if scancode < 0x80 {
        let ascii = scancode_to_ascii(scancode);
        if ascii == b'\n' { putchar(b'\n'); execute_command(); print_str(b"> "); }
        else if ascii == b'\x08' { if COMMAND_LEN > 0 { COMMAND_LEN -= 1; putchar(b'\x08'); } }
        else if ascii != 0 && COMMAND_LEN < 64 {
            COMMAND_BUFFER[COMMAND_LEN] = ascii;
            COMMAND_LEN += 1;
            putchar(ascii);
        }
    }
    asm!("mov al, 0x20", "out 0x20, al");
}

#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn kb_wrapper() {
    naked_asm!(".code32", "pushad", "call keyboard_handler", "popad", "iretd");
}

// --- ENTRY ---

#[no_mangle]
pub extern "C" fn stage2_entry() -> ! {
    unsafe {
        let vga = 0xb8000 as *mut u16;
        for i in 0..2000 { *vga.offset(i) = DEFAULT_COLOR | b' ' as u16; }
        let h = b"--- KING OS v0.5 (GOLDEN CROWN FETCH) ---";
        for (i, &b) in h.iter().enumerate() { *vga.offset(i as isize) = 0x4F00 | b as u16; }

        let addr = kb_wrapper as u32;
        IDT[33] = IdtEntry { low: (addr & 0xFFFF) as u16, sel: 0x08, res: 0, flags: 0x8E, high: (addr >> 16) as u16 };
        let ptr = IdtPtr { limit: 2047, base: &IDT as *const _ as u32 };
        asm!("lidt [{}]", in(reg) &ptr);

        // PIC Remap
        asm!("mov al, 0x11", "out 0x20, al", "out 0xA0, al", "mov al, 0x20", "out 0x21, al", "mov al, 0x28", "out 0xA1, al");
        asm!("mov al, 0x04", "out 0x21, al", "mov al, 0x02", "out 0xA1, al", "mov al, 0x01", "out 0x21, al", "out 0xA1, al");
        asm!("mov al, 0xFD", "out 0x21, al", "mov al, 0xFF", "out 0xA1, al");

        print_str(b"> ");
        asm!("sti");
        update_hardware_cursor();
    }
    loop { unsafe { asm!("hlt"); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! { loop {} }