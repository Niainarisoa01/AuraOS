//! ============================================================================
//! Dynamic Memory Allocator — Bare-Metal Linked List Heap Allocator
//! ============================================================================
//!
//! Implements `core::alloc::GlobalAlloc` to enable dynamic heap allocations
//! (`Box`, `Vec`, `String`, `format!`) in pure `#![no_std]` Rust without external crates.
//!
//! Uses a First-Fit Linked List strategy with automatic coalescing (merging)
//! of adjacent free blocks upon deallocation to prevent fragmentation.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr;
use crate::sync::{Spinlock, SpinlockGuard};

/// Total heap size: 8 MiB (supports 1024x768 32-bit TrueColor canvas & collections).
pub const HEAP_SIZE: usize = 8 * 1024 * 1024;

/// Backing memory buffer for the kernel heap, aligned to 4096 bytes.
#[repr(align(4096))]
struct HeapMemory(UnsafeCell<[u8; HEAP_SIZE]>);
unsafe impl Sync for HeapMemory {}

static HEAP_STORAGE: HeapMemory = HeapMemory(UnsafeCell::new([0; HEAP_SIZE]));

/// Align an address upward to the given power-of-two alignment.
fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

/// A node in the free memory linked list.
/// Stored directly in the unused memory of each free block.
struct ListNode {
    size: usize,
    next: Option<&'static mut ListNode>,
}

impl ListNode {
    const fn new(size: usize) -> Self {
        ListNode { size, next: None }
    }

    fn start_addr(&self) -> usize {
        self as *const Self as usize
    }

    fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
}

/// A Linked List Allocator managing a pool of free memory blocks.
pub struct LinkedListAllocator {
    head: ListNode,
}

impl LinkedListAllocator {
    /// Creates an empty allocator with an uninitialized free list.
    pub const fn new() -> Self {
        Self {
            head: ListNode::new(0),
        }
    }

    /// Initializes the allocator with a given memory bounds.
    ///
    /// # Safety
    /// The caller must ensure the memory region `[heap_start, heap_start + heap_size)`
    /// is valid, unused, and not referenced by anything else.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        unsafe {
            self.add_free_region(heap_start, heap_size);
        }
    }

    /// Adds a freed memory region to the free list, inserting it in sorted order by address
    /// and coalescing (merging) with adjacent free blocks where possible.
    unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
        // Ensure the freed region can hold at least a ListNode header
        if size < core::mem::size_of::<ListNode>() {
            return;
        }

        let node_ptr = addr as *mut ListNode;
        unsafe {
            node_ptr.write(ListNode::new(size));
        }
        let new_node = unsafe { &mut *node_ptr };

        // Insert into the linked list sorted by memory address
        let mut current = &mut self.head;
        while let Some(ref mut next) = current.next {
            if new_node.start_addr() < next.start_addr() {
                break;
            }
            current = current.next.as_mut().unwrap();
        }

        new_node.next = current.next.take();
        current.next = Some(new_node);

        // Coalesce (merge) adjacent free regions
        self.coalesce();
    }

    /// Merges contiguous free memory blocks to prevent fragmentation.
    fn coalesce(&mut self) {
        let mut current = &mut self.head;
        while current.next.is_some() {
            let current_end = current.end_addr();
            let can_merge = if let Some(ref next) = current.next {
                current.size > 0 && current_end == next.start_addr()
            } else {
                false
            };

            if can_merge {
                let next_node = current.next.take().unwrap();
                current.size += next_node.size;
                current.next = next_node.next.take();
            } else {
                current = current.next.as_mut().unwrap();
            }
        }
    }

    /// Finds a free region capable of fitting the requested layout (First-Fit).
    fn find_region(&mut self, size: usize, align: usize) -> Option<(&'static mut ListNode, usize)> {
        let mut current = &mut self.head;

        while let Some(ref mut region) = current.next {
            if let Ok(alloc_start) = Self::alloc_from_region(region, size, align) {
                let next = region.next.take();
                let ret = current.next.take().unwrap();
                current.next = next;
                return Some((ret, alloc_start));
            } else {
                current = current.next.as_mut().unwrap();
            }
        }
        None
    }

    /// Checks if a region can satisfy the allocation request.
    fn alloc_from_region(region: &ListNode, size: usize, align: usize) -> Result<usize, ()> {
        let alloc_start = align_up(region.start_addr(), align);
        let alloc_end = alloc_start.checked_add(size).ok_or(())?;

        if alloc_end > region.end_addr() {
            return Err(());
        }

        let excess_prefix = alloc_start - region.start_addr();
        if excess_prefix > 0 && excess_prefix < core::mem::size_of::<ListNode>() {
            // Cannot hold a ListNode in the prefix space
            return Err(());
        }

        let excess_suffix = region.end_addr() - alloc_end;
        if excess_suffix > 0 && excess_suffix < core::mem::size_of::<ListNode>() {
            // Cannot hold a ListNode in the suffix space
            return Err(());
        }

        Ok(alloc_start)
    }

    /// Allocates memory conforming to the given layout.
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        // Enforce minimum allocation size to hold a ListNode upon deallocation
        let (size, align) = Self::size_align(layout);

        if let Some((region, alloc_start)) = self.find_region(size, align) {
            let alloc_end = alloc_start + size;
            let excess_suffix = region.end_addr() - alloc_end;
            let excess_prefix = alloc_start - region.start_addr();

            if excess_suffix > 0 {
                unsafe {
                    self.add_free_region(alloc_end, excess_suffix);
                }
            }

            if excess_prefix > 0 {
                unsafe {
                    self.add_free_region(region.start_addr(), excess_prefix);
                }
            }

            alloc_start as *mut u8
        } else {
            ptr::null_mut()
        }
    }

    /// Deallocates the memory previously allocated.
    pub fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let (size, _) = Self::size_align(layout);
        unsafe {
            self.add_free_region(ptr as usize, size);
        }
    }

    /// Adjusts layout to satisfy minimum node size and alignment requirements.
    fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(core::mem::align_of::<ListNode>())
            .expect("Adjusting alignment failed")
            .pad_to_align();
        let size = layout.size().max(core::mem::size_of::<ListNode>());
        (size, layout.align())
    }

    /// Calculates total free memory currently available in the heap.
    pub fn free_memory(&self) -> usize {
        let mut total = 0;
        let mut current = &self.head;
        while let Some(ref next) = current.next {
            total += next.size;
            current = next;
        }
        total
    }
}

/// Global allocator wrapper around the Spinlock-protected LinkedListAllocator.
pub struct LockedHeap(Spinlock<LinkedListAllocator>);

impl LockedHeap {
    pub const fn empty() -> Self {
        LockedHeap(Spinlock::new(LinkedListAllocator::new()))
    }

    pub fn lock(&self) -> SpinlockGuard<'_, LinkedListAllocator> {
        self.0.lock()
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.lock().alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.lock().dealloc(ptr, layout);
    }
}

/// Global kernel heap allocator registered with Rust's runtime.
#[global_allocator]
pub static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Initializes the kernel heap with the dedicated 8 MiB static buffer.
pub fn init_heap() {
    let heap_start = HEAP_STORAGE.0.get() as usize;
    unsafe {
        ALLOCATOR.lock().init(heap_start, HEAP_SIZE);
    }
    crate::println!("[OK] Heap      : 8 MiB Linked List Allocator initialized.");
}

/// Returns the number of used heap bytes.
pub fn used_memory() -> usize {
    HEAP_SIZE.saturating_sub(ALLOCATOR.lock().free_memory())
}

/// Returns the number of free heap bytes.
pub fn free_memory() -> usize {
    ALLOCATOR.lock().free_memory()
}
