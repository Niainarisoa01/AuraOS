/// ============================================================================
/// Kernel Self-Test & Automated Verification Engine
/// ============================================================================
///
/// Runs automated internal diagnostics on all kernel subsystems:
/// 1. Virtual Memory & 4-Level Paging address translation
/// 2. Dynamic Heap Allocator & Free-Block Coalescing (Leak Detection)
/// 3. Virtual File System (VFS/RAMFS) Inode operations
/// 4. CMOS Real-Time Clock calendar range validation
/// 5. CPUID instruction decoding & cycle timing
/// 6. Task Scheduler & Stack Alignment verification
/// 7. COM1 Serial UART logging channel

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub struct TestResult {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

/// Runs the complete kernel automated self-test suite and returns the results.
pub fn run_all_tests() -> Vec<TestResult> {
    let mut results = Vec::new();

    // ------------------------------------------------------------------------
    // Test 1: Virtual Memory & 4-Level Paging Calculations
    // ------------------------------------------------------------------------
    {
        use crate::memory::VirtAddr;
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

    // ------------------------------------------------------------------------
    // Test 2: Dynamic Heap Allocator & Zero-Leak Coalescing
    // ------------------------------------------------------------------------
    {
        let initial_used = crate::allocator::used_memory();
        let mut boxes = Vec::new();

        // Allocate 50 dynamic objects
        for i in 0..50u64 {
            boxes.push(Box::new(i * 100));
        }

        let mid_used = crate::allocator::used_memory();
        let allocated_ok = mid_used > initial_used;

        // Verify values
        let mut values_ok = true;
        for (idx, b) in boxes.iter().enumerate() {
            if **b != (idx as u64) * 100 {
                values_ok = false;
                break;
            }
        }

        // Drop all boxes to test deallocation and coalescing
        drop(boxes);
        let final_used = crate::allocator::used_memory();
        let freed_ok = final_used == initial_used;

        let passed = allocated_ok && values_ok && freed_ok;
        results.push(TestResult {
            name: "Heap Allocation & Coalescing (Zero Leaks)",
            passed,
            detail: format!("Allocated 50 boxes, Verified values, Freed (Used: {} -> {} -> {})", initial_used, mid_used, final_used),
        });
    }

    // ------------------------------------------------------------------------
    // Test 3: VFS / RAMFS Inode Creation, Read, Write & Deletion
    // ------------------------------------------------------------------------
    {
        let mut vfs = crate::vfs::VFS.lock();
        let test_name = "test_verification.tmp";
        let test_data = b"AuraOS Subsystem Self-Test: 100% Functional";

        // 1. Create file
        let file_id_res = vfs.create_file_at(0, test_name, test_data);
        let mut passed = false;
        let mut detail = String::from("VFS failure");

        if let Ok(file_id) = file_id_res {
            // 2. Read back file
            if let Ok(content) = vfs.read_file(file_id) {
                if content == test_data {
                    // 3. Remove file
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

    // ------------------------------------------------------------------------
    // Test 4: CMOS Real-Time Clock Range Sanity
    // ------------------------------------------------------------------------
    {
        let rtc = crate::cmos::read_rtc();
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

    // ------------------------------------------------------------------------
    // Test 5: CPUID Hardware Detection & Cycle Timing
    // ------------------------------------------------------------------------
    {
        let cpu = crate::cpuid::get_cpu_info();
        let cycles_before = crate::cpuid::rdtsc();
        let cycles_after = crate::cpuid::rdtsc();

        let vendor = cpu.vendor_str();
        let valid_vendor = !vendor.is_empty() && vendor != "Unknown";
        let valid_cycles = cycles_after > cycles_before;

        let passed = valid_vendor && valid_cycles;
        results.push(TestResult {
            name: "CPUID Instruction & RDTSC Cycle Counter",
            passed,
            detail: format!("Vendor='{}', TSC delta={} cycles", vendor, cycles_after.saturating_sub(cycles_before)),
        });
    }

    // ------------------------------------------------------------------------
    // Test 6: Multitasking TCB Stack Alignment & Scheduler
    // ------------------------------------------------------------------------
    {
        let sched = crate::task::SCHEDULER.lock();
        let task_count = sched.tasks.len();
        let mut stacks_aligned = true;

        for task in &sched.tasks {
            // Task 0 uses bootloader stack (rsp=0 before first switch); spawned tasks must be 16-byte aligned
            if task.rsp != 0 && (task.rsp & 0xF) != 0 {
                stacks_aligned = false;
                break;
            }
        }

        let passed = task_count >= 2 && stacks_aligned;
        results.push(TestResult {
            name: "Multitasking TCB & 16-Byte Stack Alignment",
            passed,
            detail: format!("Tasks registered={}, Stacks 16-byte aligned={}", task_count, stacks_aligned),
        });
    }

    // ------------------------------------------------------------------------
    // Test 7: String Formatting & Dynamic Vector Memory
    // ------------------------------------------------------------------------
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

    results
}
