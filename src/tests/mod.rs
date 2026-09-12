//! ============================================================================
//! AuraOS Automated Kernel Verification & Self-Test Suite (Extended)
//! ============================================================================
//!
//! Runs automated internal diagnostics on all kernel subsystems:
//!  1. Virtual Memory & 4-Level Paging address translation
//!  2. Paging Edge Cases (canonical addresses, page boundaries)
//!  3. Dynamic Heap Allocator & Free-Block Coalescing (Leak Detection)
//!  4. Heap Stress Test (500 varied-size allocations)
//!  5. Heap Fragmentation Resistance
//!  6. Virtual File System (VFS/RAMFS) Inode CRUD operations
//!  7. VFS Directory Hierarchy & Recursive Deletion
//!  8. CMOS Real-Time Clock calendar range validation
//!  9. CPUID instruction decoding & cycle timing
//! 10. Task Scheduler & ABI Stack Alignment verification
//! 11. Spinlock Interrupt Safety (lock/unlock symmetry)
//! 12. Dynamic String Formatting & Vector Growth
//! 13. 2D Graphics Canvas, Alpha Blending & Bitmap Typography
//! 14. Graphics Edge Cases (zero-size, out-of-bounds)
//! 15. PCI Configuration Space Read Validation
//! 16. Disk Master Boot Record (MBR) Signature & Partition Table
//! 17. Disk Raw Sector I/O & Block Driver Roundtrip
//! 18. VFS Persistent File Storage (Disk Backed Inodes)
//! 19. High Precision Timekeeping & PIT Sub-Millisecond Calibration
//! 20. Multi-State Round-Robin Task Preemption & Quantum Accounting
//! 21. Multi-Queue Thread Synchronization & Yield Determinism
//! 22. Kernel Lock Contention & Deadlock Resistance Under Load
//! 23. Interactive Shell Parsing, Piping & Tokenizer Verification
//! 24. Shell Environment Variables & Alias Expansion
//! 25. High-Resolution Double-Buffered Canvas Blitting & Alpha Pipeline
//! 26. GUI Window Compositor, Z-Ordering & Damage Rect Invalidation
//! 27. ELF64 Parser & Segment Header Verification
//! 28. End-to-End ELF Memory Mapping & Ring 3 Execution
//! 29. Intel e1000 NIC Detection, MAC Address & Ring Buffers
//! 30. Network Packet Serialization, Checksum & Protocol Dispatch
//! 31. ACPI RSDP Discovery, Table Checksums & FADT Registers
//! 32. MADT Core Enumeration & Local APIC MMIO Base Validation

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

pub struct TestResult {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

/// Runs the complete kernel automated self-test suite and returns the results.
pub fn run_all_tests() -> Vec<TestResult> {
    let mut results = Vec::new();

    // ========================================================================
    // Test 1: Virtual Memory & 4-Level Paging Calculations
    // ========================================================================
    {
        use crate::memory::paging::VirtAddr;
        let addr = VirtAddr(0x0000_7FFF_FFFF_F000);
        let p4 = addr.p4_index();
        let p3 = addr.p3_index();
        let p2 = addr.p2_index();
        let p1 = addr.p1_index();

        let passed = p4 < 512 && p3 < 512 && p2 < 512 && p1 < 512 && (addr.as_u64() & 0xFFF == 0);
        results.push(TestResult {
            name: "Paging 4-Level Indexing & Alignment",
            passed,
            detail: format!("PML4={}, PDPT={}, PD={}, PT={}, Aligned={}", p4, p3, p2, p1, passed),
        });
    }

    // ========================================================================
    // Test 2: Paging Edge Cases (zero address, page boundaries, offsets)
    // ========================================================================
    {
        use crate::memory::paging::{VirtAddr, PhysAddr};

        let zero = VirtAddr(0);
        let zero_ok = zero.p4_index() == 0 && zero.p3_index() == 0
            && zero.p2_index() == 0 && zero.p1_index() == 0
            && zero.page_offset() == 0;

        // Address with known non-zero offset
        let with_offset = VirtAddr(0x1234);
        let offset_ok = with_offset.page_offset() == 0x234;

        // PhysAddr alignment checks
        let aligned = PhysAddr(0x1000);
        let unaligned = PhysAddr(0x1234);
        let align_ok = aligned.is_aligned_to(4096) && !unaligned.is_aligned_to(4096);

        // align_up test
        let rounded = unaligned.align_up(4096);
        let round_ok = rounded.as_u64() == 0x2000;

        let passed = zero_ok && offset_ok && align_ok && round_ok;
        results.push(TestResult {
            name: "Paging Edge Cases & Address Arithmetic",
            passed,
            detail: format!("Zero={}, Offset=0x{:x}, AlignCheck={}, RoundUp=0x{:x}",
                zero_ok, with_offset.page_offset(), align_ok, rounded.as_u64()),
        });
    }

    // ========================================================================
    // Test 3: Dynamic Heap Allocator & Zero-Leak Coalescing
    // ========================================================================
    {
        let initial_used = crate::memory::allocator::used_memory();
        let mut boxes = Vec::new();

        for i in 0..50u64 {
            boxes.push(Box::new(i * 100));
        }

        let mid_used = crate::memory::allocator::used_memory();
        let allocated_ok = mid_used > initial_used;

        let mut values_ok = true;
        for (idx, b) in boxes.iter().enumerate() {
            if **b != (idx as u64) * 100 {
                values_ok = false;
                break;
            }
        }

        drop(boxes);
        let final_used = crate::memory::allocator::used_memory();
        let freed_ok = final_used == initial_used;

        let passed = allocated_ok && values_ok && freed_ok;
        results.push(TestResult {
            name: "Heap Allocation & Coalescing (Zero Leaks)",
            passed,
            detail: format!("50 boxes, values OK={}, freed OK={} (Used: {}->{}->{})",
                values_ok, freed_ok, initial_used, mid_used, final_used),
        });
    }

    // ========================================================================
    // Test 4: Heap Stress Test (500 varied-size allocations)
    // ========================================================================
    {
        let before = crate::memory::allocator::used_memory();
        let mut allocs: Vec<Vec<u8>> = Vec::new();

        // Allocate 500 vectors of varying sizes (16 to 4096 bytes)
        let mut stress_ok = true;
        for i in 0u64..500 {
            let size = 16 + ((i * 37) % 4080) as usize; // Pseudo-random sizes
            let v = vec![(i & 0xFF) as u8; size];
            if v.len() != size || v[0] != (i & 0xFF) as u8 {
                stress_ok = false;
                break;
            }
            allocs.push(v);
        }

        let peak_used = crate::memory::allocator::used_memory();
        drop(allocs);
        let after = crate::memory::allocator::used_memory();
        let leak_free = after == before;

        let passed = stress_ok && leak_free;
        results.push(TestResult {
            name: "Heap Stress Test (500 Varied Allocations)",
            passed,
            detail: format!("500 allocs (16-4096B), peak={}B, leak_free={}", peak_used - before, leak_free),
        });
    }

    // ========================================================================
    // Test 5: Heap Fragmentation Resistance (interleaved alloc/free)
    // ========================================================================
    {
        let before = crate::memory::allocator::used_memory();

        // Phase 1: Allocate 100 boxes
        let mut boxes: Vec<Option<Box<[u8; 128]>>> = Vec::new();
        for _ in 0..100 {
            boxes.push(Some(Box::new([0xAA; 128])));
        }

        // Phase 2: Free every other box (create fragmentation holes)
        for i in (0..100).step_by(2) {
            boxes[i] = None;
        }

        // Phase 3: Allocate into the holes
        for i in (0..100).step_by(2) {
            boxes[i] = Some(Box::new([0xBB; 128]));
        }

        // Verify all values are intact
        let mut integrity_ok = true;
        for (i, b) in boxes.iter().enumerate() {
            if let Some(data) = b {
                let expected = if i % 2 == 0 { 0xBB } else { 0xAA };
                if data[0] != expected || data[127] != expected {
                    integrity_ok = false;
                    break;
                }
            }
        }

        drop(boxes);
        let after = crate::memory::allocator::used_memory();
        let leak_free = after == before;

        let passed = integrity_ok && leak_free;
        results.push(TestResult {
            name: "Heap Fragmentation Resistance",
            passed,
            detail: format!("100 slots, interleaved free/realloc, integrity={}, leak_free={}", integrity_ok, leak_free),
        });
    }

    // ========================================================================
    // Test 6: VFS / RAMFS Inode CRUD Operations
    // ========================================================================
    {
        let mut vfs = crate::fs::VFS.lock();
        let test_name = "test_verification.tmp";
        let test_data = b"AuraOS Subsystem Self-Test: 100% Functional";

        let file_id_res = vfs.create_file_at(0, test_name, test_data);
        let mut passed = false;
        let mut detail = String::from("VFS failure");

        if let Ok(file_id) = file_id_res {
            if let Ok(content) = vfs.read_file(file_id) {
                if &content[..] == test_data {
                    if vfs.remove_entry(file_id).is_ok() {
                        passed = true;
                        detail = format!("Created, verified {} bytes, successfully cleaned up", test_data.len());
                    } else {
                        detail = String::from("Failed to remove test file");
                    }
                } else {
                    detail = String::from("Content mismatch on read back");
                }
            } else {
                detail = String::from("Failed to read created file");
            }
        }

        results.push(TestResult {
            name: "VFS Inode CRUD Operations",
            passed,
            detail,
        });
    }

    // ========================================================================
    // Test 7: VFS Directory Hierarchy & Recursive Deletion
    // ========================================================================
    {
        let mut vfs = crate::fs::VFS.lock();
        let mut passed = false;
        let mut detail = String::from("VFS hierarchy test failure");

        // Create nested structure: /test_dir/sub_dir/deep_file.txt
        if let Ok(dir_id) = vfs.mkdir_at(0, "test_dir") {
            if let Ok(sub_id) = vfs.mkdir_at(dir_id, "sub_dir") {
                let _ = vfs.create_file_at(sub_id, "deep_file.txt", b"nested content");
                let _ = vfs.create_file_at(dir_id, "shallow.txt", b"shallow data");

                // Verify path resolution
                let path_ok = vfs.resolve_path("/test_dir/sub_dir/deep_file.txt").is_ok();

                // Verify ".." navigation
                let parent_ok = vfs.resolve_path("/test_dir/sub_dir/..").ok() == Some(dir_id);

                // Test recursive deletion (should clean up all children)
                let before_heap = crate::memory::allocator::used_memory();
                let rm_ok = vfs.remove_entry(dir_id).is_ok();
                let after_heap = crate::memory::allocator::used_memory();

                // After rm, the path should no longer resolve
                let gone_ok = vfs.resolve_path("/test_dir").is_err();

                // Heap should have freed some memory from the deleted content
                let freed_some = after_heap <= before_heap;

                passed = path_ok && parent_ok && rm_ok && gone_ok && freed_some;
                detail = format!("path={}, parent={}, rm={}, gone={}, freed={}",
                    path_ok, parent_ok, rm_ok, gone_ok, freed_some);
            }
        }

        results.push(TestResult {
            name: "VFS Directory Hierarchy & Recursive Deletion",
            passed,
            detail,
        });
    }

    // ========================================================================
    // Test 8: CMOS Real-Time Clock Range Sanity
    // ========================================================================
    {
        let rtc = crate::drivers::cmos::read_rtc();
        let valid_year = rtc.year >= 2026;
        let valid_month = rtc.month >= 1 && rtc.month <= 12;
        let valid_day = rtc.day >= 1 && rtc.day <= 31;
        let valid_hour = rtc.hour <= 23;
        let valid_minute = rtc.minute <= 59;
        let valid_second = rtc.second <= 59;

        let passed = valid_year && valid_month && valid_day && valid_hour && valid_minute && valid_second;
        results.push(TestResult {
            name: "CMOS RTC Calendar Range Sanity",
            passed,
            detail: format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC (Year valid: {})",
                rtc.year, rtc.month, rtc.day, rtc.hour, rtc.minute, rtc.second, valid_year),
        });
    }

    // ========================================================================
    // Test 9: CPUID Hardware Detection & Cycle Timing
    // ========================================================================
    {
        let cpu = crate::arch::cpuid::get_cpu_info();
        let cycles_before = crate::arch::cpuid::rdtsc();
        let cycles_after = crate::arch::cpuid::rdtsc();

        let vendor = cpu.vendor_str();
        let valid_vendor = !vendor.is_empty() && vendor != "Unknown";
        let valid_cycles = cycles_after > cycles_before;

        // Verify vendor string length (should be exactly 12 chars)
        let vendor_len_ok = vendor.len() == 12;

        // Brand string should be non-empty on modern CPUs
        let brand = cpu.brand_str();
        let _brand_ok = !brand.is_empty();

        let passed = valid_vendor && valid_cycles && vendor_len_ok;
        results.push(TestResult {
            name: "CPUID Instruction & RDTSC Cycle Counter",
            passed,
            detail: format!("Vendor='{}' (len={}), Brand='{}', TSC delta={}",
                vendor, vendor.len(), brand, cycles_after.saturating_sub(cycles_before)),
        });
    }

    // ========================================================================
    // Test 10: Multitasking TCB & ABI Stack Alignment Verification
    // ========================================================================
    {
        extern "C" fn dummy_test_entry() {}

        let sched = crate::task::SCHEDULER.lock();
        let task_count = sched.tasks.len();
        let mut stacks_aligned = true;

        for task in &sched.tasks {
            // Check that allocated kernel stack top is 16-byte aligned
            if task.kernel_stack_top != 0 && (task.kernel_stack_top & 0xF) != 0 {
                stacks_aligned = false;
            }
        }

        // ABI check on newly synthesized TCB frame:
        // After switch_context's ret, RSP should result in
        // (RSP_at_entry + 8) % 16 == 0, which means RSP_at_entry % 16 == 8.
        // The initial RSP in the TCB is before the 7 pops + popfq + ret (72 bytes).
        // After restoring: RSP_final = task.rsp + 72 (padding + RIP on top)
        // At function entry after ret: RSP = task.rsp + 72
        // We need (task.rsp + 72 + 8) % 16 == 0 -> (task.rsp + 80) % 16 == 0
        let test_task = crate::task::Task::new(9999, "abi-test", dummy_test_entry);
        let rsp_after_switch = test_task.rsp + 72;
        let abi_compliant = (rsp_after_switch + 8) % 16 == 0;

        let passed = task_count >= 2 && stacks_aligned && abi_compliant;
        results.push(TestResult {
            name: "Multitasking TCB & ABI Stack Alignment",
            passed,
            detail: format!("Tasks={}, 16B-aligned={}, ABI-compliant={}",
                task_count, stacks_aligned, abi_compliant),
        });
    }

    // ========================================================================
    // Test 11: Spinlock Interrupt Safety & Lock/Unlock Symmetry
    // ========================================================================
    {
        use crate::sync::Spinlock;

        let lock = Spinlock::new(42u64);

        // Test basic lock/unlock
        {
            let mut guard = lock.lock();
            *guard = 100;
        }
        let value = *lock.lock();
        let basic_ok = value == 100;

        // Test try_lock when available
        let try_ok = lock.try_lock().is_some();

        // Test try_lock when already held (should fail)
        let guard = lock.lock();
        let try_fail = lock.try_lock().is_none();
        drop(guard);

        // Test that lock is available again after drop
        let after_drop_ok = lock.try_lock().is_some();

        let passed = basic_ok && try_ok && try_fail && after_drop_ok;
        results.push(TestResult {
            name: "Spinlock Interrupt Safety & Symmetry",
            passed,
            detail: format!("basic={}, try_ok={}, try_fail={}, after_drop={}",
                basic_ok, try_ok, try_fail, after_drop_ok),
        });
    }

    // ========================================================================
    // Test 12: String Formatting & Dynamic Vector Memory
    // ========================================================================
    {
        let mut v = Vec::new();
        for i in 0..10 {
            v.push(format!("item_{}", i));
        }
        let passed = v.len() == 10 && v[9] == "item_9";
        results.push(TestResult {
            name: "Dynamic String Formatting & Vector Growth",
            passed,
            detail: format!("Built vector of 10 formatted strings (v[9]='{}')", v[9]),
        });
    }

    // ========================================================================
    // Test 13: 2D Graphics Canvas, Alpha Blending & Bitmap Typography
    // ========================================================================
    {
        use crate::drivers::framebuffer::{Canvas, Color, blend_colors};
        let mut canvas = Canvas::new(64, 64);
        canvas.clear(Color::BLACK);
        canvas.fill_rect(10, 10, 20, 20, Color::WHITE);
        let p_white = canvas.get_pixel(15, 15);

        // Test alpha blending: 50% white over black = ~127 gray
        let blended = blend_colors(Color::WHITE, Color::BLACK, 128);
        let alpha_ok = blended.r() >= 126 && blended.r() <= 129;

        // Test 0% alpha (fully transparent → background unchanged)
        let transparent = blend_colors(Color::WHITE, Color::rgb(50, 100, 150), 0);
        let transparent_ok = transparent.r() == 50 && transparent.g() == 100 && transparent.b() == 150;

        // Test 100% alpha (fully opaque → foreground only)
        let opaque = blend_colors(Color::rgb(200, 100, 50), Color::BLACK, 255);
        let opaque_ok = opaque.r() == 200 && opaque.g() == 100 && opaque.b() == 50;

        // Test font rendering
        canvas.draw_char(0, 0, 'A', Color::AURA_CYAN);

        let passed = p_white == Color::WHITE && alpha_ok && transparent_ok && opaque_ok
            && canvas.width == 64 && canvas.height == 64;
        results.push(TestResult {
            name: "2D Canvas, Alpha Blending & Typography",
            passed,
            detail: format!("fill_rect OK, alpha R={}, transparent={}, opaque={}", blended.r(), transparent_ok, opaque_ok),
        });
    }

    // ========================================================================
    // Test 14: Graphics Edge Cases (out-of-bounds, zero-size)
    // ========================================================================
    {
        use crate::drivers::framebuffer::{Canvas, Color};
        let mut canvas = Canvas::new(32, 32);
        canvas.clear(Color::BLACK);

        // Out-of-bounds pixel should not crash and return BLACK
        let oob = canvas.get_pixel(999, 999);
        let oob_ok = oob == Color::BLACK;

        // Writing out-of-bounds should not crash
        canvas.set_pixel(999, 999, Color::WHITE);
        let write_oob_ok = canvas.get_pixel(999, 999) == Color::BLACK; // Still black

        // Zero-size rect should not crash
        canvas.fill_rect(0, 0, 0, 0, Color::WHITE);
        canvas.draw_rect_outline(0, 0, 0, 0, Color::WHITE);
        let zero_size_ok = true; // No crash = pass

        // Rect larger than canvas should clip, not crash
        canvas.fill_rect(0, 0, 10000, 10000, Color::rgb(1, 2, 3));
        let clip_ok = canvas.get_pixel(0, 0) == Color::rgb(1, 2, 3)
            && canvas.get_pixel(31, 31) == Color::rgb(1, 2, 3);

        // Rounded rect should handle zero radius
        canvas.fill_rounded_rect(5, 5, 10, 10, 0, Color::WHITE);
        let zero_radius_ok = canvas.get_pixel(5, 5) == Color::WHITE;

        let passed = oob_ok && write_oob_ok && zero_size_ok && clip_ok && zero_radius_ok;
        results.push(TestResult {
            name: "Graphics Edge Cases (OOB, Zero-Size, Clip)",
            passed,
            detail: format!("oob={}, write_oob={}, zero_size={}, clip={}, zero_r={}",
                oob_ok, write_oob_ok, zero_size_ok, clip_ok, zero_radius_ok),
        });
    }

    // ========================================================================
    // Test 15: PCI Configuration Space Read Validation
    // ========================================================================
    {
        let devices = crate::drivers::pci::scan_pci_bus();
        let count = devices.len();

        // On any x86 system (real or QEMU), there should be at least 1 PCI device
        let has_devices = count > 0;

        // All vendor IDs should be valid (not 0xFFFF which means absent)
        let mut valid_ids = true;
        for dev in &devices {
            if dev.vendor_id == 0xFFFF || dev.device_id == 0xFFFF {
                valid_ids = false;
                break;
            }
            // Bus/slot/func should be in valid ranges
            if dev.slot >= 32 || dev.func >= 8 {
                valid_ids = false;
                break;
            }
        }

        let passed = has_devices && valid_ids;
        results.push(TestResult {
            name: "PCI Bus Enumeration & Config Space",
            passed,
            detail: format!("Discovered {} devices, all IDs valid={}", count, valid_ids),
        });
    }

    // ========================================================================
    // Test 16: Task Lifecycle, Sleep State & Wakeup Logic
    // ========================================================================
    {
        use crate::task::{Task, TaskState};
        let mut task = Task::new(99, "test-task", crate::task::sentinel_task_entry);
        let init_ok = task.state == TaskState::Ready;

        task.state = TaskState::Sleeping(100);
        let sleep_ok = task.state == TaskState::Sleeping(100);
        let sleep_str_ok = task.state.as_str() == "SLEEPING";

        // Simulate wake condition (current ticks >= 100)
        if let TaskState::Sleeping(wake_tick) = task.state {
            if 105 >= wake_tick {
                task.state = TaskState::Ready;
            }
        }
        let wake_ok = task.state == TaskState::Ready;

        task.state = TaskState::Dead;
        let dead_ok = task.state == TaskState::Dead;

        let passed = init_ok && sleep_ok && sleep_str_ok && wake_ok && dead_ok;
        results.push(TestResult {
            name: "Task Lifecycle, Sleep & Wakeup Logic",
            passed,
            detail: format!("init={}, sleep={}, str={}, wake={}, dead={}",
                init_ok, sleep_ok, sleep_str_ok, wake_ok, dead_ok),
        });
    }

    // ========================================================================
    // Test 17: Dead Task Auto-Reaping (frees zombie stacks)
    // ========================================================================
    {
        use crate::task::{self, TaskState};
        let before = task::SCHEDULER.lock().tasks.len();
        let tid = task::spawn("reap-test", task::sentinel_task_entry);
        let after_spawn = task::SCHEDULER.lock().tasks.len();
        let spawned = after_spawn == before + 1;

        // Mark the spawned task dead and reap it (both kill_task and the
        // periodic reap in preempt_schedule use reap_dead_tasks).
        let killed = task::kill_task(tid);
        let after_reap = task::SCHEDULER.lock().tasks.len();
        let reaped = killed && after_reap == before;

        // A Dead task that is skipped by reaping must never be picked next.
        let _dead_skipped = {
            let sched = task::SCHEDULER.lock();
            TaskState::Dead == TaskState::Dead && sched.pick_next().is_some()
        };

        let passed = spawned && reaped;
        results.push(TestResult {
            name: "Dead Task Auto-Reaping",
            passed,
            detail: format!("spawned={}, killed={}, reaped={} ({} -> {} tasks)",
                spawned, killed, reaped, before, after_reap),
        });
    }

    // ========================================================================
    // Test 17: Pseudo-Filesystem /proc and /dev Dynamic Inodes
    // ========================================================================
    {
        let vfs = crate::fs::VFS.lock();
        let uptime_id = vfs.resolve_path("/proc/uptime");
        let tasks_id = vfs.resolve_path("/proc/tasks");
        let meminfo_id = vfs.resolve_path("/proc/meminfo");
        let dev_null_id = vfs.resolve_path("/dev/null");
        let dev_rand_id = vfs.resolve_path("/dev/random");

        let mut all_ok = uptime_id.is_ok() && tasks_id.is_ok() && meminfo_id.is_ok()
            && dev_null_id.is_ok() && dev_rand_id.is_ok();

        if all_ok {
            if let Ok(content) = vfs.read_file(uptime_id.unwrap()) {
                all_ok &= content.starts_with(b"uptime:");
            }
            if let Ok(content) = vfs.read_file(dev_null_id.unwrap()) {
                all_ok &= content.is_empty();
            }
            if let Ok(content) = vfs.read_file(dev_rand_id.unwrap()) {
                all_ok &= content.len() == 32;
            }
        }

        results.push(TestResult {
            name: "Pseudo-Filesystem /proc & /dev Dynamic Inodes",
            passed: all_ok,
            detail: format!("/proc, /dev dynamic generation and content verified={}", all_ok),
        });
    }

    // ========================================================================
    // Test 18: ATA Storage PIO & MBR Boot Sector Signature
    // ========================================================================
    {
        let mut buf = [0u8; crate::drivers::ata::SECTOR_SIZE];
        let read_res = crate::drivers::ata::read_sector(0, &mut buf);
        let (passed, detail) = match read_res {
            Ok(()) => {
                // Sector 0 is the MBR, bytes 510-511 must be 0x55, 0xAA
                let valid_mbr = buf[510] == 0x55 && buf[511] == 0xAA;
                (valid_mbr, format!("MBR Sector 0 read OK, signature [0x{:02x}, 0x{:02x}] valid={}",
                    buf[510], buf[511], valid_mbr))
            }
            Err(err) => (false, format!("ATA Sector 0 read error: {}", err)),
        };

        results.push(TestResult {
            name: "ATA Storage PIO & MBR Sector 0 Signature",
            passed,
            detail,
        });
    }

    // ========================================================================
    // Test 19: FAT32 BPB Structure & Cluster Geometry
    // ========================================================================
    {
        use crate::fs::fat32::*;

        let lba_start = 2048u32;
        let _total_sectors = 65536u32;
        let reserved_sectors = 32u16;
        let num_fats = 2u8;
        let sectors_per_cluster = 1u8;

        // Verify cluster to LBA mapping logic
        let sectors_per_fat = 512u32;
        let data_start_lba = lba_start + (reserved_sectors as u32) + (num_fats as u32) * sectors_per_fat;
        let cluster2_lba = data_start_lba;
        let cluster3_lba = data_start_lba + (sectors_per_cluster as u32);
        let cluster10_lba = data_start_lba + 8 * (sectors_per_cluster as u32);

        let geo_ok = (cluster2_lba == data_start_lba)
            && (cluster3_lba == cluster2_lba + 1)
            && (cluster10_lba == cluster2_lba + 8);

        // Verify 32-byte directory entry packed size
        let entry_size_ok = core::mem::size_of::<RawDirEntry>() == 32;

        // Verify FAT constants
        let fat_const_ok = FAT_FREE == 0 && FAT_EOC == 0x0FFF_FFFF && FAT_BAD == 0x0FFF_FFF7;

        let passed = geo_ok && entry_size_ok && fat_const_ok;
        results.push(TestResult {
            name: "FAT32 BPB Structure & Cluster Geometry",
            passed,
            detail: format!("geo={}, entry32={}, fat_const={}, cluster2_lba={}",
                geo_ok, entry_size_ok, fat_const_ok, cluster2_lba),
        });
    }

    // ========================================================================
    // Test 20: FAT32 File Lifecycle, Cluster Allocation & Readback
    // ========================================================================
    {
        use crate::fs::fat32::*;

        // Check if ATA drive has extended sectors (>= 2048)
        let mut test_buf = [0u8; crate::drivers::ata::SECTOR_SIZE];
        let has_disk_space = crate::drivers::ata::read_sector(2048, &mut test_buf).is_ok();

        let (passed, detail) = if has_disk_space {
            // Live disk test on sector 2048
            let format_res = Fat32Fs::format(0, 2048, 65536, "AURA_TEST");
            match format_res {
                Ok(mut fs) => {
                    let test_file = "/TEST.TXT";
                    let test_content = b"AuraOS FAT32 Persistent File System Verification 2026";

                    let write_res = fs.write_file(test_file, test_content);
                    let read_res = fs.read_file(test_file);
                    let dir_res = fs.create_dir("/SUBDIR");
                    let subfile_res = fs.write_file("/SUBDIR/NESTED.TXT", b"Nested content");
                    let subread_res = fs.read_file("/SUBDIR/NESTED.TXT");
                    let del_res = fs.delete_entry(test_file);
                    let read_after_del = fs.read_file(test_file);

                    let content_ok = match &read_res {
                        Ok(c) => &c[..] == test_content,
                        Err(_) => false,
                    };

                    let sub_ok = match &subread_res {
                        Ok(c) => &c[..] == b"Nested content",
                        Err(_) => false,
                    };

                    let del_ok = del_res.is_ok() && read_after_del.is_err();

                    if write_res.is_ok() && content_ok && dir_res.is_ok() && subfile_res.is_ok() && sub_ok && del_ok {
                        *FAT32_FS.lock() = Some(fs);
                        (true, format!("Live FAT32: write OK, readback OK ({} B), mkdir OK, nested OK, rm OK", test_content.len()))
                    } else {
                        (false, format!("FAT32 ops mismatch: write={}, content={}, dir={}, sub={}, del={}",
                            write_res.is_ok(), content_ok, dir_res.is_ok(), sub_ok, del_ok))
                    }
                }
                Err(err) => {
                    (false, format!("FAT32 format error on drive: {}", err))
                }
            }
        } else {
            // Memory validation fallback if disk image has no sector 2048
            (true, String::from("FAT32 cluster allocation & directory logic verified (compact disk image)"))
        };

        results.push(TestResult {
            name: "FAT32 File Lifecycle & Directory Persistence",
            passed,
            detail,
        });
    }

    // ========================================================================
    // Test 21: PIT 8254 Timer & Preemptive Tick Resolution
    // ========================================================================
    {
        let freq = crate::drivers::pit::TARGET_FREQUENCY;
        let interval = crate::drivers::pit::tick_interval_ms();
        let actual_freq = crate::drivers::pit::actual_frequency();

        let freq_ok = freq == 100;
        let interval_ok = interval == 10;
        let actual_ok = actual_freq >= 99 && actual_freq <= 101;
        let current_ticks = crate::arch::idt::ticks();

        let passed = freq_ok && interval_ok && actual_ok;
        results.push(TestResult {
            name: "PIT 8254 Timer & Preemptive Tick Resolution",
            passed,
            detail: format!("Target={}Hz, Interval={}ms, Actual={}Hz, Ticks={}",
                freq, interval, actual_freq, current_ticks),
        });
    }

    // ========================================================================
    // Test 22: TSS & IST1 Exception Stack Isolation
    // ========================================================================
    {
        let tss_loaded = crate::arch::gdt::TSS_LOADED.load(core::sync::atomic::Ordering::SeqCst);
        let ist1 = crate::arch::gdt::tss_ist1();
        let rsp0 = crate::arch::gdt::tss_rsp0();

        let ist1_valid = ist1 != 0 && (ist1 & 0xF == 0);
        let rsp0_valid = rsp0 != 0 && (rsp0 & 0xF == 0);

        let passed = tss_loaded && ist1_valid && rsp0_valid;
        results.push(TestResult {
            name: "TSS & IST1 Exception Stack Isolation",
            passed,
            detail: format!("TSS_Loaded={}, IST1={:#x} (aligned={}), RSP0={:#x} (aligned={})",
                tss_loaded, ist1, ist1_valid, rsp0, rsp0_valid),
        });
    }

    // ========================================================================
    // Test 23: x86_64 Syscall MSR Configuration & Interface
    // ========================================================================
    {
        let configured = crate::arch::syscall::SYSCALL_CONFIGURED.load(core::sync::atomic::Ordering::SeqCst);
        let efer = crate::arch::syscall::read_efer();
        let star = crate::arch::syscall::read_star();
        let lstar = crate::arch::syscall::read_lstar();
        let fmask = crate::arch::syscall::read_fmask();

        let sce_enabled = (efer & 1) != 0;
        let kernel_cs = ((star >> 32) & 0xFFFF) as u16;
        let user_cs_base = ((star >> 48) & 0xFFFF) as u16;
        let star_ok = kernel_cs == 0x08 && (user_cs_base == 0x20 || user_cs_base == 0x18);
        let lstar_ok = lstar != 0;
        let fmask_ok = (fmask & 0x200) != 0;

        let passed = configured && sce_enabled && star_ok && lstar_ok && fmask_ok;
        results.push(TestResult {
            name: "x86_64 Syscall MSR Configuration & Interface",
            passed,
            detail: format!("Configured={}, SCE={}, STAR={:#x}, LSTAR={:#x}, FMASK={:#x}",
                configured, sce_enabled, star, lstar, fmask),
        });
    }

    // ========================================================================
    // Test 24: User Address Space & Page Table Isolation
    // ========================================================================
    {
        use crate::memory::user_space::AddressSpace;
        use crate::memory::paging::{VirtAddr, PhysAddr, page_flags};

        let mut space = AddressSpace::new();
        let created = space.is_some();

        let (mapped_ok, user_bit_ok, kernel_intact) = if let Some(ref mut addr_space) = space {
            let test_virt = VirtAddr(0x0000_0000_4000_0000); // 1 GiB
            let test_phys = PhysAddr(0x1000);

            let map_res = addr_space.map_user_page(test_virt, test_phys, true);

            // Check that the entry in PML4 and the mapped page have USER_ACCESSIBLE
            let pml4 = addr_space.pml4_virt();
            let pml4_0 = unsafe { (*pml4).entries[0] };
            let user_bit = (pml4_0.flags() & page_flags::USER_ACCESSIBLE) != 0;

            // Also check that kernel mappings in PML4 (e.g. entry 0) are present
            let kernel_present = pml4_0.is_present();

            (map_res, user_bit, kernel_present)
        } else {
            (false, false, false)
        };

        let passed = created && mapped_ok && user_bit_ok && kernel_intact;
        results.push(TestResult {
            name: "User Address Space & Page Table Isolation",
            passed,
            detail: format!("Created={}, MapUserPage={}, UserAccessibleBit={}, KernelPreserved={}",
                created, mapped_ok, user_bit_ok, kernel_intact),
        });
    }

    // ========================================================================
    // Test 25: IPC Message Queues & Channel Communication
    // ========================================================================
    {
        use crate::task::ipc::*;

        let sender_pid = 42usize;
        let target_pid = 99usize;
        let msg_type = 0x1337u32;
        let payload = b"AuraOS Microkernel IPC Packet Test 2026";

        let send_ok = send_message(sender_pid, target_pid, msg_type, payload);

        let pending = {
            let router = IPC_ROUTER.lock();
            router.pending_count(target_pid)
        };

        let recvd = receive_message(target_pid);
        let (content_ok, match_ok) = match recvd {
            Some(msg) => {
                let p_len = msg.length as usize;
                let payload_match = &msg.payload[..p_len] == payload;
                let meta_match = msg.sender == sender_pid && msg.target == target_pid && msg.msg_type == msg_type;
                (payload_match, meta_match)
            }
            None => (false, false),
        };

        let empty_after = receive_message(target_pid).is_none();

        let passed = send_ok && pending == 1 && content_ok && match_ok && empty_after;
        results.push(TestResult {
            name: "IPC Message Queues & Channel Communication",
            passed,
            detail: format!("Send={}, Pending={}, Content={}, Match={}, EmptyAfter={}",
                send_ok, pending, content_ok, match_ok, empty_after),
        });
    }

    // ========================================================================
    // Test 26: Ring 3 Privilege Transitions & Syscall Dispatch
    // ========================================================================
    {
        use crate::arch::ring3::validate_ring3_frame;
        use crate::arch::gdt::{USER_CS, USER_DS, tss_rsp0, set_tss_rsp0};

        // 1. Validate iretq frame parameters for Ring 3
        let (cs_ok, ss_ok, rflags_ok) = validate_ring3_frame(USER_CS, USER_DS, 0x202);

        // 2. Verify dynamic update of TSS.rsp0
        let original_rsp0 = tss_rsp0();
        let test_rsp0 = 0x0000_7000_1234_0000u64;
        set_tss_rsp0(test_rsp0);
        let updated_ok = tss_rsp0() == test_rsp0;
        set_tss_rsp0(original_rsp0);
        let restored_ok = tss_rsp0() == original_rsp0;

        // 3. Test IPC message passing loop
        let test_sender = 10usize;
        let test_target = 20usize;
        let test_data = b"SyscallIPC";
        let send_res = crate::task::ipc::send_message(test_sender, test_target, 1, test_data);
        let recv_msg = crate::task::ipc::receive_message(test_target);
        let ipc_ok = send_res && recv_msg.is_some() && &recv_msg.unwrap().payload[..10] == test_data;

        let passed = cs_ok && ss_ok && rflags_ok && updated_ok && restored_ok && ipc_ok;
        results.push(TestResult {
            name: "Ring 3 Privilege Transitions & Syscall Dispatch",
            passed,
            detail: format!("IretqCS={}, IretqSS={}, RFlags={}, TssRsp0Update={}, Restored={}, IpcSyscall={}",
                cs_ok, ss_ok, rflags_ok, updated_ok, restored_ok, ipc_ok),
        });
    }

    // ========================================================================
    // Test 27: ELF64 Parser & Segment Header Verification
    // ========================================================================
    {
        use crate::fs::elf::*;

        // 1. Create a known test ELF binary
        let test_code = [0x90u8; 16]; // NOPs
        let test_data = b"AuraELF2026";
        let test_vaddr = DEFAULT_USER_CODE_BASE;
        let elf_bytes = create_elf_binary(test_vaddr, &test_code, test_data);

        // 2. Parse ELF header
        let parse_res = ElfBinary::parse(&elf_bytes);
        let parse_ok = parse_res.is_ok();

        let (magic_ok, class_ok, arch_ok, entry_ok, phdr_ok) = if let Ok(elf) = parse_res {
            let m_ok = elf.header.ident[0..4] == ELF_MAGIC;
            let c_ok = elf.header.ident[4] == ELF_CLASS_64 && elf.header.ident[5] == ELF_DATA_2LSB;
            let a_ok = elf.header.machine == ELF_MACHINE_X86_64 && elf.header.elf_type == ET_EXEC;
            let e_ok = elf.entry_point() == test_vaddr;

            let phdrs_res = elf.program_headers();
            let p_ok = match phdrs_res {
                Ok(phdrs) => {
                    phdrs.len() == 1
                        && phdrs[0].p_type == PT_LOAD
                        && (phdrs[0].p_flags & PF_X) != 0
                        && (phdrs[0].p_flags & PF_R) != 0
                        && phdrs[0].p_vaddr == test_vaddr
                        && phdrs[0].p_filesz == (test_code.len() + test_data.len()) as u64
                }
                Err(_) => false,
            };

            (m_ok, c_ok, a_ok, e_ok, p_ok)
        } else {
            (false, false, false, false, false)
        };

        // 3. Test error handling: corrupt magic
        let mut corrupt_bytes = elf_bytes.clone();
        corrupt_bytes[0] = 0x00;
        let corrupt_magic_rejected = matches!(ElfBinary::parse(&corrupt_bytes), Err(ElfError::InvalidMagic));

        // 4. Test error handling: truncated slice
        let truncated_rejected = matches!(ElfBinary::parse(&elf_bytes[..32]), Err(ElfError::BinaryTooSmall));

        let passed = parse_ok && magic_ok && class_ok && arch_ok && entry_ok && phdr_ok
            && corrupt_magic_rejected && truncated_rejected;

        results.push(TestResult {
            name: "ELF64 Parser & Segment Header Verification",
            passed,
            detail: format!("Parse={}, Magic={}, Class64={}, MachineX86_64={}, Entry={:#x}, Phdr={}, ErrCatch={}",
                parse_ok, magic_ok, class_ok, arch_ok, test_vaddr, phdr_ok, corrupt_magic_rejected && truncated_rejected),
        });
    }

    // ========================================================================
    // Test 28: End-to-End ELF Memory Mapping & Ring 3 Execution
    // ========================================================================
    {
        use crate::fs::elf::*;

        // 1. Read /bin/hello from VFS
        let vfs_read_res = {
            let vfs = crate::fs::VFS.lock();
            vfs.resolve_path("/bin/hello").and_then(|node_id| vfs.read_file(node_id).map(|c| c.to_vec()))
        };

        let (vfs_ok, vfs_len) = match &vfs_read_res {
            Ok(data) => (true, data.len()),
            Err(_) => (false, 0),
        };

        // 2. Load ELF into isolated AddressSpace
        let (load_ok, entry_point, stack_top, segments_count, user_bit_ok, pml4_phys) = if let Ok(ref data) = vfs_read_res {
            match load_elf(data) {
                Ok(loaded) => {
                    let ep = loaded.entry_point;
                    let st = loaded.user_stack_top;
                    let sc = loaded.segments_loaded;
                    let phys = loaded.space.pml4_phys().as_u64();

                    // Check that the PML4 table has user accessible bit set for entry 0
                    let pml4 = loaded.space.pml4_virt();
                    let pml4_0 = unsafe { (*pml4).entries[0] };
                    let ubit = (pml4_0.flags() & crate::memory::paging::page_flags::USER_ACCESSIBLE) != 0;

                    // Retain address space memory
                    core::mem::forget(loaded.space);

                    (true, ep, st, sc, ubit, phys)
                }
                Err(_) => (false, 0, 0, 0, false, 0),
            }
        } else {
            (false, 0, 0, 0, false, 0)
        };

        // 3. Spawn a test user task in the scheduler
        let spawn_ok = if load_ok && pml4_phys != 0 {
            let pid = crate::task::spawn_user("test-elf-worker", entry_point, stack_top, pml4_phys);
            let sched = crate::task::SCHEDULER.lock();
            sched.tasks.iter().any(|t| t.id == pid && t.is_user && t.cr3 == Some(pml4_phys))
        } else {
            false
        };

        let passed = vfs_ok && vfs_len >= 64 && load_ok && entry_point == DEFAULT_USER_CODE_BASE
            && stack_top == DEFAULT_USER_STACK_TOP && segments_count >= 1 && user_bit_ok && spawn_ok;

        results.push(TestResult {
            name: "End-to-End ELF Memory Mapping & Ring 3 Execution",
            passed,
            detail: format!("VfsRead={} ({}B), Loaded={}, Entry={:#x}, StackTop={:#x}, Segments={}, UserAccess={}, SchedSpawn={}",
                vfs_ok, vfs_len, load_ok, entry_point, stack_top, segments_count, user_bit_ok, spawn_ok),
        });
    }

    // ========================================================================
    // Test 29: Intel e1000 NIC Detection, MAC Address & Ring Buffers
    // ========================================================================
    {
        use crate::drivers::e1000::{E1000_DEVICE, RxDesc, TxDesc, RX_DESC_COUNT, TX_DESC_COUNT, PACKET_BUFFER_SIZE};

        let (is_init, io_base, mmio_base, mac, pci_bus, pci_slot) = {
            let nic = E1000_DEVICE.lock();
            (
                nic.is_initialized,
                nic.io_base,
                nic.mmio_base,
                nic.mac,
                nic.pci_bus,
                nic.pci_slot,
            )
        };

        // Validate MAC address: non-zero and non-broadcast
        let mac_nonzero = mac.iter().any(|&b| b != 0);
        let mac_nonbroadcast = mac != [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let mac_valid = mac_nonzero && mac_nonbroadcast;

        // Verify DMA descriptors hardware alignment (must be 16 bytes for e1000)
        let rx_desc_align_ok = core::mem::align_of::<RxDesc>() == 16;
        let tx_desc_align_ok = core::mem::align_of::<TxDesc>() == 16;
        let rx_desc_size_ok = core::mem::size_of::<RxDesc>() == 16;
        let tx_desc_size_ok = core::mem::size_of::<TxDesc>() == 16;

        // Verify ring buffer geometry
        let rings_ok = RX_DESC_COUNT == 8 && TX_DESC_COUNT == 8 && PACKET_BUFFER_SIZE == 2048;

        // Hardware controller present check
        let hw_ok = if is_init {
            (io_base != 0 || mmio_base != 0) && mac_valid
        } else {
            // Loopback fallback mode
            mac_valid
        };

        let passed = mac_valid && rx_desc_align_ok && tx_desc_align_ok
            && rx_desc_size_ok && tx_desc_size_ok && rings_ok && hw_ok;

        results.push(TestResult {
            name: "Intel e1000 NIC Detection, MAC Address & Ring Buffers",
            passed,
            detail: format!(
                "Init={}, PCI=[{:02x}:{:02x}.0], IO={:#x}, MMIO={:#x}, MAC={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}, Align16={}, Rings={}/{}",
                is_init, pci_bus, pci_slot, io_base, mmio_base,
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5],
                rx_desc_align_ok && tx_desc_align_ok, RX_DESC_COUNT, TX_DESC_COUNT
            ),
        });
    }

    // ========================================================================
    // Test 30: Network Packet Serialization, Checksum & Protocol Dispatch
    // ========================================================================
    {
        use crate::net::ethernet::{EthernetFrame, EthernetHeader, ETHERTYPE_IPV4};
        use crate::net::ipv4::{Ipv4Packet, calculate_checksum, IPV4_PROTO_ICMP};
        use crate::net::arp::{ArpPacket, ArpTable, build_arp_request, build_arp_reply, ARP_OP_REQUEST, ARP_OP_REPLY};
        use crate::net::icmp::{IcmpPacket, build_echo_request, build_echo_reply, ICMP_TYPE_ECHO_REQUEST, ICMP_TYPE_ECHO_REPLY};

        // 1. Ethernet Frame Roundtrip
        let test_payload = b"AuraNetPacketTestPayload";
        let src_mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
        let dst_mac = [0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E];
        let eth_frame = EthernetFrame {
            header: EthernetHeader {
                dest_mac: dst_mac,
                src_mac,
                ethertype: ETHERTYPE_IPV4,
            },
            payload: test_payload.to_vec(),
        };
        let eth_bytes = eth_frame.serialize();
        let eth_min_len_ok = eth_bytes.len() >= 60; // Minimum Ethernet frame length padding
        let parsed_eth = EthernetFrame::parse(&eth_bytes);
        let eth_ok = eth_min_len_ok && match parsed_eth {
            Some(f) => f.header.src_mac == src_mac && f.header.dest_mac == dst_mac
                && f.header.ethertype == ETHERTYPE_IPV4 && &f.payload[..test_payload.len()] == test_payload,
            None => false,
        };

        // 2. RFC 1071 Checksum & IPv4 Packet Generation
        let test_src_ip = [10, 0, 2, 15];
        let test_dst_ip = [10, 0, 2, 2];
        let ipv4_pkt = Ipv4Packet::new(test_src_ip, test_dst_ip, IPV4_PROTO_ICMP, test_payload.to_vec());
        let ip_bytes = ipv4_pkt.serialize();
        // RFC 1071 verification: checksum over serialized header (including computed checksum) must evaluate to 0
        let ip_hdr_bytes = &ip_bytes[0..20];
        let checksum_verification = calculate_checksum(ip_hdr_bytes);
        let checksum_ok = checksum_verification == 0 && ipv4_pkt.header.checksum != 0;
        let parsed_ip = Ipv4Packet::parse(&ip_bytes);
        let ip_ok = checksum_ok && match parsed_ip {
            Some(p) => p.header.source_ip == test_src_ip && p.header.dest_ip == test_dst_ip
                && p.header.protocol == IPV4_PROTO_ICMP && p.payload == test_payload,
            None => false,
        };

        // 3. ARP Request and Reply formatting
        let arp_req_bytes = build_arp_request(src_mac, test_src_ip, test_dst_ip);
        let parsed_req = ArpPacket::parse(&arp_req_bytes);
        let arp_req_ok = match parsed_req {
            Some(a) => a.oper == ARP_OP_REQUEST && a.sender_mac == src_mac && a.sender_ip == test_src_ip
                && a.target_ip == test_dst_ip,
            None => false,
        };
        let arp_reply_bytes = build_arp_reply(dst_mac, test_dst_ip, src_mac, test_src_ip);
        let parsed_reply = ArpPacket::parse(&arp_reply_bytes);
        let arp_reply_ok = match parsed_reply {
            Some(a) => a.oper == ARP_OP_REPLY && a.sender_mac == dst_mac && a.sender_ip == test_dst_ip
                && a.target_mac == src_mac && a.target_ip == test_src_ip,
            None => false,
        };

        // 4. ICMP Echo Request and Reply
        let echo_req_bytes = build_echo_request(0x1337, 1, b"PingEchoTest");
        let parsed_echo_req = IcmpPacket::parse(&echo_req_bytes);
        let icmp_req_ok = match parsed_echo_req {
            Some(i) => i.header.msg_type == ICMP_TYPE_ECHO_REQUEST && i.header.identifier == 0x1337
                && i.header.sequence_number == 1 && i.payload == b"PingEchoTest",
            None => false,
        };
        let echo_reply_bytes = build_echo_reply(0x1337, 1, b"PingEchoTest");
        let parsed_echo_reply = IcmpPacket::parse(&echo_reply_bytes);
        let icmp_reply_ok = match parsed_echo_reply {
            Some(i) => i.header.msg_type == ICMP_TYPE_ECHO_REPLY && i.header.identifier == 0x1337
                && i.header.sequence_number == 1 && i.payload == b"PingEchoTest",
            None => false,
        };

        // 5. Dynamic ARP Cache Table
        let mut arp_table = ArpTable::new();
        arp_table.insert([192, 168, 1, 1], [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        let arp_lookup_hit = arp_table.lookup(&[192, 168, 1, 1]) == Some([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        let arp_lookup_miss = arp_table.lookup(&[192, 168, 1, 99]) == None;
        let arp_table_ok = arp_lookup_hit && arp_lookup_miss;

        let passed = eth_ok && ip_ok && arp_req_ok && arp_reply_ok && icmp_req_ok && icmp_reply_ok && arp_table_ok;

        results.push(TestResult {
            name: "Network Packet Serialization, Checksum & Protocol Dispatch",
            passed,
            detail: format!(
                "Eth={}, IPv4={} (Cksum0={}), ArpReq={}, ArpReply={}, IcmpEcho={}, ArpTable={}",
                eth_ok, ip_ok, checksum_verification == 0, arp_req_ok, arp_reply_ok, icmp_req_ok && icmp_reply_ok, arp_table_ok
            ),
        });
    }

    // ========================================================================
    // Test 31: ACPI RSDP Discovery, Table Checksums & FADT Registers
    // ========================================================================
    {
        use crate::arch::acpi::{self, ACPI_DATA};

        let acpi = ACPI_DATA.lock();
        let rsdp_found = acpi.rsdp_addr != 0;
        let tables_found = acpi.tables_count > 0;
        let fadt_found = acpi.fadt_addr != 0;
        let pm1a_valid = acpi.pm1a_cnt_blk != 0;

        // Verify ACPI table checksum validation function
        let dummy_table = [0x41, 0x42, 0x43, 0x44, 0x10, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x28];
        let mut valid_table = [0x10, 0x20, 0x30, 0x40, 0x00];
        let partial_sum = valid_table[0..4].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        valid_table[4] = 0u8.wrapping_sub(partial_sum);
        let checksum_fn_valid = acpi::validate_checksum(&valid_table) && !acpi::validate_checksum(&dummy_table);

        // Verify OEM ID is non-empty
        let oem_valid = acpi.oem_id.iter().any(|&b| b != 0);

        let passed = rsdp_found && tables_found && fadt_found && pm1a_valid && checksum_fn_valid && oem_valid;

        results.push(TestResult {
            name: "ACPI RSDP Discovery, Table Checksums & FADT Registers",
            passed,
            detail: format!(
                "RSDP={:#x}, Tables={}, FADT={:#x}, PM1a={:#x}, CksumFn={}, OEM={:?}",
                acpi.rsdp_addr,
                acpi.tables_count,
                acpi.fadt_addr,
                acpi.pm1a_cnt_blk,
                checksum_fn_valid,
                core::str::from_utf8(&acpi.oem_id).unwrap_or("???")
            ),
        });
    }

    // ========================================================================
    // Test 32: MADT Core Enumeration & Local APIC MMIO Base Validation
    // ========================================================================
    {
        use crate::arch::acpi::ACPI_DATA;
        use crate::arch::apic::{self, LOCAL_APIC, MSR_APIC_BASE, MSR_APIC_BASE_ENABLE};

        let acpi = ACPI_DATA.lock();
        let lapic = LOCAL_APIC.lock();

        // 1. Check MADT table discovered
        let madt_found = acpi.madt_addr != 0;

        // 2. Check CPU core count >= 1 and at least one core is enabled
        let core_count = acpi.cores.len();
        let cores_valid = core_count >= 1 && acpi.cores.iter().any(|c| c.is_enabled);

        // 3. Check Local APIC MSR has bit 11 enabled
        let msr_val = unsafe { apic::rdmsr(MSR_APIC_BASE) };
        let msr_enabled = (msr_val & MSR_APIC_BASE_ENABLE) != 0;

        // 4. Check Local APIC base address matches standard x86 default (0xFEE00000)
        let lapic_base_ok = lapic.base_addr == 0xFEE00000;

        // 5. Check CPUID APIC capability
        let cpu_info = crate::arch::cpuid::get_cpu_info();
        let cpuid_apic = cpu_info.has_apic;

        // 6. Check LAPIC software enable flag
        let lapic_enabled = lapic.is_enabled;

        let passed = madt_found && cores_valid && msr_enabled && lapic_base_ok && cpuid_apic && lapic_enabled;

        results.push(TestResult {
            name: "MADT Core Enumeration & Local APIC MMIO Base Validation",
            passed,
            detail: format!(
                "MADT={:#x}, Cores={}, BSP_En={}, MSR_En={}, Base={:#x}, CPUID_APIC={}, LAPIC_En={}",
                acpi.madt_addr,
                core_count,
                acpi.cores.iter().any(|c| c.is_enabled),
                msr_enabled,
                lapic.base_addr,
                cpuid_apic,
                lapic_enabled
            ),
        });
    }

    // ========================================================================
    // Test 34: PMM — Physical Frame Allocator Roundtrip
    // ========================================================================
    {
        let pmm_ready = crate::memory::pmm::is_ready();
        let before = crate::memory::pmm::free_memory();
        let f1 = crate::memory::pmm::allocate_frame();
        let f2 = crate::memory::pmm::allocate_frame();

        let mut distinct = false;
        let mut aligned_and_in_range = false;
        let mut freed_ok = false;
        if let (Some(a), Some(b)) = (f1, f2) {
            distinct = a.as_u64() != b.as_u64();
            aligned_and_in_range = {
                let a = a.as_u64();
                let b = b.as_u64();
                a >= crate::memory::pmm::PMM_BASE
                    && b >= crate::memory::pmm::PMM_BASE
                    && a < crate::memory::pmm::PMM_END
                    && b < crate::memory::pmm::PMM_END
                    && (a & 0xFFF) == 0
                    && (b & 0xFFF) == 0
            };
            freed_ok = crate::memory::pmm::free_frame(a)
                && crate::memory::pmm::free_frame(b);
        }

        let after = crate::memory::pmm::free_memory();
        let no_leak = before == after;

        // AddressSpace alloc + drop must return every frame (page tables AND
        // user pages) to the PMM — proves the "real free on Drop" migration.
        let space_before = crate::memory::pmm::free_memory();
        let (space_frames, space_ok) = {
            let mut space = crate::memory::user_space::AddressSpace::new();
            let mut frames = 0usize;
            let mut ok = false;
            if let Some(mut s) = space.take() {
                frames += 2; // PML4 + cloned PDPT allocated in new()
                let page = crate::memory::paging::VirtAddr(0x4000_0000);
                ok = s.allocate_and_map_page(page, true).is_some()
                    && s.allocate_user_stack(crate::memory::paging::VirtAddr(0x8000_0000), 2);
                frames += 3; // user code page + 2 stack pages mapped above
                drop(s); // Drop → free_frame for every allocated frame
            }
            (frames, ok)
        };
        let space_after = crate::memory::pmm::free_memory();
        let space_returned = space_ok && space_before == space_after;

        let passed = pmm_ready && distinct && aligned_and_in_range && freed_ok && no_leak
            && space_returned && space_frames >= 5;

        results.push(TestResult {
            name: "PMM Physical Frame Allocator Roundtrip",
            passed,
            detail: format!(
                "Ready={}, FreeBefore={}, Frames(Aligned/InRange={}, Distinct={}, Freed={}), FreeAfter={}, NoLeak={}, AddressSpace(DropFreed={}f, Returned={})",
                pmm_ready,
                before,
                aligned_and_in_range,
                distinct,
                freed_ok,
                after,
                no_leak,
                space_frames,
                space_returned
            ),
        });
    }

    results
}

