//! ============================================================================
//! x86_64 Architecture Layer
//! ============================================================================
//!
//! Encapsulates all CPU-level structures, registers, hardware interrupts,
//! and low-level port I/O primitives.

pub mod io;
pub mod gdt;
pub mod idt;
pub mod pic;
pub mod cpuid;
