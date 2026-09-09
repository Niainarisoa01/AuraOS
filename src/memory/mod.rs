//! ============================================================================
//! Memory Management Subsystem Layer
//! ============================================================================
//!
//! Encapsulates:
//! - x86_64 4-Level Paging abstractions (PML4, PDPT, PD, PT) and CR3 inspection
//! - 8 MiB Coalescing Linked List Dynamic Heap Allocator (GlobalAlloc)

pub mod paging;
pub mod allocator;
pub mod user_space;
