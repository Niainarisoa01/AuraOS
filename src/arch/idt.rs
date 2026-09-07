//! ============================================================================
//! IDT — Interrupt Descriptor Table
//! ============================================================================
//!
//! The IDT is the table consulted by the CPU whenever an interrupt or exception occurs
//! (CPU faults, keyboard keystrokes, timer ticks, system calls, etc.).
//!
//! Each IDT entry points to a handler function executed automatically by the CPU.
//! In x86_64, each entry is 16 bytes wide and contains the handler address,
//! segment selector, and gate attributes.

use core::cell::UnsafeCell;

/// 16-byte IDT entry structure in 64-bit Long Mode.
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct IdtEntry {
    offset_low: u16,       // Handler address (bits 0-15)
    selector: u16,         // Code segment selector (0x08 = Ring 0 Kernel)
    ist: u8,               // Interrupt Stack Table index (0 = disabled)
    attributes: u8,        // Gate type, DPL, Present bit
    offset_mid: u16,       // Handler address (bits 16-31)
    offset_high: u32,      // Handler address (bits 32-63)
    reserved: u32,         // Reserved by Intel/AMD, must be zero
}

impl IdtEntry {
    /// Creates an empty (unconfigured/missing) IDT entry.
    pub const fn missing() -> Self {
        IdtEntry {
            offset_low: 0,
            selector: 0,
            ist: 0,
            attributes: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    /// Configures an IDT entry to point to the given interrupt service routine (ISR).
    ///   - `handler`: Address of the ISR function
    ///   - Code selector is set to 0x08 (GDT Kernel Code Segment)
    ///   - Attributes set to 0x8E (64-bit Interrupt Gate, Present, DPL 0)
    pub fn set_handler(&mut self, handler: u64) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = 0x08;   // Kernel code segment (GDT index 1)
        self.ist = 0;
        self.attributes = 0x8E; // 64-bit Interrupt Gate, Present, DPL=0
        self.reserved = 0;
    }
}

/// The IDTR pointer structure expected by the `lidt` instruction.
#[repr(C, packed)]
pub struct IdtPointer {
    pub limit: u16,
    pub base: u64,
}

/// Maximum number of IDT entries supported by x86_64.
const IDT_ENTRIES: usize = 256;

/// Thread-safe wrapper for the static IDT table.
struct IdtWrapper(UnsafeCell<[IdtEntry; IDT_ENTRIES]>);
unsafe impl Sync for IdtWrapper {}

/// Static IDT table (256 entries, all initialized to empty).
static IDT: IdtWrapper = IdtWrapper(UnsafeCell::new([IdtEntry::missing(); IDT_ENTRIES]));

/// CPU Exception Stack Frame automatically pushed by the processor upon interrupt.
#[repr(C)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64, // RIP
    pub code_segment: u64,        // CS
    pub cpu_flags: u64,           // RFLAGS
    pub stack_pointer: u64,       // RSP
    pub stack_segment: u64,       // SS
}

// ============================================================================
// CPU Exception Handlers (Faults & Traps)
// ============================================================================

/// Exception 0: Divide-by-Zero (#DE).
extern "x86-interrupt" fn divide_by_zero_handler(frame: InterruptStackFrame) {
    crate::serial_println!("\n[FATAL CPU EXCEPTION] Divide by Zero (#DE) at RIP: {:#x}", frame.instruction_pointer);
    crate::println!("\n[FATAL CPU EXCEPTION] Divide by Zero (#DE)");
    crate::println!("  Faulting RIP: {:#x}", frame.instruction_pointer);
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

/// Exception 6: Invalid Opcode (#UD).
extern "x86-interrupt" fn invalid_opcode_handler(frame: InterruptStackFrame) {
    crate::serial_println!("\n[FATAL CPU EXCEPTION] Invalid Opcode (#UD) at RIP: {:#x}", frame.instruction_pointer);
    crate::println!("\n[FATAL CPU EXCEPTION] Invalid Opcode (#UD)");
    crate::println!("  Faulting RIP: {:#x}", frame.instruction_pointer);
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

/// Exception 8: Double Fault (#DF).
extern "x86-interrupt" fn double_fault_handler(frame: InterruptStackFrame, error_code: u64) -> ! {
    crate::serial_println!("\n[CRITICAL CPU EXCEPTION] Double Fault (#DF) Code: {:#x}, RIP: {:#x}", error_code, frame.instruction_pointer);
    crate::println!("\n[CRITICAL CPU EXCEPTION] Double Fault (#DF)");
    crate::println!("  Error Code  : {:#x}", error_code);
    crate::println!("  Faulting RIP: {:#x}", frame.instruction_pointer);
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

/// Exception 13: General Protection Fault (#GP).
extern "x86-interrupt" fn general_protection_fault_handler(frame: InterruptStackFrame, error_code: u64) {
    crate::serial_println!("\n[FATAL CPU EXCEPTION] General Protection Fault (#GP) Code: {:#x}, RIP: {:#x}", error_code, frame.instruction_pointer);
    crate::println!("\n[FATAL CPU EXCEPTION] General Protection Fault (#GP)");
    crate::println!("  Error Code  : {:#x}", error_code);
    crate::println!("  Faulting RIP: {:#x}", frame.instruction_pointer);
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

/// Exception 14: Page Fault (#PF).
extern "x86-interrupt" fn page_fault_handler(frame: InterruptStackFrame, error_code: u64) {
    let faulting_address: u64;
    unsafe {
        core::arch::asm!("mov {}, cr2", out(reg) faulting_address, options(nomem, nostack, preserves_flags));
    }
    crate::serial_println!("\n[FATAL CPU EXCEPTION] Page Fault (#PF) at Addr: {:#x}, Flags: {:#b}, RIP: {:#x}",
        faulting_address, error_code, frame.instruction_pointer);
    crate::println!("\n[FATAL CPU EXCEPTION] Page Fault (#PF)");
    crate::println!("  Accessed Address (CR2) : {:#x}", faulting_address);
    crate::println!("  Error Code Flags       : {:#b}", error_code);
    crate::println!("  Faulting RIP           : {:#x}", frame.instruction_pointer);
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

// ============================================================================
// Hardware Interrupt Handlers (IRQs)
// ============================================================================

/// System timer tick counter (triggered ~18.2 times/sec by default via PIT).
static TICKS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// IRQ 0: System Timer (8254 PIT).
extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame) {
    TICKS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    crate::task::timer_tick();
    super::pic::send_eoi(super::pic::IRQ_TIMER);
}

/// IRQ 1: PS/2 Keyboard Keystroke.
extern "x86-interrupt" fn keyboard_handler(_frame: InterruptStackFrame) {
    crate::drivers::keyboard::handle_interrupt();
    super::pic::send_eoi(super::pic::IRQ_KEYBOARD);
}

/// Returns the number of system timer ticks elapsed since boot.
#[allow(dead_code)]
pub fn ticks() -> u64 {
    TICKS.load(core::sync::atomic::Ordering::Relaxed)
}

// ============================================================================
// IDT Initialization
// ============================================================================

/// Registers exception and hardware interrupt handlers into the IDT,
/// then loads the table into the CPU's IDTR register via `lidt`.
pub fn init() {
    let idt = IDT.0.get();
    unsafe {
        // CPU Exceptions (0..31)
        (*idt)[0].set_handler(divide_by_zero_handler as *const () as u64);
        (*idt)[6].set_handler(invalid_opcode_handler as *const () as u64);
        (*idt)[8].set_handler(double_fault_handler as *const () as u64);
        (*idt)[13].set_handler(general_protection_fault_handler as *const () as u64);
        (*idt)[14].set_handler(page_fault_handler as *const () as u64);

        // Hardware IRQs (32..47)
        (*idt)[super::pic::PIC1_OFFSET as usize + super::pic::IRQ_TIMER as usize]
            .set_handler(timer_handler as *const () as u64);
        (*idt)[super::pic::PIC1_OFFSET as usize + super::pic::IRQ_KEYBOARD as usize]
            .set_handler(keyboard_handler as *const () as u64);

        let idt_ptr = IdtPointer {
            limit: (core::mem::size_of::<[IdtEntry; IDT_ENTRIES]>() - 1) as u16,
            base: idt as u64,
        };

        core::arch::asm!(
            "lidt [{}]",
            in(reg) &idt_ptr,
            options(readonly, nostack, preserves_flags)
        );
    }

    crate::println!("[OK] IDT       : 5 CPU exceptions + IRQ0 (Timer) + IRQ1 (Keyboard) active.");
}
