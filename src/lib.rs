#![no_std]
#![no_main]

use core::arch::{asm, naked_asm};
use core::panic::PanicInfo;

// VGA Constants
const VGA_WIDTH: isize = 80;
const VGA_HEIGHT: isize = 25;
const DEFAULT_COLOR: u16 = 0x0F00; // White on Black

static mut CURSOR_X: isize = 0;
static mut CURSOR_Y: isize = 1; // Start below the header

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct IdtEntry { low: u16, sel: u16, res: u8, flags: u8, high: u16 }

#[repr(C, packed)]
struct IdtPtr { limit: u16, base: u32 }

static mut IDT: [IdtEntry; 256] = [IdtEntry { low: 0, sel: 0, res: 0, flags: 0, high: 0 }; 256];

unsafe fn update_hardware_cursor() {
    let pos = (CURSOR_Y * VGA_WIDTH + CURSOR_X) as u16;
    asm!("out dx, al", in("dx") 0x3D4u16, in("al") 0x0Fu8);
    asm!("out dx, al", in("dx") 0x3D5u16, in("al") (pos & 0xFF) as u8);
    asm!("out dx, al", in("dx") 0x3D4u16, in("al") 0x0Eu8);
    asm!("out dx, al", in("dx") 0x3D5u16, in("al") ((pos >> 8) & 0xFF) as u8);
}

unsafe fn scroll() {
    let vga = 0xb8000 as *mut u16;
    // Move lines 2-24 up to lines 1-23 (keeps header at line 0)
    for y in 2..VGA_HEIGHT {
        for x in 0..VGA_WIDTH {
            let src = y * VGA_WIDTH + x;
            let dest = (y - 1) * VGA_WIDTH + x;
            *vga.offset(dest) = *vga.offset(src);
        }
    }
    // Clear the last line
    for x in 0..VGA_WIDTH {
        *vga.offset((VGA_HEIGHT - 1) * VGA_WIDTH + x) = DEFAULT_COLOR | b' ' as u16;
    }
    CURSOR_Y = VGA_HEIGHT - 1;
}

unsafe fn putchar(c: u8) {
    let vga = 0xb8000 as *mut u16;
    match c {
        b'\x08' => { // Backspace
            if CURSOR_X > 0 {
                CURSOR_X -= 1;
                *vga.offset(CURSOR_Y * VGA_WIDTH + CURSOR_X) = DEFAULT_COLOR | b' ' as u16;
            }
        }
        b'\n' => { // Enter
            CURSOR_X = 0;
            CURSOR_Y += 1;
        }
        _ => {
            *vga.offset(CURSOR_Y * VGA_WIDTH + CURSOR_X) = DEFAULT_COLOR | c as u16;
            CURSOR_X += 1;
        }
    }

    if CURSOR_X >= VGA_WIDTH {
        CURSOR_X = 0;
        CURSOR_Y += 1;
    }
    if CURSOR_Y >= VGA_HEIGHT {
        scroll();
    }
    update_hardware_cursor();
}

fn scancode_to_ascii(scancode: u8) -> u8 {
    match scancode {
        0x1E => b'A', 0x30 => b'B', 0x2E => b'C', 0x20 => b'D', 0x12 => b'E',
        0x21 => b'F', 0x22 => b'G', 0x23 => b'H', 0x17 => b'I', 0x24 => b'J',
        0x25 => b'K', 0x26 => b'L', 0x32 => b'M', 0x31 => b'N', 0x18 => b'O',
        0x19 => b'P', 0x10 => b'Q', 0x13 => b'R', 0x1F => b'S', 0x14 => b'T',
        0x16 => b'U', 0x2F => b'V', 0x11 => b'W', 0x2D => b'X', 0x15 => b'Y',
        0x2C => b'Z', 0x39 => b' ', 0x1C => b'\n', 0x0E => b'\x08',
        0x02..=0x0B => scancode + 47, // 1-9 is 0x02-0x0A, 0 is 0x0B
        _ => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn keyboard_handler() {
    let scancode: u8;
    asm!("in al, dx", out("al") scancode, in("dx") 0x60u16);
    if scancode < 0x80 {
        let ascii = scancode_to_ascii(scancode);
        if ascii != 0 { putchar(ascii); }
    }
    asm!("mov al, 0x20", "out 0x20, al");
}

#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn kb_wrapper() {
    naked_asm!(".code32", "pushad", "call keyboard_handler", "popad", "iretd");
}

#[no_mangle]
pub extern "C" fn stage2_entry() -> ! {
    unsafe {
        let vga = 0xb8000 as *mut u16;
        for i in 0..(VGA_WIDTH * VGA_HEIGHT) {
            core::ptr::write_volatile(vga.offset(i), DEFAULT_COLOR | b' ' as u16);
        }

        let header = b"--- KING OS v0.2 (SCROLLING ENABLED) ---";
        for (i, &byte) in header.iter().enumerate() {
            *vga.offset(i as isize) = 0x4F00 | byte as u16; // White on Red
        }

        let addr = kb_wrapper as u32;
        IDT[33] = IdtEntry { low: (addr & 0xFFFF) as u16, sel: 0x08, res: 0, flags: 0x8E, high: (addr >> 16) as u16 };
        let ptr = IdtPtr { limit: 2047, base: &IDT as *const _ as u32 };
        asm!("lidt [{}]", in(reg) &ptr);

        asm!("mov al, 0x11", "out 0x20, al", "out 0xA0, al");
        asm!("mov al, 0x20", "out 0x21, al", "mov al, 0x28", "out 0xA1, al");
        asm!("mov al, 0x04", "out 0x21, al", "mov al, 0x02", "out 0xA1, al");
        asm!("mov al, 0x01", "out 0x21, al", "out 0xA1, al");
        asm!("mov al, 0xFD", "out 0x21, al", "mov al, 0xFF", "out 0xA1, al");
        asm!("sti");
        update_hardware_cursor();
    }
    loop { unsafe { asm!("hlt"); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! { loop {} }