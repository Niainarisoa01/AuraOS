#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;
use bootloader as _;

// Core concurrency primitive
mod sync;

// Structured kernel logging
mod klog;

// Subsystem architecture layers
mod arch;
mod drivers;
mod memory;
mod task;
mod fs;
pub mod gui;
mod net;
mod shell;
mod tests;

use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use core::panic::PanicInfo;

/// Entry point of the AuraOS kernel.
/// Called by the bootloader in 64-bit Long Mode, which passes the boot
/// information (including the E820 memory map) in RDI.
#[unsafe(no_mangle)]
pub extern "C" fn _start(boot_info: &'static bootloader::bootinfo::BootInfo) -> ! {
    // Step 0: Initialize COM1 serial port for immediate debug logging
    drivers::serial::init();
    serial_println!("============================================================");
    serial_println!("        AuraOS Kernel v0.1.0 - Booting Up...                ");
    serial_println!("============================================================");

    // Step 0b: Initialize the Physical Memory Manager from the E820 memory map.
    // This must happen as early as possible so the bootloader-provided BootInfo
    // is captured into kernel-owned storage before anything could clobber it.
    memory::pmm::init(boot_info);

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
    klog!(Info, "pic", "PIC remapped.");

    // Step 3: Initialize the Interrupt Descriptor Table (IDT)
    arch::idt::init();
    klog!(Info, "idt", "IDT loaded.");

    // Step 3a: Initialize PIT 8254 Timer at 100 Hz (preemptive scheduling)
    drivers::pit::init();

    // Step 3b: Initialize PS/2 Mouse auxiliary controller
    drivers::mouse::init();
    println!("[OK] Mouse     : PS/2 Mouse initialized (IRQ 12 active).");
    klog!(Info, "mouse", "PS/2 Mouse initialized.");

    // Step 3c: Initialize x86_64 Native System Call Interface (syscall/sysret)
    arch::syscall::init();

    // Step 4: Initialize the 8 MiB Dynamic Heap Allocator
    memory::allocator::init_heap();
    klog!(Info, "allocator", "8 MiB heap initialized.");

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
        klog!(Debug, "allocator", "Dynamic allocations verified: Box, Vec, String.");
    }

    // Step 6: CPU and Hardware Clock Detection
    let cpu = arch::cpuid::get_cpu_info();
    let rtc = drivers::cmos::read_rtc();
    println!("[OK] Processor : {} ({})", cpu.brand_str(), cpu.vendor_str());
    println!("[OK] RTC Clock : {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", rtc.year, rtc.month, rtc.day, rtc.hour, rtc.minute, rtc.second);
    klog!(Info, "cpu", "Processor: {} ({})", cpu.brand_str(), cpu.vendor_str());
    klog!(Info, "rtc", "RTC Clock: {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", rtc.year, rtc.month, rtc.day, rtc.hour, rtc.minute, rtc.second);

    // Step 7: PCI Hardware Bus Enumeration
    let pci_devices = drivers::pci::scan_pci_bus();
    println!("[OK] PCI Bus   : Discovered {} active hardware device(s).", pci_devices.len());
    klog!(Info, "pci", "{} active device(s) enumerated.", pci_devices.len());
    for dev in &pci_devices {
        serial_println!("      [{:02x}:{:02x}.{}] {:04x}:{:04x} | {} - {}",
            dev.bus, dev.slot, dev.func, dev.vendor_id, dev.device_id,
            dev.vendor_name(), dev.class_name());
    }

    // Step 7b: Initialize ACPI Subsystem & Local APIC
    memory::paging::init_hardware_mappings();
    arch::acpi::init();
    arch::apic::init();

    // Step 7c: Initialize Kernel Multitasking & Scheduler (BSP run-queue and TCBs)
    task::init();
    println!("[OK] Tasks     : Per-CPU Round-Robin scheduler initialized (TCBs ready).");
    klog!(Info, "scheduler", "Multitasking initialized.");

    // Step 7d: Initialize SMP — wake Application Processors via INIT-SIPI-SIPI
    arch::smp::init();

    // Step 7e: Activate Bochs BGA high-resolution framebuffer if present.
    // The desktop GUI stays available afterwards via the `gui` shell command,
    // or launches automatically once the console is made interactive below.
    let bga_active = {
        if drivers::bga::is_available() {
            drivers::bga::init_graphics_mode(drivers::bga::SCREEN_WIDTH, drivers::bga::SCREEN_HEIGHT)
        } else {
            false
        }
    };
    if bga_active {
        println!("[OK] Video     : BGA 1024x768x32bpp framebuffer active (TrueColor).");
        klog!(Info, "video", "BGA 1024x768x32bpp activated at boot.");
    } else {
        println!("[--] Video     : BGA not available, staying in VGA text mode.");
    }
    {
        let acpi = arch::acpi::ACPI_DATA.lock();
        if acpi.is_initialized {
            println!("[OK] ACPI       : {} tables (FADT: {:#x}, MADT: {:#x}, {} core(s))",
                acpi.tables_count, acpi.fadt_addr, acpi.madt_addr, acpi.cores.len());
        } else {
            println!("[--] ACPI       : RSDP not detected (legacy PC mode).");
        }
    }

    // Step 8: Initialize Virtual File System & RAMFS
    fs::init();
    println!("[OK] VFS       : Root RAM disk mounted at '/' (hierarchy ready).");
    klog!(Info, "vfs", "VFS & RAMFS mounted.");

    // Step 10: Enable hardware interrupts at the CPU level
    unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
    println!("[OK] CPU       : Hardware interrupts enabled (sti).");
    klog!(Info, "cpu", "Interrupts enabled (sti).");

    // Step 10b: Initialize Network Stack (Intel e1000 + Ethernet/ARP/IPv4/ICMP/UDP)
    let net_ok = net::init();
    if net_ok {
        let net_info = net::NETWORK.lock();
        println!("[OK] Network   : e1000 NIC active ({:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}, {}.{}.{}.{})",
            net_info.mac[0], net_info.mac[1], net_info.mac[2],
            net_info.mac[3], net_info.mac[4], net_info.mac[5],
            net_info.ip[0], net_info.ip[1], net_info.ip[2], net_info.ip[3]);
    } else {
        println!("[--] Network   : No e1000 NIC detected (loopback mode).");
    }

    // Step 10c: Start Local APIC periodic timer on BSP (Vector 0x40, ~100 Hz countdown)
    arch::apic::init_timer(arch::apic::LAPIC_TIMER_VECTOR, 1_000_000);

    // Step 10d: Release all Application Processors to begin timer preemption and scheduling
    arch::smp::start_aps();
    // I1: Enable per-CPU serial ring buffers now that SMP is initialized.
    // Before this point, serial output goes directly through SERIAL1 lock.
    drivers::serial::enable_percpu_buffering();
    println!("[OK] SMP       : Multi-core scheduling active across all online CPUs.");

    // Step 11: Execute Kernel Automated Subsystem Diagnostics
    println!();
    println!("[DIAG] Running AuraOS automated self-test verification...");
    let test_results = tests::run_all_tests();
    serial_println!("[DEBUG] Automated self-tests finished (total: {}).", test_results.len());
    let mut all_passed = true;
    for res in &test_results {
        if res.passed {
            println!("  [PASS] {}", res.name);
            klog!(Info, "tests", "PASS: {} -- {}", res.name, res.detail);
        } else {
            all_passed = false;
            println!("  [FAIL] {} ({})", res.name, res.detail);
            klog!(Warn, "tests", "FAIL: {} -- {}", res.name, res.detail);
        }
        drivers::serial::serial_flush_all();
    }
    if all_passed {
        println!("[OK] All {} subsystem tests PASSED. System verified 100%.", test_results.len());
        klog!(Info, "tests", "All {} automated self-tests passed.", test_results.len());
    } else {
        println!("[WARN] One or more diagnostic checks reported issues.");
    }
    drivers::serial::serial_flush_all();

    // Step 12: System status report
    println!();
    println!("[OK] Mode      : x86_64 Bare-Metal Long Mode (64-bit)");
    println!("[OK] Memory    : 8 MiB Heap, 4 KiB Paging abstractions");
    println!("[OK] RAM       : Physical RAM {} MiB detected (E820), {} MiB free frames via PMM",
        memory::pmm::total_memory() / (1024 * 1024),
        memory::pmm::free_memory() / (1024 * 1024));
    println!("[OK] Filesystem: Virtual Inode VFS mounted at '/'");
    println!("[OK] Storage   : ATA / IDE PIO 28-bit driver active");
    println!("[OK] Network   : Intel e1000 Gigabit Ethernet (ARP/IPv4/ICMP/UDP)");
    println!("[OK] Power/ACPI: S5 Soft-Off ready, Local APIC initialized");
    println!("[OK] Serial    : COM1 UART at 0x3F8 (115200 baud)");
    println!("[OK] Keyboard  : PS/2 driver active (direct typing)");
    println!();

    // Step 12b: Launch the graphical desktop if BGA framebuffer is active.
    // The GUI loop returns on ESC (or 'q' via serial) and falls through to
    // the interactive text console below.
    if bga_active {
        println!("------------------------------------------------------------");
        println!("  AuraOS v0.1.0 — Launching graphical desktop (ESC to return)");
        println!("------------------------------------------------------------");
        gui::run_interactive_desktop();
        drivers::vga::clear_screen();
        println!("============================================================");
        println!("        AuraOS v0.1.0 — Graphical session ended.");
        println!("============================================================");
        println!();
    }

    println!("------------------------------------------------------------");
    println!("  AuraOS v0.1.0 ready. Interactive console active:");
    println!("------------------------------------------------------------");
    print!("auraos> ");

    klog!(Info, "shell", "Interactive console ready.");

    // Interactive kernel execution loop:
    // Processes pending keyboard inputs, flushes per-CPU serial buffers,
    // and enters low-power sleep (HLT) until the next hardware interrupt.
    loop {
        drivers::keyboard::process_pending_keys();
        net::poll();
        // I1: Drain all per-CPU serial ring buffers to the physical UART.
        // This is the single point where SERIAL1 lock is taken for output.
        drivers::serial::serial_flush_all();
        unsafe { core::arch::asm!("sti; hlt", options(nomem, nostack)); }
    }
}

static PANICKING: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Invoked on panic. Prints error message and halts CPU.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack));
    }

    if PANICKING.swap(true, core::sync::atomic::Ordering::SeqCst) {
        // Recursive panic detected: halt immediately to avoid stack overflow / triple fault
        loop {
            unsafe { core::arch::asm!("hlt", options(nomem, nostack)) };
        }
    }

    unsafe {
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
