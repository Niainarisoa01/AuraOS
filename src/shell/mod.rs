/// ============================================================================
/// AuraOS Interactive Shell — Bare-Metal Terminal
/// ============================================================================
///
/// Manages the line input buffer and parses/executes user commands
/// entered via the PS/2 keyboard.

use crate::drivers::vga::clear_screen;
use crate::arch::io::outb;
use crate::sync::Spinlock;

const BUFFER_MAX: usize = 128;

pub struct Shell {
    buffer: [u8; BUFFER_MAX],
    length: usize,
}

impl Shell {
    pub const fn new() -> Self {
        Self {
            buffer: [0; BUFFER_MAX],
            length: 0,
        }
    }

    /// Appends a typed ASCII character to the line buffer and prints it.
    pub fn push_char(&mut self, c: u8) {
        if self.length < BUFFER_MAX - 1 {
            self.buffer[self.length] = c;
            self.length += 1;
            crate::print!("{}", c as char);
        }
    }

    /// Erases the last character from the buffer and screen.
    pub fn backspace(&mut self) {
        if self.length > 0 {
            self.length -= 1;
            crate::drivers::vga::backspace();
        }
    }

    /// Submits the current line (Enter key) and executes the parsed command.
    pub fn enter(&mut self) {
        crate::println!();

        if self.length > 0 {
            let len = self.length;
            self.length = 0;
            if let Ok(line) = core::str::from_utf8(&self.buffer[..len]) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    Self::execute(trimmed);
                }
            }
        }

        crate::print!("auraos> ");
    }

    /// Parses and executes the requested command.
    fn execute(cmd: &str) {
        let mut parts = cmd.split_whitespace();
        let command = parts.next().unwrap_or("");

        match command {
            "help" => {
                crate::println!("Available commands in AuraOS v0.1.0:");
                crate::println!("  help        - Display this help message");
                crate::println!("  clear       - Clear the VGA screen");
                crate::println!("  info        - Display system and CPU status");
                crate::println!("  cpu         - Display detailed CPUID processor features");
                crate::println!("  pci / lspci - Enumerate and inspect PCI hardware devices");
                crate::println!("  tasks / ps  - Display kernel tasks, states, and stack pointers");
                crate::println!("  yield       - Cooperatively yield CPU slice to background worker");
                crate::println!("  time / date - Display hardware RTC calendar date & time");
                crate::println!("  mem         - Display physical/virtual memory & heap usage");
                crate::println!("  serial <msg>- Send a message to the COM1 serial port");
                crate::println!("  ticks       - Display system timer ticks (PIT IRQ0)");
                crate::println!("  manifesto   - AuraOS 10-year roadmap & architecture vision");
                crate::println!("  ls [path]   - List directory contents (files & folders)");
                crate::println!("  cd <path>   - Change current working directory");
                crate::println!("  pwd         - Print current working directory");
                crate::println!("  cat <file>  - Display contents of a text file");
                crate::println!("  touch <file>- Create a new empty file in RAMFS");
                crate::println!("  mkdir <dir> - Create a new directory");
                crate::println!("  write <f> <t> Write text content into a file");
                crate::println!("  rm <name>   - Remove a file or directory entry");
                crate::println!("  readsec <lba> Read 512-byte raw disk sector via ATA PIO");
                crate::println!("  test        - Run automated kernel subsystem self-tests");
                crate::println!("  reboot      - Reset and restart the computer");
                crate::println!("  shutdown    - Power off the system / virtual machine");
                crate::println!("  halt        - Put the CPU into deep sleep");
            }

            "clear" => {
                clear_screen();
            }

            "info" => {
                let cr3 = crate::memory::paging::read_cr3();
                let cpu = crate::arch::cpuid::get_cpu_info();
                let rtc = crate::drivers::cmos::read_rtc();
                crate::println!("============================================================");
                crate::println!("                    AURA OPERATING SYSTEM                   ");
                crate::println!("============================================================");
                crate::println!("  Version        : v0.1.0 (Bare-Metal Prototype)");
                crate::println!("  Architecture   : x86_64 Long Mode (64-bit pure Rust)");
                crate::println!("  Processor      : {} ({})", cpu.brand_str(), cpu.vendor_str());
                crate::println!("  RTC Clock      : {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", rtc.year, rtc.month, rtc.day, rtc.hour, rtc.minute, rtc.second);
                crate::println!("  Memory Model   : 4-Level Paging (PML4 at {:#x})", cr3.as_u64());
                crate::println!("  Heap Allocator : 512 KiB Linked List Allocator (Dynamic)");
                crate::println!("  Serial Port    : COM1 (UART 16550 at 0x3F8, 115200 baud)");
                crate::println!("  Video Driver   : VGA Text Mode 80x25 (Buffer 0xb8000)");
                crate::println!("  IRQ Controller : Dual 8259 PIC remapped (32..47)");
                crate::println!("  Protection     : 64-bit GDT + 256-entry IDT");
                crate::println!("  Keyboard       : PS/2 Driver (Set 1 Make/Break)");
                crate::println!("  Timer Ticks    : {}", crate::arch::idt::ticks());
                crate::println!("============================================================");
            }

            "cpu" => {
                let cpu = crate::arch::cpuid::get_cpu_info();
                let cycles = crate::arch::cpuid::rdtsc();
                crate::println!("--- AURAOS CPUID HARDWARE REPORT ---");
                crate::println!("  Brand String   : {}", cpu.brand_str());
                crate::println!("  Vendor ID      : {}", cpu.vendor_str());
                crate::println!("  TSC Cycles     : {}", cycles);
                crate::println!("  Feature Flags  :");
                crate::println!("    * FPU        : {}", if cpu.has_fpu { "Supported" } else { "No" });
                crate::println!("    * TSC        : {}", if cpu.has_tsc { "Supported" } else { "No" });
                crate::println!("    * APIC       : {}", if cpu.has_apic { "Supported" } else { "No" });
                crate::println!("    * SSE / SSE2 : {}", if cpu.has_sse && cpu.has_sse2 { "Supported" } else { "No" });
                crate::println!("    * SSE3       : {}", if cpu.has_sse3 { "Supported" } else { "No" });
                crate::println!("    * AVX        : {}", if cpu.has_avx { "Supported" } else { "No" });
                crate::println!("    * RDRAND     : {}", if cpu.has_rdrand { "Supported" } else { "No" });
                crate::println!("------------------------------------");
            }

            "pci" | "lspci" => {
                let devices = crate::drivers::pci::scan_pci_bus();
                crate::println!("--- DISCOVERED PCI BUS DEVICES ({}) ---", devices.len());
                if devices.is_empty() {
                    crate::println!("  No PCI devices found on scanned buses.");
                } else {
                    for dev in &devices {
                        crate::println!("[{:02x}:{:02x}.{}] {:04x}:{:04x} | {} - {}",
                            dev.bus, dev.slot, dev.func, dev.vendor_id, dev.device_id,
                            dev.vendor_name(), dev.class_name());
                    }
                }
                crate::println!("---------------------------------------");
            }

            "tasks" | "ps" => {
                let sched = crate::task::SCHEDULER.lock();
                let count = crate::task::SENTINEL_HEARTBEATS.load(core::sync::atomic::Ordering::SeqCst);
                crate::println!("--- AURAOS KERNEL TASK SCHEDULER ---");
                crate::println!("PID  NAME               STATE     TICKS      RSP");
                for task in &sched.tasks {
                    crate::println!("{:<4} {:<18} {:<9} {:<10} {:#x}",
                        task.id, task.name, task.state.as_str(), task.ticks, task.rsp);
                }
                crate::println!("Sentinel Heartbeats sent: {}", count);
                crate::println!("-----------------------------------");
            }

            "yield" => {
                crate::println!("Yielding CPU time slice to background tasks...");
                crate::task::yield_now();
                crate::println!("Resumed in kernel shell!");
            }

            "time" | "date" => {
                let rtc = crate::drivers::cmos::read_rtc();
                crate::println!("Hardware RTC Clock: {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
                    rtc.year, rtc.month, rtc.day, rtc.hour, rtc.minute, rtc.second);
            }

            "mem" => {
                let free = crate::memory::allocator::free_memory();
                let used = crate::memory::allocator::used_memory();
                let cr3 = crate::memory::paging::read_cr3();
                crate::println!("--- AURAOS MEMORY SUBSYSTEM ---");
                crate::println!("  Paging Model : x86_64 4-Level Paging (4 KiB pages)");
                crate::println!("  Active PML4  : CR3 base = {:#x}", cr3.as_u64());
                crate::println!("  Heap Strategy: Linked List Allocator (Coalescing)");
                crate::println!("  Heap Total   : {} KiB ({} bytes)", crate::memory::allocator::HEAP_SIZE / 1024, crate::memory::allocator::HEAP_SIZE);
                crate::println!("  Heap Used    : {} bytes", used);
                crate::println!("  Heap Free    : {} bytes ({} KiB)", free, free / 1024);
                crate::println!("-------------------------------");
            }

            "serial" => {
                let msg = cmd.strip_prefix("serial").unwrap_or("").trim();
                if msg.is_empty() {
                    crate::println!("  Usage: serial <message to send to COM1>");
                } else {
                    crate::serial_println!("[SERIAL COM1] {}", msg);
                    crate::println!("  [OK] Sent to COM1: \"{}\"", msg);
                }
            }

            "ticks" => {
                crate::println!("System timer (IRQ0): {} ticks", crate::arch::idt::ticks());
            }

            "manifesto" | "manifeste" => {
                crate::println!("--- AURAOS: 10-YEAR VISION & CORE PRINCIPLES ---");
                crate::println!("1. 100% Pure Rust bare-metal: compile-time memory safety without GC.");
                crate::println!("2. Ultra-lightweight: complete core kernel < 1 MB.");
                crate::println!("3. Modular microkernel architecture with isolated Ring 3 drivers.");
                crate::println!("4. Fluid vector graphics compositor with modern typography.");
                crate::println!("------------------------------------------------");
            }

            "calc" => {
                let expr = parts.next().unwrap_or("");
                if let Some(pos) = expr.find('+') {
                    let left_str = &expr[..pos];
                    let right_str = &expr[pos + 1..];
                    if let (Ok(a), Ok(b)) = (parse_u64(left_str), parse_u64(right_str)) {
                        crate::println!("  {} + {} = {}", a, b, a + b);
                    } else {
                        crate::println!("  Error: invalid numbers. Example: calc 15+27");
                    }
                } else {
                    crate::println!("  Usage: calc <number>+<number>  (Example: calc 123+456)");
                }
            }

            "pwd" => {
                let vfs = crate::fs::VFS.lock();
                let path = vfs.get_path(vfs.current_inode);
                crate::println!("{}", path);
            }

            "cd" => {
                let target = parts.next().unwrap_or("/");
                let mut vfs = crate::fs::VFS.lock();
                match vfs.resolve_path(target) {
                    Ok(node_id) => {
                        if vfs.inodes[node_id].is_dir() {
                            vfs.current_inode = node_id;
                        } else {
                            crate::println!("cd: not a directory: {}", target);
                        }
                    }
                    Err(err) => crate::println!("cd: {}: {}", target, err),
                }
            }

            "ls" => {
                let target = parts.next().unwrap_or("");
                let vfs = crate::fs::VFS.lock();
                let dir_id = if target.is_empty() {
                    vfs.current_inode
                } else {
                    match vfs.resolve_path(target) {
                        Ok(id) => id,
                        Err(err) => {
                            crate::println!("ls: {}: {}", target, err);
                            return;
                        }
                    }
                };

                match vfs.list_directory(dir_id) {
                    Ok(entries) => {
                        crate::println!("TYPE    SIZE (BYTES)  NAME");
                        crate::println!("----    ------------  ----");
                        for entry in &entries {
                            let type_str = if entry.is_dir { "<DIR> " } else { "<FILE>" };
                            crate::println!("{}  {:<12}  {}", type_str, entry.size, entry.name);
                        }
                        crate::println!("Total entries: {}", entries.len());
                    }
                    Err(err) => crate::println!("ls: {}", err),
                }
            }

            "cat" => {
                let path = parts.next().unwrap_or("");
                if path.is_empty() {
                    crate::println!("Usage: cat <file_path>");
                } else {
                    let vfs = crate::fs::VFS.lock();
                    match vfs.resolve_path(path) {
                        Ok(file_id) => match vfs.read_file(file_id) {
                            Ok(bytes) => {
                                if let Ok(text) = core::str::from_utf8(bytes) {
                                    crate::println!("{}", text);
                                } else {
                                    crate::println!("<Binary content: {} bytes>", bytes.len());
                                }
                            }
                            Err(err) => crate::println!("cat: {}: {}", path, err),
                        },
                        Err(err) => crate::println!("cat: {}: {}", path, err),
                    }
                }
            }

            "touch" => {
                let file_name = parts.next().unwrap_or("");
                if file_name.is_empty() {
                    crate::println!("Usage: touch <file_name>");
                } else {
                    let mut vfs = crate::fs::VFS.lock();
                    let curr = vfs.current_inode;
                    match vfs.create_file_at(curr, file_name, b"") {
                        Ok(_) => crate::println!("File created: {}", file_name),
                        Err(err) => crate::println!("touch: {}: {}", file_name, err),
                    }
                }
            }

            "mkdir" => {
                let dir_name = parts.next().unwrap_or("");
                if dir_name.is_empty() {
                    crate::println!("Usage: mkdir <directory_name>");
                } else {
                    let mut vfs = crate::fs::VFS.lock();
                    let curr = vfs.current_inode;
                    match vfs.mkdir_at(curr, dir_name) {
                        Ok(_) => crate::println!("Directory created: {}", dir_name),
                        Err(err) => crate::println!("mkdir: {}: {}", dir_name, err),
                    }
                }
            }

            "write" => {
                if let Some(rest) = cmd.strip_prefix("write ") {
                    let rest = rest.trim_start();
                    if let Some(space_idx) = rest.find(' ') {
                        let filename = &rest[..space_idx];
                        let content = &rest[space_idx + 1..];
                        let mut vfs = crate::fs::VFS.lock();
                        let curr = vfs.current_inode;
                        match vfs.create_file_at(curr, filename, content.as_bytes()) {
                            Ok(_) => crate::println!("Wrote {} bytes to '{}'", content.len(), filename),
                            Err(err) => crate::println!("write: {}: {}", filename, err),
                        }
                    } else {
                        crate::println!("Usage: write <filename> <content...>");
                    }
                } else {
                    crate::println!("Usage: write <filename> <content...>");
                }
            }

            "rm" => {
                let name = parts.next().unwrap_or("");
                if name.is_empty() {
                    crate::println!("Usage: rm <file_or_dir_name>");
                } else {
                    let mut vfs = crate::fs::VFS.lock();
                    match vfs.resolve_path(name) {
                        Ok(target_id) => match vfs.remove_entry(target_id) {
                            Ok(_) => crate::println!("Removed: {}", name),
                            Err(err) => crate::println!("rm: {}: {}", name, err),
                        },
                        Err(err) => crate::println!("rm: {}: {}", name, err),
                    }
                }
            }

            "readsec" => {
                let lba_str = parts.next().unwrap_or("0");
                let lba = parse_u64(lba_str).unwrap_or(0) as u32;
                let mut buf = [0u8; crate::drivers::ata::SECTOR_SIZE];
                match crate::drivers::ata::read_sector(lba, &mut buf) {
                    Ok(()) => {
                        crate::println!("--- ATA SECTOR {} (FIRST 32 BYTES) ---", lba);
                        for chunk in buf[..32].chunks(16) {
                            for b in chunk {
                                crate::print!("{:02x} ", b);
                            }
                            crate::println!();
                        }
                        crate::println!("--------------------------------------");
                    }
                    Err(err) => crate::println!("readsec: error reading sector {}: {}", lba, err),
                }
            }

            "test" | "selftest" => {
                crate::println!("--- AURAOS AUTOMATED SUBSYSTEM SELF-TESTS ---");
                let results = crate::tests::run_all_tests();
                let mut all_ok = true;
                for res in &results {
                    if res.passed {
                        crate::println!("  [PASS] {}", res.name);
                        crate::println!("         Detail: {}", res.detail);
                    } else {
                        all_ok = false;
                        crate::println!("  [FAIL] {}", res.name);
                        crate::println!("         Detail: {}", res.detail);
                    }
                }
                if all_ok {
                    crate::println!("Result: All {} tests passed successfully! System 100% OK.", results.len());
                } else {
                    crate::println!("Result: One or more tests failed!");
                }
                crate::println!("----------------------------------------------");
            }

            "reboot" => {
                crate::println!("Restarting AuraOS...");
                unsafe {
                    // Pulse reset line via 8042 keyboard controller (port 0x64, command 0xFE)
                    outb(0x64, 0xFE);
                }
            }

            "shutdown" | "poweroff" => {
                crate::println!("Powering off system via ACPI...");
                unsafe {
                    // Modern QEMU ACPI poweroff
                    crate::arch::io::outw(0x604, 0x2000);
                    // Legacy Bochs / older QEMU
                    crate::arch::io::outw(0xB004, 0x2000);
                    // VirtualBox ACPI shutdown
                    crate::arch::io::outw(0x4004, 0x3400);
                    // Cloud Hypervisor
                    crate::arch::io::outw(0x600, 0x34);

                    crate::println!("ACPI poweroff triggered. Halting CPU.");
                    loop {
                        core::arch::asm!("cli; hlt", options(nomem, nostack, preserves_flags));
                    }
                }
            }

            "halt" => {
                crate::println!("AuraOS halted. CPU entering deep sleep state.");
                loop {
                    unsafe {
                        core::arch::asm!("cli; hlt", options(nomem, nostack, preserves_flags));
                    }
                }
            }

            _ => {
                crate::println!("Unknown command: '{}'. Type 'help' for a list.", cmd);
            }
        }
    }
}

/// Parses an ASCII numeric string slice into a u64 without standard library.
fn parse_u64(s: &str) -> Result<u64, ()> {
    let s = s.trim();
    if s.is_empty() {
        return Err(());
    }
    let mut acc: u64 = 0;
    for b in s.bytes() {
        if b.is_ascii_digit() {
            acc = acc.checked_mul(10).ok_or(())?;
            acc = acc.checked_add((b - b'0') as u64).ok_or(())?;
        } else {
            return Err(());
        }
    }
    Ok(acc)
}

/// Global singleton instance of the AuraOS Shell, synchronized with a Spinlock.
pub static SHELL: Spinlock<Shell> = Spinlock::new(Shell::new());
