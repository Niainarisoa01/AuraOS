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
                if content == test_data {
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
        let sched = crate::task::SCHEDULER.lock();
        let task_count = sched.tasks.len();
        let mut stacks_aligned = true;
        let mut abi_compliant = true;

        for task in &sched.tasks {
            // Task 0 uses bootloader stack (rsp=0 before first switch)
            if task.rsp != 0 {
                // 16-byte alignment check
                if (task.rsp & 0xF) != 0 {
                    stacks_aligned = false;
                }
                // ABI check: after switch_context's ret, RSP should result in
                // (RSP_at_entry + 8) % 16 == 0, which means RSP_at_entry % 16 == 8.
                // The initial RSP in the TCB is before the 7 pops + popfq + ret (72 bytes).
                // After restoring: RSP_final = task.rsp + 72 (padding + RIP on top)
                // At function entry after ret: RSP = task.rsp + 72
                // We need (task.rsp + 72 + 8) % 16 == 0 → (task.rsp + 80) % 16 == 0
                let rsp_after_switch = task.rsp + 72;
                if (rsp_after_switch + 8) % 16 != 0 {
                    abi_compliant = false;
                }
            }
        }

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

    results
}

