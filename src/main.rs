#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

mod vga_buffer;
mod gdt;
mod idt;
mod io;
mod pic;
mod keyboard;
mod memory;
mod allocator;
mod shell;

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use core::panic::PanicInfo;

/// Entry point of the AuraOS kernel.
/// Called by the bootloader in 64-bit Long Mode.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // ==========================================
    // AuraOS Kernel Boot Phase
    // ==========================================
    println!("============================================================");
    println!("        AuraOS Kernel v0.1.0 - Booting Up...                ");
    println!("============================================================");
    println!();

    // Step 1: Initialize the Global Descriptor Table (GDT)
    gdt::init();
    println!("[OK] GDT       : Global Descriptor Table loaded.");

    // Step 2: Initialize and remap the dual 8259 PIC controllers
    pic::init();

    // Step 3: Initialize the Interrupt Descriptor Table (IDT)
    idt::init();

    // Step 4: Initialize the 512 KiB Dynamic Heap Allocator
    allocator::init_heap();

    // Step 5: Verify dynamic allocations (Box, Vec, String)
    {
        let heap_val = Box::new(42u64);
        let mut test_vec = Vec::new();
        for i in 0..5 {
            test_vec.push(i * 10);
        }
        let test_str = format!("AuraOS Dynamic String (Vec len = {})", test_vec.len());
        println!("[OK] Allocator : Box({}), Vec({:?}), String ready.", *heap_val, &test_vec[..3]);
        println!("                 {}", test_str);
    }

    // Step 6: Enable hardware interrupts at the CPU level
    unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
    println!("[OK] CPU       : Hardware interrupts enabled (sti).");

    // Step 7: System status report
    println!();
    println!("[OK] Mode      : x86_64 Bare-Metal Long Mode (64-bit)");
    println!("[OK] Memory    : 512 KiB Heap, 4 KiB Paging abstractions");
    println!("[OK] Video     : VGA 80x25 text mode with auto-scrolling");
    println!("[OK] Keyboard  : PS/2 driver active (direct typing)");
    println!();
    println!("------------------------------------------------------------");
    println!("  AuraOS v0.1.0 ready. Interactive console active:");
    println!("------------------------------------------------------------");
    print!("auraos> ");

    // Low-power idle loop (HLT):
    // The CPU halts and only wakes up when a hardware interrupt
    // occurs (timer tick or keyboard keystroke).
    loop {
        unsafe { core::arch::asm!("hlt", options(nostack, preserves_flags)) };
    }
}

/// Invoked on panic. Prints error message and halts CPU.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!();
    println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    println!("  KERNEL PANIC - AuraOS encountered a fatal error!");
    println!("  {}", info);
    println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    loop {
        unsafe { core::arch::asm!("hlt", options(nostack, preserves_flags)) };
    }
}
