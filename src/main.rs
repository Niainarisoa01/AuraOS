#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;
use bootloader as _;

// Core concurrency primitive
mod sync;

// Subsystem architecture layers
mod arch;
mod drivers;
mod memory;
mod task;
mod fs;
pub mod gui;
mod shell;
mod tests;

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use core::panic::PanicInfo;

/// Entry point of the AuraOS kernel.
/// Called by the bootloader in 64-bit Long Mode.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Step 0: Initialize COM1 serial port for immediate debug logging
    drivers::serial::init();
    serial_println!("============================================================");
    serial_println!("        AuraOS Kernel v0.1.0 - Booting Up...                ");
    serial_println!("============================================================");

    // ==========================================
    // AuraOS Kernel Boot Phase
    // ==========================================
    println!("============================================================");
    println!("        AuraOS Kernel v0.1.0 - Booting Up...                ");
    println!("============================================================");
    println!();

    // Step 1: Initialize the Global Descriptor Table (GDT)
    arch::gdt::init();

    // Step 2: Initialize and remap the dual 8259 PIC controllers
    arch::pic::init();
    serial_println!("[OK] PIC remapped.");

    // Step 3: Initialize the Interrupt Descriptor Table (IDT)
    arch::idt::init();
    serial_println!("[OK] IDT loaded.");

    // Step 3a: Initialize PIT 8254 Timer at 100 Hz (preemptive scheduling)
    drivers::pit::init();

    // Step 3b: Initialize PS/2 Mouse auxiliary controller
    drivers::mouse::init();
    println!("[OK] Mouse     : PS/2 Mouse initialized (IRQ 12 active).");
    serial_println!("[OK] PS/2 Mouse initialized.");

    // Step 3c: Initialize x86_64 Native System Call Interface (syscall/sysret)
    arch::syscall::init();

    // Step 4: Initialize the 8 MiB Dynamic Heap Allocator
    memory::allocator::init_heap();
    serial_println!("[OK] 8 MiB Heap initialized.");

    // Step 4b: Initialize User Space Memory Manager
    memory::user_space::init();

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
        serial_println!("[OK] Dynamic allocations verified: Box, Vec, String.");
    }

    // Step 6: CPU and Hardware Clock Detection
    let cpu = arch::cpuid::get_cpu_info();
    let rtc = drivers::cmos::read_rtc();
    println!("[OK] Processor : {} ({})", cpu.brand_str(), cpu.vendor_str());
    println!("[OK] RTC Clock : {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", rtc.year, rtc.month, rtc.day, rtc.hour, rtc.minute, rtc.second);
    serial_println!("[OK] Processor: {} ({})", cpu.brand_str(), cpu.vendor_str());
    serial_println!("[OK] RTC Clock: {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", rtc.year, rtc.month, rtc.day, rtc.hour, rtc.minute, rtc.second);

    // Step 7: PCI Hardware Bus Enumeration
    let pci_devices = drivers::pci::scan_pci_bus();
    println!("[OK] PCI Bus   : Discovered {} active hardware device(s).", pci_devices.len());
    serial_println!("[OK] PCI Bus: {} active device(s) enumerated.", pci_devices.len());
    for dev in &pci_devices {
        serial_println!("      [{:02x}:{:02x}.{}] {:04x}:{:04x} | {} - {}",
            dev.bus, dev.slot, dev.func, dev.vendor_id, dev.device_id,
            dev.vendor_name(), dev.class_name());
    }

    // Step 8: Initialize Kernel Multitasking & Scheduler
    task::init();
    println!("[OK] Tasks     : Round-Robin scheduler initialized (TCBs ready).");
    serial_println!("[OK] Multitasking initialized.");

    // Step 9: Initialize Virtual File System & RAMFS
    fs::init();
    println!("[OK] VFS       : Root RAM disk mounted at '/' (hierarchy ready).");
    serial_println!("[OK] VFS & RAMFS mounted.");

    // Step 10: Enable hardware interrupts at the CPU level
    unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
    println!("[OK] CPU       : Hardware interrupts enabled (sti).");
    serial_println!("[OK] Interrupts enabled (sti).");

    // Step 11: Execute Kernel Automated Subsystem Diagnostics
    println!();
    println!("[DIAG] Running AuraOS automated self-test verification...");
    let test_results = tests::run_all_tests();
    let mut all_passed = true;
    for res in &test_results {
        if res.passed {
            println!("  [PASS] {}", res.name);
            serial_println!("  [PASS] {} -- {}", res.name, res.detail);
        } else {
            all_passed = false;
            println!("  [FAIL] {} ({})", res.name, res.detail);
            serial_println!("  [FAIL] {} -- {}", res.name, res.detail);
        }
    }
    if all_passed {
        println!("[OK] All {} subsystem tests PASSED. System verified 100%.", test_results.len());
        serial_println!("[OK] All {} automated self-tests passed.", test_results.len());
    } else {
        println!("[WARN] One or more diagnostic checks reported issues.");
    }

    // Step 12: System status report
    println!();
    println!("[OK] Mode      : x86_64 Bare-Metal Long Mode (64-bit)");
    println!("[OK] Memory    : 8 MiB Heap, 4 KiB Paging abstractions");
    println!("[OK] Filesystem: Virtual Inode VFS mounted at '/'");
    println!("[OK] Storage   : ATA / IDE PIO 28-bit driver active");
    println!("[OK] Serial    : COM1 UART at 0x3F8 (115200 baud)");
    println!("[OK] Video     : VGA 80x25 text mode with auto-scrolling");
    println!("[OK] Keyboard  : PS/2 driver active (direct typing)");
    println!();
    println!("------------------------------------------------------------");
    println!("  AuraOS v0.1.0 ready. Interactive console active:");
    println!("------------------------------------------------------------");
    print!("auraos> ");

    serial_println!("AuraOS v0.1.0 interactive console ready.");

    // Interactive kernel execution loop:
    // Processes pending keyboard inputs and enters low-power sleep (HLT)
    // until the next hardware interrupt (timer tick or keypress).
    loop {
        drivers::keyboard::process_pending_keys();
        unsafe { core::arch::asm!("sti; hlt", options(nomem, nostack)); }
    }
}

/// Invoked on panic. Prints error message and halts CPU.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack));
        // Safely force unlock output devices in case panic occurred inside print!
        drivers::vga::WRITER.force_unlock();
        drivers::serial::SERIAL1.force_unlock();
    }
    serial_println!("\n[KERNEL PANIC] {}", info);
    println!();
    println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    println!("  KERNEL PANIC - AuraOS encountered a fatal error!");
    println!("  {}", info);
    println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    loop {
        unsafe { core::arch::asm!("hlt", options(nostack, preserves_flags)) };
    }
}
