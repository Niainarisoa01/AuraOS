//! ============================================================================
//! Physical Memory Manager (PMM) — E820 Memory Map & Frame Allocator
//! ============================================================================
//!
//! Implements a real physical memory manager on top of the BIOS E820 memory
//! map provided by the bootloader via `BootInfo.memory_map`.
//!
//! Design constraints (see docs/AVANTAGES_ET_INCONVENIENTS.md, item I5):
//! - The kernel runs without the `map_physical_memory` bootloader feature, so
//!   there is no global virtual offset for arbitrary physical memory. Instead,
//!   `memory::paging::init_hardware_mappings()` identity-maps the RAM range
//!   [2 MiB .. 512 MiB) using 2 MiB huge pages (virtual == physical).
//! - Consequently this PMM only allocates frames inside that identity-mapped
//!   window. A frame returned by `allocate_frame()` can be accessed directly
//!   by the kernel at its physical address (`addr as *mut u8`).
//!
//! The allocator is a simple bitmap: one bit per 4 KiB frame, bit = 0 means
//! free, bit = 1 means used. All frames are marked used at init, then the
//! ones belonging to `Usable` E820 regions intersecting the window are
//! released. Frames below 2 MiB and above 512 MiB are not allocatable but are
//! still counted in the memory statistics (derived from the real E820 map).

use core::sync::atomic::{AtomicUsize, Ordering};
use crate::memory::paging::{PhysAddr, PAGE_SIZE};
use crate::sync::Spinlock;

/// Start of the identity-mapped RAM window (2 MiB).
pub const PMM_BASE: u64 = 0x0020_0000;
/// End (exclusive) of the identity-mapped RAM window (512 MiB).
pub const PMM_END: u64 = 0x2000_0000;
/// Number of 4 KiB frames managed by the bitmap.
pub const PMM_FRAMES: usize = ((PMM_END - PMM_BASE) / PAGE_SIZE as u64) as usize; // 130 560
/// Number of 64-bit words in the frame bitmap (bit = 0 free, bit = 1 used).
const BITMAP_WORDS: usize = PMM_FRAMES.div_ceil(64); // 2 040

/// Maximum number of memory map regions we can store (matches the bootloader cap).
const MAX_REGIONS: usize = 64;

/// A simplified copy of one E820 memory region (decoupled from the bootloader types).
#[derive(Clone, Copy, Debug)]
pub struct PmmRegion {
    /// Start physical address (inclusive).
    pub start: u64,
    /// End physical address (exclusive).
    pub end: u64,
    /// True when the region was marked `Usable` by the firmware.
    pub usable: bool,
}

/// Runtime state of the physical memory manager.
pub struct Pmm {
    /// Copied E820 regions (kernel-owned, valid for the whole boot).
    regions: [PmmRegion; MAX_REGIONS],
    /// Number of valid entries in `regions`.
    region_count: usize,
    /// Total physical RAM reported usable by the firmware, in bytes.
    total_memory: u64,
    /// Highest physical address present in the memory map.
    max_phys_addr: u64,
    /// Frame bitmap: bit = 0 free, bit = 1 used. Only covers [PMM_BASE, PMM_END).
    bitmap: [u64; BITMAP_WORDS],
}

impl Pmm {
    /// Creates a PMM with an all-allocated bitmap (nothing allocatable yet).
    const fn new() -> Self {
        Pmm {
            regions: [PmmRegion { start: 0, end: 0, usable: false }; MAX_REGIONS],
            region_count: 0,
            total_memory: 0,
            max_phys_addr: 0,
            bitmap: [u64::MAX; BITMAP_WORDS], // everything marked used by default
        }
    }

    /// Marks the frame at `addr` as free if it belongs to a usable region.
    fn release_frame(&mut self, addr: u64) {
        if !(PMM_BASE..PMM_END).contains(&addr) {
            return;
        }
        let frame = ((addr - PMM_BASE) / PAGE_SIZE as u64) as usize;
        let word = frame / 64;
        let bit = frame % 64;
        self.bitmap[word] &= !(1u64 << bit);
    }

    /// Marks the frame at `addr` as used. Returns true on success.
    /// Explicit reservation API for special frames (DMA buffers, MMIO carve-outs).
    #[allow(dead_code)]
    fn reserve_frame(&mut self, addr: u64) -> bool {
        if !(PMM_BASE..PMM_END).contains(&addr) || (addr & 0xFFF) != 0 {
            return false;
        }
        let frame = ((addr - PMM_BASE) / PAGE_SIZE as u64) as usize;
        let word = frame / 64;
        let bit = frame % 64;
        let mask = 1u64 << bit;
        if (self.bitmap[word] & mask) != 0 {
            return false; // already used
        }
        self.bitmap[word] |= mask;
        true
    }

    /// Finds the first free frame and reserves it. Returns its physical address.
    fn alloc_frame(&mut self) -> Option<PhysAddr> {
        for (word_idx, word) in self.bitmap.iter_mut().enumerate() {
            if *word != u64::MAX {
                let free = !*word;
                let bit = free.trailing_zeros() as usize;
                *word |= 1u64 << bit;
                let frame = word_idx * 64 + bit;
                let addr = PMM_BASE + (frame as u64 * PAGE_SIZE as u64);
                return Some(PhysAddr(addr));
            }
        }
        None
    }

    /// Releases a previously allocated frame back to the free pool.
    fn free_frame(&mut self, addr: PhysAddr) -> bool {
        let addr = addr.as_u64();
        if !(PMM_BASE..PMM_END).contains(&addr) || (addr & 0xFFF) != 0 {
            return false;
        }
        let frame = ((addr - PMM_BASE) / PAGE_SIZE as u64) as usize;
        let word = frame / 64;
        let bit = frame % 64;
        let mask = 1u64 << bit;
        if (self.bitmap[word] & mask) == 0 {
            return false; // not allocated (double free)
        }
        self.bitmap[word] &= !mask;
        true
    }
}

/// Global physical memory manager state.
static PMM: Spinlock<Pmm> = Spinlock::new(Pmm::new());

/// Initialized flag (also doubles as "PMM ready" for stats consumers).
static PMM_READY: AtomicUsize = AtomicUsize::new(0);

/// Initializes the physical memory manager from a unified memory map.
///
/// Called very early during boot (step 0b of `kernel_main`), before the kernel heap
/// is set up. Only writable kernel-statics are touched here, so no dependency
/// on the identity-mapped RAM window exists yet.
pub fn init(memory_map: &[crate::boot::MemoryRegion]) {
    let mut pmm = PMM.lock();

    let mut total = 0u64;
    let mut max_addr = 0u64;
    let mut count = 0usize;

    for region in memory_map.iter() {
        let start = region.start;
        let end = region.end;
        if count < MAX_REGIONS {
            pmm.regions[count] = PmmRegion {
                start,
                end,
                usable: region.is_usable(),
            };
            count += 1;
        }
        if end > max_addr {
            max_addr = end;
        }
        if region.is_usable() {
            total += end - start;
        }
    }
    pmm.region_count = count;
    pmm.total_memory = total;
    pmm.max_phys_addr = max_addr;

    // Release every frame belonging to an Usable region intersecting the window.
    pmm.bitmap = [u64::MAX; BITMAP_WORDS];
    // Copy the (small) usable-region list to avoid an aliasing borrow conflict
    // between iterating `pmm.regions` and calling `pmm.release_frame`.
    let mut usable: [(u64, u64); MAX_REGIONS] = [(0, 0); MAX_REGIONS];
    let mut usable_count = 0;
    for region in &pmm.regions[..count] {
        if region.usable {
            usable[usable_count] = (region.start, region.end);
            usable_count += 1;
        }
    }
    for &(start, end) in &usable[..usable_count] {
        let start = start.max(PMM_BASE);
        let end = end.min(PMM_END);
        let mut addr = (start + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1); // align up
        while addr < end {
            pmm.release_frame(addr);
            addr += PAGE_SIZE as u64;
        }
    }

    let usable_mib = total / (1024 * 1024);
    let free_mib = pmm.free_memory_locked() / (1024 * 1024);
    let used_mib = pmm.used_memory_locked() / (1024 * 1024);
    let regions_all = count;

    drop(pmm);
    PMM_READY.store(1, Ordering::SeqCst);

    crate::println!("[OK] Memory     : Physical RAM: {} MiB detected (E820, {} regions), {} MiB free frames.",
        usable_mib, regions_all, free_mib);
    crate::klog!(Info, "pmm", "E820 map loaded: {} usable regions, {} MiB total, {} MiB free ({} MiB used).",
        regions_all, usable_mib, free_mib, used_mib);
}

impl Pmm {
    /// Helper: free memory computed on the locked instance.
    fn free_memory_locked(&self) -> u64 {
        let mut free = 0u64;
        for word in &self.bitmap {
            free += (!word).count_ones() as u64;
        }
        free * PAGE_SIZE as u64
    }

    /// Helper: used memory computed on the locked instance.
    ///
    /// Used = detected usable RAM - currently free frames inside the window.
    /// (On machines with more RAM than the identity-mapped window, the excess
    /// is not allocatable by the PMM and therefore counts as "used".)
    fn used_memory_locked(&self) -> u64 {
        self.total_memory.saturating_sub(self.free_memory_locked())
    }
}

/// Allocates a single 4 KiB physical frame inside the identity-mapped window.
///
/// The returned frame is zeroing-free: the caller is responsible for clearing
/// it before use. Frames are aligned to 4 KiB by construction.
pub fn allocate_frame() -> Option<PhysAddr> {
    PMM.lock().alloc_frame()
}

/// Releases a frame back to the free pool. Returns false for invalid/double frees.
pub fn free_frame(addr: PhysAddr) -> bool {
    PMM.lock().free_frame(addr)
}

/// Total usable physical RAM reported by the firmware (E820), in bytes.
pub fn total_memory() -> u64 {
    PMM.lock().total_memory
}

/// Total free physical memory in the allocatable window, in bytes.
pub fn free_memory() -> u64 {
    PMM.lock().free_memory_locked()
}

/// Used physical memory inside the allocatable window, in bytes.
pub fn used_memory() -> u64 {
    PMM.lock().used_memory_locked()
}

/// Highest physical address present in the memory map.
#[allow(dead_code)]
pub fn max_phys_addr() -> u64 {
    PMM.lock().max_phys_addr
}

/// Number of E820 regions captured at boot.
#[allow(dead_code)]
pub fn region_count() -> usize {
    PMM.lock().region_count
}

/// Returns true once `init()` has run.
pub fn is_ready() -> bool {
    PMM_READY.load(Ordering::SeqCst) == 1
}