//! ============================================================================
//! AuraOS Interactive Shell — Bare-Metal Terminal
//! ============================================================================
//!
//! Manages the line input buffer and parses/executes user commands
//! entered via the PS/2 keyboard.

use crate::drivers::vga::clear_screen;
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
    #[allow(dead_code)]
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
    pub fn execute(cmd: &str) {
        let mut parts = cmd.split_whitespace();
        let command = parts.next().unwrap_or("");

        match command {
            "help" => {
                crate::println!("Available commands in AuraOS v0.1.0:");
                crate::println!("  help        - Display this help message");
                crate::println!("  clear       - Clear the VGA screen");
                crate::println!("  info        - Display system and CPU status");
                crate::println!("  sysinfo     - Display PIT, TSS, and syscall subsystem metrics");
                crate::println!("  cpu         - Display detailed CPUID processor features");
                crate::println!("  pci / lspci - Enumerate and inspect PCI hardware devices");
                crate::println!("  tasks / ps  - Display kernel tasks, states, and stack pointers");
                crate::println!("  ipc [cmd]   - Inter-Process Communication (status, send, recv)");
                crate::println!("  userdemo    - Spawn and benchmark Ring 3 user process");
                crate::println!("  exec <path> - Execute a 64-bit ELF binary in Ring 3 userspace");
                crate::println!("  elfinfo <p> - Inspect 64-bit ELF executable header & segments");
                crate::println!("  sleep <ms>  - Put current task to sleep for N milliseconds");
                crate::println!("  spawn <name>- Spawn a background worker task");
                crate::println!("  kill <id>   - Terminate a task by its numeric ID");
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
                crate::println!("  writesec <lba> <t> Write text into raw ATA disk sector");
                crate::println!("  formatfat [lba] [sec] Format a FAT32 partition on ATA disk");
                crate::println!("  mountfat [lba]        Mount an existing FAT32 volume");
                crate::println!("  fatinfo               Display mounted FAT32 volume metrics");
                crate::println!("  fatls [path]          List files/folders on FAT32 volume");
                crate::println!("  fatcat <path>         Display contents of a FAT32 file");
                crate::println!("  fatwrite <p> <text>   Write text to a persistent FAT32 file");
                crate::println!("  fatmkdir <path>       Create a directory on FAT32 volume");
                crate::println!("  fatrm <path>          Delete a file from FAT32 volume");
                crate::println!("  gui / desktop- Render and benchmark macOS-style Desktop UI");
                crate::println!("  ifconfig    - Display network interface configuration");
                crate::println!("  ping <ip>   - Send ICMP Echo Request (ping) to an IP address");
                crate::println!("  arp         - Display the ARP translation cache table");
                crate::println!("  udpsend <ip> <port> <text> Send a UDP datagram");
                crate::println!("  netstat     - Display network interface statistics");
                crate::println!("  acpi        - Display ACPI hardware description tables & power info");
                crate::println!("  cores / smp - Display enumerated CPU cores & Local APIC metrics");
                crate::println!("  test        - Run automated kernel subsystem self-tests");
                crate::println!("  reboot      - Reset and restart the computer");
                crate::println!("  shutdown    - Power off the system / virtual machine");
                crate::println!("  halt        - Put the CPU into deep sleep");
            }

            "gui" | "desktop" => {
                crate::println!("--- AURAOS GRAPHICAL DESKTOP ---");
                crate::println!("  Switching to BGA 1024x768x32bpp TrueColor mode...");
                crate::println!("  Press ESC to return to the text shell.");
                crate::println!("---");
                crate::gui::run_interactive_desktop();
                // Returned from GUI — reinitialize VGA text
                crate::drivers::vga::clear_screen();
                crate::println!("============================================================");
                crate::println!("        AuraOS v0.1.0 — Returned to Text Console            ");
                crate::println!("============================================================");
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
                crate::println!("  Heap Allocator : 8 MiB Linked List Allocator (Dynamic)");
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

            "acpi" => {
                let acpi = crate::arch::acpi::ACPI_DATA.lock();
                crate::println!("--- AURAOS ACPI POWER & CONFIGURATION TABLES ---");
                if !acpi.is_initialized {
                    crate::println!("  ACPI status: Not detected or uninitialized.");
                } else {
                    let oem_str = core::str::from_utf8(&acpi.oem_id).unwrap_or("???");
                    crate::println!("  OEM ID         : {}", oem_str);
                    crate::println!("  RSDP Address   : {:#x}", acpi.rsdp_addr);
                    if acpi.xsdt_addr != 0 {
                        crate::println!("  XSDT Address   : {:#x} (64-bit)", acpi.xsdt_addr);
                    } else {
                        crate::println!("  RSDT Address   : {:#x} (32-bit)", acpi.rsdt_addr);
                    }
                    crate::println!("  Tables Count   : {}", acpi.tables_count);
                    crate::println!("  FADT Address   : {:#x}", acpi.fadt_addr);
                    crate::println!("  DSDT Address   : {:#x}", acpi.dsdt_addr);
                    crate::println!("  MADT Address   : {:#x}", acpi.madt_addr);
                    crate::println!("  SMI Command    : {:#x} (ACPI Enable: {:#x})", acpi.smi_cmd, acpi.acpi_enable);
                    crate::println!("  PM1a Control   : {:#x}", acpi.pm1a_cnt_blk);
                    if acpi.pm1b_cnt_blk != 0 {
                        crate::println!("  PM1b Control   : {:#x}", acpi.pm1b_cnt_blk);
                    }
                    crate::println!("  Soft-Off (_S5) : {}", if acpi.has_s5 { "Discovered" } else { "Not found in DSDT" });
                    if acpi.has_s5 {
                        crate::println!("    * SLP_TYPa   : {:#x}", acpi.slp_typa);
                        crate::println!("    * SLP_TYPb   : {:#x}", acpi.slp_typb);
                    }
                }
                crate::println!("------------------------------------------------");
            }

            "cores" | "smp" => {
                let acpi = crate::arch::acpi::ACPI_DATA.lock();
                let lapic = crate::arch::apic::LOCAL_APIC.lock();
                let online_cpus = crate::arch::smp::cpu_count();
                crate::println!("--- AURAOS MULTIPROCESSOR (SMP) & APIC REPORT ---");
                crate::println!("  Online CPUs    : {} / {} (MADT)", online_cpus, acpi.cores.len());
                crate::println!("  Local APIC Base: {:#x}", lapic.base_addr);
                crate::println!("  Bootstrap CPU  : APIC ID {}", lapic.apic_id);
                crate::println!("  LAPIC Version  : {:#x}", lapic.version);
                crate::println!("  Software Enable: {}", if lapic.is_enabled { "Yes (SVR active)" } else { "No" });
                crate::println!("  Discovered Cores (MADT): {}", acpi.cores.len());
                let per_cpu = unsafe { &*crate::arch::smp::PER_CPU.get() };
                for (idx, core) in acpi.cores.iter().enumerate() {
                    let smp_online = if idx < crate::arch::smp::MAX_CPUS {
                        per_cpu[idx].online.load(core::sync::atomic::Ordering::Relaxed)
                    } else {
                        false
                    };
                    crate::println!(
                        "    [Core {}] ACPI ProcID: {}, APIC ID: {}, MADT: {}, SMP: {}",
                        idx,
                        core.processor_id,
                        core.apic_id,
                        if core.is_enabled { "Enabled" } else { "Disabled" },
                        if smp_online { "ONLINE" } else { "offline" }
                    );
                }
                if !acpi.io_apics.is_empty() {
                    crate::println!("  Discovered I/O APICs: {}", acpi.io_apics.len());
                    for io in &acpi.io_apics {
                        crate::println!("    [I/O APIC {}] MMIO Address: {:#x}, GSI Base: {}", io.id, io.address, io.gsi_base);
                    }
                }
                crate::println!("-------------------------------------------------");
            }

            "sysinfo" => {
                crate::println!("=== AURAOS SUBSYSTEM DIAGNOSTICS & METRICS ===");
                // PIT 8254
                crate::println!("[PIT 8254 Timer]");
                crate::println!("  Target Frequency: {} Hz", crate::drivers::pit::TARGET_FREQUENCY);
                crate::println!("  Actual Frequency: {} Hz", crate::drivers::pit::actual_frequency());
                crate::println!("  Tick Interval   : {} ms", crate::drivers::pit::tick_interval_ms());
                crate::println!("  Elapsed Ticks   : {}", crate::arch::idt::ticks());

                // TSS
                crate::println!("[Task State Segment (TSS)]");
                let tss_active = crate::arch::gdt::TSS_LOADED.load(core::sync::atomic::Ordering::Relaxed);
                crate::println!("  Loaded          : {}", if tss_active { "ACTIVE (Ring 0 / IST enabled)" } else { "NOT LOADED" });
                crate::println!("  IST1 Stack Top  : {:#x}", crate::arch::gdt::tss_ist1());
                crate::println!("  RSP0 Stack Top  : {:#x}", crate::arch::gdt::tss_rsp0());

                // Syscall
                crate::println!("[Native Syscall Interface]");
                let sys_active = crate::arch::syscall::SYSCALL_CONFIGURED.load(core::sync::atomic::Ordering::Relaxed);
                let sys_calls = crate::arch::syscall::SYSCALL_COUNT.load(core::sync::atomic::Ordering::Relaxed);
                crate::println!("  Status          : {}", if sys_active { "CONFIGURED (Fast MSR syscall/sysret)" } else { "DISABLED" });
                crate::println!("  Invocations     : {}", sys_calls);
                crate::println!("  LSTAR Target    : {:#x}", crate::arch::syscall::read_lstar());

                // Heap Memory
                crate::println!("[Heap Memory]");
                crate::println!("  Used Memory     : {} bytes", crate::memory::allocator::used_memory());
                crate::println!("==============================================");
            }

            "tasks" | "ps" => {
                let online = crate::arch::smp::cpu_count();
                let num_cpus = online.clamp(1, crate::arch::smp::MAX_CPUS);
                let count = crate::task::SENTINEL_HEARTBEATS.load(core::sync::atomic::Ordering::SeqCst);
                crate::println!("--- AURAOS KERNEL TASK SCHEDULER (SMP: {} CPU(s) online) ---", online);
                crate::println!("CPU  PID  NAME               RING   STATE           PRIO QUANTUM    TICKS      RSP");
                for cpu_id in 0..num_cpus {
                    let sched = crate::task::CPU_SCHEDULERS[cpu_id].lock();
                    for task in &sched.tasks {
                        let state_buf;
                        let state_str = match task.state {
                            crate::task::TaskState::Ready => "READY",
                            crate::task::TaskState::Running => "RUNNING",
                            crate::task::TaskState::Sleeping(w) => {
                                state_buf = alloc::format!("SLEEP({})", w);
                                &state_buf
                            }
                            crate::task::TaskState::Dead => "DEAD",
                        };
                        let ring_str = if task.is_user { "RING 3" } else { "RING 0" };
                        crate::println!("{:<4} {:<4} {:<18} {:<6} {:<15} {:<4} {}/{:<6} {:<10} {:#x}",
                            cpu_id, task.id, task.name, ring_str, state_str, task.priority, task.quantum_remaining, task.quantum, task.ticks, task.rsp);
                    }
                }
                crate::println!("Sentinel Heartbeats sent: {}", count);
                crate::println!("------------------------------------------------------------");
            }

            "ipc" => {
                let sub = parts.next().unwrap_or("status");
                match sub {
                    "send" => {
                        let target_str = parts.next().unwrap_or("0");
                        let target = parse_u64(target_str).unwrap_or(0) as usize;
                        let text_parts: alloc::vec::Vec<&str> = parts.collect();
                        let text = text_parts.join(" ");
                        let my_pid = {
                            let sched = crate::task::SCHEDULER.lock();
                            sched.tasks[sched.current].id
                        };
                        if crate::task::ipc::send_message(my_pid, target, 1, text.as_bytes()) {
                            crate::println!("IPC: Sent {} bytes to PID {}", text.len(), target);
                        } else {
                            crate::println!("IPC: Failed to send (mailbox full or target invalid)");
                        }
                    }
                    "recv" => {
                        let my_pid = {
                            let sched = crate::task::SCHEDULER.lock();
                            sched.tasks[sched.current].id
                        };
                        if let Some(msg) = crate::task::ipc::receive_message(my_pid) {
                            let p_len = msg.length as usize;
                            let text = core::str::from_utf8(&msg.payload[..p_len]).unwrap_or("<binary data>");
                            crate::println!("IPC Message received from PID {}: '{}' (type {})", msg.sender, text, msg.msg_type);
                        } else {
                            crate::println!("IPC: Mailbox empty for PID {}", my_pid);
                        }
                    }
                    _ => {
                        let (sent, deliv, active) = crate::task::ipc::stats();
                        crate::println!("--- AURAOS IPC ROUTER STATUS ---");
                        crate::println!("  Total Messages Sent     : {}", sent);
                        crate::println!("  Total Messages Delivered: {}", deliv);
                        crate::println!("  Active Mailboxes        : {}", active);
                        crate::println!("  Commands: ipc send <pid> <text> | ipc recv");
                        crate::println!("--------------------------------");
                    }
                }
            }

            "userdemo" => {
                crate::println!("--- AURAOS RING 3 USER SPACE DEMO ---");
                crate::println!("  1. Allocating isolated per-process AddressSpace (PML4)...");
                if let Some(mut space) = crate::memory::user_space::AddressSpace::new() {
                    crate::println!("     [OK] User PML4 root allocated at {:#x}", space.pml4_phys().as_u64());

                    let user_code_virt = crate::memory::paging::VirtAddr(0x0000_0000_4000_0000);
                    let user_stack_top = crate::memory::paging::VirtAddr(0x0000_0000_8000_0000);

                    crate::println!("  2. Mapping User Code at {:#x} (USER_ACCESSIBLE)...", user_code_virt.as_u64());
                    // Minimal x86_64 machine code that performs syscalls in Ring 3:
                    // 1. mov rax, 39 (SYS_GETPID); syscall;
                    // 2. mov rax, 24 (SYS_YIELD); syscall;
                    // 3. mov rax, 1 (SYS_EXIT); xor rdi, rdi; syscall;
                    let code: [u8; 30] = [
                        0x48, 0xc7, 0xc0, 0x27, 0x00, 0x00, 0x00, // mov rax, 39 (SYS_GETPID)
                        0x0f, 0x05,                               // syscall
                        0x48, 0xc7, 0xc0, 0x18, 0x00, 0x00, 0x00, // mov rax, 24 (SYS_YIELD)
                        0x0f, 0x05,                               // syscall
                        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // mov rax, 1 (SYS_EXIT)
                        0x48, 0x31, 0xff,                         // xor rdi, rdi
                        0x0f, 0x05,                               // syscall
                    ];
                    let code_ok = space.allocate_user_code(user_code_virt, &code);
                    crate::println!("     [OK] User code mapped: {}", code_ok);

                    crate::println!("  3. Mapping User Stack at {:#x} (16 KiB, USER_ACCESSIBLE)...", user_stack_top.as_u64());
                    let stack_ok = space.allocate_user_stack(user_stack_top, 4);
                    crate::println!("     [OK] User stack mapped: {}", stack_ok);

                    crate::println!("  4. Validating CPU Ring 3 iretq frame (CS={:#x}, SS={:#x}, RFLAGS=0x202)...",
                        crate::arch::gdt::USER_CS, crate::arch::gdt::USER_DS);
                    let (cs_ok, ss_ok, rflags_ok) = crate::arch::ring3::validate_ring3_frame(
                        crate::arch::gdt::USER_CS,
                        crate::arch::gdt::USER_DS,
                        0x202,
                    );
                    crate::println!("     [OK] Ring 3 selectors: CS={} ({:#x}), SS={} ({:#x}), IF={}",
                        cs_ok, crate::arch::gdt::USER_CS, ss_ok, crate::arch::gdt::USER_DS, rflags_ok);

                    crate::println!("  5. Spawning Ring 3 User Process in scheduler...");
                    let pid = crate::task::spawn_user(
                        "user-app-demo",
                        user_code_virt.as_u64(),
                        user_stack_top.as_u64(),
                        space.pml4_phys().as_u64(),
                    );
                    core::mem::forget(space); // Keep address space allocated for the task
                    crate::println!("     [OK] User process PID {} spawned in Ring 3!", pid);
                } else {
                    crate::println!("  [FAIL] Could not allocate user address space");
                }
                crate::println!("--------------------------------------");
            }

            "elfinfo" => {
                let raw_path = parts.next().unwrap_or("/bin/hello");
                let resolved_path = if !raw_path.starts_with('/') && !raw_path.contains('/') {
                    alloc::format!("/bin/{}", raw_path)
                } else {
                    alloc::string::String::from(raw_path)
                };

                let file_data = {
                    let vfs = crate::fs::VFS.lock();
                    match vfs.resolve_path(&resolved_path) {
                        Ok(node_id) => match vfs.read_file(node_id) {
                            Ok(cow) => Some(cow.to_vec()),
                            Err(e) => {
                                crate::println!("elfinfo: cannot read '{}': {}", resolved_path, e);
                                None
                            }
                        },
                        Err(e) => {
                            crate::println!("elfinfo: file not found '{}': {}", resolved_path, e);
                            None
                        }
                    }
                };

                if let Some(data) = file_data {
                    match crate::fs::elf::ElfBinary::parse(&data) {
                        Ok(elf) => {
                            crate::println!("--- ELF64 BINARY INFO: {} ---", resolved_path);
                            crate::println!("  Format:       ELF64 (64-bit AMD x86-64)");
                            crate::println!("  Type:         {:#x} (ET_EXEC=2, ET_DYN=3)", elf.header.elf_type);
                            crate::println!("  Entry Point:  {:#018x}", elf.entry_point());
                            crate::println!("  Program Hdr:  offset={:#x}, count={}, entry_size={}",
                                elf.header.phoff, elf.header.phnum, elf.header.phentsize);
                            crate::println!("  Section Hdr:  offset={:#x}, count={}, entry_size={}",
                                elf.header.shoff, elf.header.shnum, elf.header.shentsize);

                            if let Ok(phdrs) = elf.program_headers() {
                                crate::println!("  Segments ({} total):", phdrs.len());
                                for (i, p) in phdrs.iter().enumerate() {
                                    let type_str = match p.p_type {
                                        crate::fs::elf::PT_LOAD => "PT_LOAD",
                                        crate::fs::elf::PT_DYNAMIC => "PT_DYNAMIC",
                                        crate::fs::elf::PT_INTERP => "PT_INTERP",
                                        crate::fs::elf::PT_NOTE => "PT_NOTE",
                                        crate::fs::elf::PT_PHDR => "PT_PHDR",
                                        crate::fs::elf::PT_GNU_STACK => "PT_GNU_STACK",
                                        _ => "UNKNOWN",
                                    };
                                    let r = if (p.p_flags & crate::fs::elf::PF_R) != 0 { 'R' } else { '-' };
                                    let w = if (p.p_flags & crate::fs::elf::PF_W) != 0 { 'W' } else { '-' };
                                    let x = if (p.p_flags & crate::fs::elf::PF_X) != 0 { 'X' } else { '-' };
                                    crate::println!("    [{}] {:<12} vaddr={:#010x} filesz={} memsz={} [{}{}{}] align={:#x}",
                                        i, type_str, p.p_vaddr, p.p_filesz, p.p_memsz, r, w, x, p.p_align);
                                }
                            }
                            crate::println!("--------------------------------------");
                        }
                        Err(e) => {
                            crate::println!("elfinfo: failed to parse '{}': {}", resolved_path, e.as_str());
                        }
                    }
                }
            }

            "exec" => {
                let raw_path = parts.next().unwrap_or("/bin/hello");
                let resolved_path = if !raw_path.starts_with('/') && !raw_path.contains('/') {
                    alloc::format!("/bin/{}", raw_path)
                } else {
                    alloc::string::String::from(raw_path)
                };

                let file_data = {
                    let vfs = crate::fs::VFS.lock();
                    match vfs.resolve_path(&resolved_path) {
                        Ok(node_id) => match vfs.read_file(node_id) {
                            Ok(cow) => Some(cow.to_vec()),
                            Err(e) => {
                                crate::println!("exec: cannot read '{}': {}", resolved_path, e);
                                None
                            }
                        },
                        Err(e) => {
                            crate::println!("exec: file not found '{}': {}", resolved_path, e);
                            None
                        }
                    }
                };

                if let Some(data) = file_data {
                    let program_name: &'static str = alloc::boxed::Box::leak(
                        alloc::format!("user-{}", resolved_path.trim_start_matches('/')).into_boxed_str()
                    );

                    match crate::fs::elf::load_and_spawn(program_name, &data) {
                        Ok(pid) => {
                            crate::println!("[EXEC] Launched Ring 3 ELF process '{}' (PID {})", program_name, pid);
                            // Yield CPU slice to immediately run the process
                            crate::task::yield_now();
                        }
                        Err(err) => {
                            crate::println!("exec: failed to load ELF binary '{}': {}", resolved_path, err.as_str());
                        }
                    }
                }
            }

            "sleep" => {
                let ms_str = parts.next().unwrap_or("1000");
                let ms = parse_u64(ms_str).unwrap_or(1000);
                crate::println!("Sleeping for {} ms...", ms);
                crate::task::sleep_ms(ms);
                crate::println!("Woke up!");
            }

            "spawn" => {
                let name = parts.next().unwrap_or("worker");
                let static_name: &'static str = alloc::boxed::Box::leak(alloc::string::String::from(name).into_boxed_str());
                let tid = crate::task::spawn(static_name, crate::task::demo_worker_entry);
                crate::println!("Spawned background worker task '{}' with PID {}", name, tid);
            }

            "kill" => {
                if let Some(id_str) = parts.next() {
                    if let Ok(id) = parse_u64(id_str) {
                        if crate::task::kill_task(id as usize) {
                            crate::println!("Task PID {} terminated.", id);
                        } else {
                            crate::println!("kill: cannot kill task PID {} (not found, already dead, or root shell)", id);
                        }
                    } else {
                        crate::println!("kill: invalid PID: {}", id_str);
                    }
                } else {
                    crate::println!("Usage: kill <pid>");
                }
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
                                if let Ok(text) = core::str::from_utf8(&bytes) {
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

            "writesec" => {
                let lba_str = parts.next().unwrap_or("0");
                let lba = parse_u64(lba_str).unwrap_or(0) as u32;
                let text: alloc::string::String = parts.collect::<alloc::vec::Vec<&str>>().join(" ");
                let mut buf = [0u8; crate::drivers::ata::SECTOR_SIZE];
                let bytes = text.as_bytes();
                let copy_len = bytes.len().min(crate::drivers::ata::SECTOR_SIZE);
                buf[..copy_len].copy_from_slice(&bytes[..copy_len]);

                match crate::drivers::ata::write_sector(lba, &buf) {
                    Ok(()) => {
                        crate::println!("writesec: successfully wrote {} bytes to LBA sector {}", copy_len, lba);
                    }
                    Err(err) => {
                        crate::println!("writesec: error writing sector {}: {}", lba, err);
                    }
                }
            }

            "formatfat" => {
                let lba = parts.next().and_then(|s| parse_u64(s).ok()).unwrap_or(2048) as u32;
                let sectors = parts.next().and_then(|s| parse_u64(s).ok()).unwrap_or(65536) as u32;
                crate::println!("Formatting FAT32 partition at LBA {} ({} sectors = {} MiB)...", lba, sectors, (sectors * 512) / (1024 * 1024));
                match crate::fs::fat32::Fat32Fs::format(0, lba, sectors, "AURAOS_PERS") {
                    Ok(fs) => {
                        let info = fs.fs_info();
                        crate::println!("formatfat: FAT32 successfully formatted!");
                        crate::println!("  Volume Label : {}", info.volume_label);
                        crate::println!("  Total Space  : {} KiB ({} MiB)", info.total_space_kb, info.total_space_kb / 1024);
                        crate::println!("  Cluster Size : {} bytes", info.bytes_per_cluster);
                        crate::println!("  Free Clusters: {} / {}", info.free_clusters, info.total_clusters);
                        *crate::fs::fat32::FAT32_FS.lock() = Some(fs);
                        crate::println!("  Mounted into global FAT32 filesystem.");
                    }
                    Err(e) => crate::println!("formatfat: error: {}", e),
                }
            }

            "mountfat" => {
                let lba = parts.next().and_then(|s| parse_u64(s).ok()).unwrap_or(2048) as u32;
                match crate::fs::fat32::Fat32Fs::mount(0, lba) {
                    Ok(fs) => {
                        let info = fs.fs_info();
                        crate::println!("mountfat: successfully mounted FAT32 volume from LBA {}!", lba);
                        crate::println!("  Volume Label : {}", info.volume_label);
                        crate::println!("  Total Space  : {} KiB ({} MiB)", info.total_space_kb, info.total_space_kb / 1024);
                        crate::println!("  Free Space   : {} KiB ({} MiB)", info.free_space_kb, info.free_space_kb / 1024);
                        crate::println!("  Cluster Size : {} bytes", info.bytes_per_cluster);
                        *crate::fs::fat32::FAT32_FS.lock() = Some(fs);
                    }
                    Err(e) => crate::println!("mountfat: error: {}", e),
                }
            }

            "fatinfo" => {
                let fs_guard = crate::fs::fat32::FAT32_FS.lock();
                match &*fs_guard {
                    Some(fs) => {
                        let info = fs.fs_info();
                        crate::println!("--- FAT32 PERSISTENT VOLUME INFO ---");
                        crate::println!("  Volume Label : {}", info.volume_label);
                        crate::println!("  Drive / LBA  : Drive {} at LBA {}", fs.drive, fs.lba_start);
                        crate::println!("  Total Space  : {} KiB ({} MiB)", info.total_space_kb, info.total_space_kb / 1024);
                        crate::println!("  Free Space   : {} KiB ({} MiB)", info.free_space_kb, info.free_space_kb / 1024);
                        crate::println!("  Cluster Size : {} bytes ({} sector/cluster)", info.bytes_per_cluster, fs.bpb.sectors_per_cluster);
                        crate::println!("  Total Clusters: {}", info.total_clusters);
                        crate::println!("  Free Clusters : {}", info.free_clusters);
                        crate::println!("------------------------------------");
                    }
                    None => crate::println!("fatinfo: No FAT32 volume mounted. Use 'mountfat' or 'formatfat'."),
                }
            }

            "fatls" => {
                let path = parts.next().unwrap_or("/");
                let fs_guard = crate::fs::fat32::FAT32_FS.lock();
                match &*fs_guard {
                    Some(fs) => match fs.resolve_path(path) {
                        Ok((Some(entry), _)) => {
                            if !entry.is_directory {
                                crate::println!("  {} ({} bytes)", entry.name, entry.file_size);
                            } else {
                                match fs.list_directory_cluster(entry.first_cluster) {
                                    Ok(entries) => {
                                        crate::println!("FAT32 Directory Listing for '{}':", path);
                                        if entries.is_empty() {
                                            crate::println!("  (empty directory)");
                                        } else {
                                            for e in &entries {
                                                let type_str = if e.is_directory { "<DIR>" } else { "     " };
                                                crate::println!("  {}  {:>8} B  {}", type_str, e.file_size, e.name);
                                            }
                                        }
                                    }
                                    Err(err) => crate::println!("fatls: error listing directory: {}", err),
                                }
                            }
                        }
                        Ok((None, _)) => crate::println!("fatls: path '{}' not found", path),
                        Err(err) => crate::println!("fatls: error: {}", err),
                    },
                    None => crate::println!("fatls: No FAT32 volume mounted. Use 'mountfat' or 'formatfat'."),
                }
            }

            "fatcat" => {
                let path = parts.next().unwrap_or("");
                if path.is_empty() {
                    crate::println!("Usage: fatcat <path>");
                } else {
                    let fs_guard = crate::fs::fat32::FAT32_FS.lock();
                    match &*fs_guard {
                        Some(fs) => match fs.read_file(path) {
                            Ok(bytes) => match core::str::from_utf8(&bytes) {
                                Ok(text) => crate::println!("{}", text),
                                Err(_) => crate::println!("<Binary content: {} bytes>", bytes.len()),
                            },
                            Err(err) => crate::println!("fatcat: error: {}", err),
                        },
                        None => crate::println!("fatcat: No FAT32 volume mounted. Use 'mountfat' or 'formatfat'."),
                    }
                }
            }

            "fatwrite" => {
                let path = parts.next().unwrap_or("");
                let text: alloc::string::String = parts.collect::<alloc::vec::Vec<&str>>().join(" ");
                if path.is_empty() {
                    crate::println!("Usage: fatwrite <path> <text>");
                } else {
                    let mut fs_guard = crate::fs::fat32::FAT32_FS.lock();
                    match &mut *fs_guard {
                        Some(fs) => match fs.write_file(path, text.as_bytes()) {
                            Ok(()) => crate::println!("fatwrite: successfully wrote {} bytes to '{}'", text.len(), path),
                            Err(err) => crate::println!("fatwrite: error: {}", err),
                        },
                        None => crate::println!("fatwrite: No FAT32 volume mounted. Use 'mountfat' or 'formatfat'."),
                    }
                }
            }

            "fatmkdir" => {
                let path = parts.next().unwrap_or("");
                if path.is_empty() {
                    crate::println!("Usage: fatmkdir <path>");
                } else {
                    let mut fs_guard = crate::fs::fat32::FAT32_FS.lock();
                    match &mut *fs_guard {
                        Some(fs) => match fs.create_dir(path) {
                            Ok(c) => crate::println!("fatmkdir: created directory '{}' (cluster {})", path, c),
                            Err(err) => crate::println!("fatmkdir: error: {}", err),
                        },
                        None => crate::println!("fatmkdir: No FAT32 volume mounted. Use 'mountfat' or 'formatfat'."),
                    }
                }
            }

            "fatrm" => {
                let path = parts.next().unwrap_or("");
                if path.is_empty() {
                    crate::println!("Usage: fatrm <path>");
                } else {
                    let mut fs_guard = crate::fs::fat32::FAT32_FS.lock();
                    match &mut *fs_guard {
                        Some(fs) => match fs.delete_entry(path) {
                            Ok(()) => crate::println!("fatrm: removed entry '{}'", path),
                            Err(err) => crate::println!("fatrm: error: {}", err),
                        },
                        None => crate::println!("fatrm: No FAT32 volume mounted. Use 'mountfat' or 'formatfat'."),
                    }
                }
            }

            "ifconfig" => {
                let net = crate::net::NETWORK.lock();
                crate::println!("--- AURAOS NETWORK INTERFACE (eth0) ---");
                crate::println!("  Status       : {}", if net.is_enabled { "UP (e1000 Active)" } else { "DOWN (No NIC)" });
                crate::println!("  MAC Address  : {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                    net.mac[0], net.mac[1], net.mac[2], net.mac[3], net.mac[4], net.mac[5]);
                crate::println!("  IPv4 Address : {}.{}.{}.{}",
                    net.ip[0], net.ip[1], net.ip[2], net.ip[3]);
                crate::println!("  Subnet Mask  : {}.{}.{}.{}",
                    net.subnet[0], net.subnet[1], net.subnet[2], net.subnet[3]);
                crate::println!("  Gateway      : {}.{}.{}.{}",
                    net.gateway[0], net.gateway[1], net.gateway[2], net.gateway[3]);
                crate::println!("  DNS Server   : {}.{}.{}.{}",
                    net.dns[0], net.dns[1], net.dns[2], net.dns[3]);
                crate::println!("  TX Packets   : {} ({} bytes)", net.packets_tx, net.bytes_tx);
                crate::println!("  RX Packets   : {} ({} bytes)", net.packets_rx, net.bytes_rx);
                crate::println!("---------------------------------------");
            }

            "ping" => {
                let ip_str = parts.next().unwrap_or("10.0.2.2");
                if let Some(target_ip) = parse_ipv4(ip_str) {
                    crate::println!("PING {}.{}.{}.{} — sending ICMP Echo Request...",
                        target_ip[0], target_ip[1], target_ip[2], target_ip[3]);

                    let sent = {
                        let mut net = crate::net::NETWORK.lock();
                        if !net.is_enabled {
                            crate::println!("ping: network interface is DOWN. No e1000 NIC detected.");
                            false
                        } else {
                            net.send_ping(target_ip)
                        }
                    };

                    if sent {
                        // Poll for reply (wait up to ~2 seconds = 200 ticks at 100 Hz, with spin guard)
                        let start = crate::arch::idt::ticks();
                        let mut got_reply = false;
                        let mut guard: u32 = 0;
                        while crate::arch::idt::ticks().saturating_sub(start) < 200 && guard < 100_000 {
                            crate::net::poll();
                            {
                                let net = crate::net::NETWORK.lock();
                                if let Some(rtt) = net.last_ping_rtt_ms {
                                    crate::println!("Reply from {}.{}.{}.{}: time={} ms (seq={})",
                                        target_ip[0], target_ip[1], target_ip[2], target_ip[3],
                                        rtt, net.last_ping_seq);
                                    got_reply = true;
                                    break;
                                }
                            }
                            core::hint::spin_loop();
                            guard += 1;
                        }
                        if !got_reply {
                            crate::println!("Request timed out (no reply in 2000 ms).");
                        }
                    }
                } else {
                    crate::println!("ping: invalid IP address '{}'. Usage: ping 10.0.2.2", ip_str);
                }
            }

            "arp" => {
                let net = crate::net::NETWORK.lock();
                crate::println!("--- AURAOS ARP CACHE TABLE ---");
                if net.arp_table.entries.is_empty() {
                    crate::println!("  (empty — no resolved entries)");
                } else {
                    crate::println!("  IP ADDRESS         MAC ADDRESS           AGE (ticks)");
                    crate::println!("  ---------------    -----------------     -----------");
                    let now = crate::arch::idt::ticks();
                    for entry in &net.arp_table.entries {
                        let age = now.saturating_sub(entry.timestamp_tick);
                        crate::println!("  {}.{}.{}.{:<8}  {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}      {}",
                            entry.ip[0], entry.ip[1], entry.ip[2], entry.ip[3],
                            entry.mac[0], entry.mac[1], entry.mac[2],
                            entry.mac[3], entry.mac[4], entry.mac[5],
                            age);
                    }
                }
                crate::println!("  Total entries: {}", net.arp_table.entries.len());
                crate::println!("------------------------------");
            }

            "udpsend" => {
                let ip_str = parts.next().unwrap_or("");
                let port_str = parts.next().unwrap_or("");
                let text: alloc::string::String = parts.collect::<alloc::vec::Vec<&str>>().join(" ");

                if ip_str.is_empty() || port_str.is_empty() {
                    crate::println!("Usage: udpsend <ip> <port> <text>");
                    crate::println!("  Example: udpsend 10.0.2.2 9999 Hello from AuraOS!");
                } else if let (Some(dest_ip), Ok(dest_port)) = (parse_ipv4(ip_str), parse_u64(port_str)) {
                    let msg = if text.is_empty() { "AuraOS UDP Test" } else { &text };
                    let mut net = crate::net::NETWORK.lock();
                    if !net.is_enabled {
                        crate::println!("udpsend: network interface is DOWN.");
                    } else {
                        let ok = net.send_udp(dest_ip, 12345, dest_port as u16, msg.as_bytes());
                        if ok {
                            crate::println!("UDP: Sent {} bytes to {}.{}.{}.{}:{}",
                                msg.len(), dest_ip[0], dest_ip[1], dest_ip[2], dest_ip[3], dest_port);
                        } else {
                            crate::println!("udpsend: failed to transmit UDP datagram.");
                        }
                    }
                } else {
                    crate::println!("udpsend: invalid IP or port. Usage: udpsend <ip> <port> <text>");
                }
            }

            "netstat" => {
                let net = crate::net::NETWORK.lock();
                crate::println!("=== AURAOS NETWORK STATISTICS ===");
                crate::println!("  Interface    : eth0 (Intel e1000 Gigabit)");
                crate::println!("  Status       : {}", if net.is_enabled { "UP" } else { "DOWN" });
                crate::println!("  TX Packets   : {}", net.packets_tx);
                crate::println!("  TX Bytes     : {} ({} KiB)", net.bytes_tx, net.bytes_tx / 1024);
                crate::println!("  RX Packets   : {}", net.packets_rx);
                crate::println!("  RX Bytes     : {} ({} KiB)", net.bytes_rx, net.bytes_rx / 1024);
                crate::println!("  Pings Sent   : {}", net.pings_sent);
                crate::println!("  Pings Received: {}", net.pings_received);
                if let Some(rtt) = net.last_ping_rtt_ms {
                    crate::println!("  Last RTT     : {} ms", rtt);
                }
                crate::println!("  ARP Entries  : {}", net.arp_table.entries.len());
                crate::println!("=================================");
            }

            "test" | "selftest" => {
                crate::println!("============ AURAOS KERNEL SELF-TEST SUITE ============");
                let results = crate::tests::run_all_tests();
                let mut pass_count = 0u32;
                let mut fail_count = 0u32;
                for (i, res) in results.iter().enumerate() {
                    if res.passed {
                        pass_count += 1;
                        crate::println!("  [{:>2}/{}] PASS  {}", i + 1, results.len(), res.name);
                    } else {
                        fail_count += 1;
                        crate::println!("  [{:>2}/{}] FAIL  {}", i + 1, results.len(), res.name);
                        crate::println!("         -> {}", res.detail);
                    }
                }
                crate::println!("=======================================================");
                if fail_count == 0 {
                    crate::println!("  Result: {}/{} PASSED | System integrity: 100%% OK", pass_count, results.len());
                } else {
                    crate::println!("  Result: {}/{} PASSED, {} FAILED", pass_count, results.len(), fail_count);
                }
                crate::println!("=======================================================");
            }

            "reboot" => {
                crate::arch::power::reboot();
            }

            "shutdown" | "poweroff" => {
                crate::arch::power::shutdown();
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

/// Parses a dotted-decimal IPv4 address string (e.g. "10.0.2.2") into [u8; 4].
fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let s = s.trim();
    let mut octets = [0u8; 4];
    let mut idx = 0;
    for part in s.split('.') {
        if idx >= 4 {
            return None;
        }
        match parse_u64(part) {
            Ok(v) if v <= 255 => octets[idx] = v as u8,
            _ => return None,
        }
        idx += 1;
    }
    if idx == 4 { Some(octets) } else { None }
}

/// Global singleton instance of the AuraOS Shell, synchronized with a Spinlock.
pub static SHELL: Spinlock<Shell> = Spinlock::new(Shell::new());

/// Submits the current line: extracts command, releases the SHELL lock (re-enabling interrupts),
/// executes the command with interrupts active, and prints the new prompt.
pub fn on_enter() {
    let mut cmd_buf = alloc::string::String::new();
    {
        crate::println!();
        let mut shell = SHELL.lock();
        if shell.length > 0 {
            let len = shell.length;
            shell.length = 0;
            if let Ok(line) = core::str::from_utf8(&shell.buffer[..len]) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    cmd_buf.push_str(trimmed);
                }
            }
        }
    }

    if !cmd_buf.is_empty() {
        Shell::execute(&cmd_buf);
    }

    crate::print!("auraos> ");
}
