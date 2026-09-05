/// ============================================================================
/// IDT — Interrupt Descriptor Table
/// ============================================================================
///
/// The IDT is the table consulted by the CPU whenever an interrupt or exception occurs
/// (CPU faults, keyboard keystrokes, timer ticks, system calls, etc.).
///
/// Each IDT entry points to a handler function executed automatically by the CPU.
/// In x86_64, each entry is 16 bytes wide and contains the handler address,
/// segment selector, and gate attributes.

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

use core::cell::UnsafeCell;

/// Thread-safe wrapper for the static IDT table.
struct IdtWrapper(UnsafeCell<[IdtEntry; IDT_ENTRIES]>);
unsafe impl Sync for IdtWrapper {}

/// Static IDT table (256 entries, all initialized to empty).
static IDT: IdtWrapper = IdtWrapper(UnsafeCell::new([IdtEntry::missing(); IDT_ENTRIES]));

// ============================================================================
// CPU-Saved Interrupt Stack Frame
// ============================================================================

/// The Interrupt Stack Frame is automatically pushed onto the kernel stack
/// by the CPU prior to invoking an interrupt handler. It preserves the state
/// of the interrupted execution context.
#[derive(Debug)]
#[repr(C)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,    // RIP: interrupted instruction pointer
    pub code_segment: u64,           // CS: code segment selector
    pub cpu_flags: u64,              // RFLAGS: processor status flags
    pub stack_pointer: u64,          // RSP: stack pointer
    pub stack_segment: u64,          // SS: stack segment selector
}

// ============================================================================
// Handlers for Critical CPU Exceptions
// ============================================================================

/// Exception 0: Divide by Zero.
/// Raised when code attempts to divide a number by zero.
extern "x86-interrupt" fn divide_by_zero_handler(frame: InterruptStackFrame) {
    crate::println!("\n[EXCEPTION] Division by Zero!");
    crate::println!("  Faulting RIP : {:#x}", frame.instruction_pointer);
    crate::println!("  Kernel halted for safety.");
    loop {}
}

/// Exception 6: Invalid Opcode.
/// Raised when the processor encounters an unknown or undefined instruction.
extern "x86-interrupt" fn invalid_opcode_handler(frame: InterruptStackFrame) {
    crate::println!("\n[EXCEPTION] Invalid Opcode (unknown instruction)!");
    crate::println!("  Faulting RIP : {:#x}", frame.instruction_pointer);
    loop {}
}

/// Exception 8: Double Fault.
/// Raised when an exception occurs while trying to invoke a prior exception handler.
/// If unhandled, the processor escalates to a Triple Fault (instant reboot).
extern "x86-interrupt" fn double_fault_handler(frame: InterruptStackFrame, _error_code: u64) -> ! {
    crate::println!("\n[CRITICAL EXCEPTION] DOUBLE FAULT!");
    crate::println!("  Faulting RIP : {:#x}", frame.instruction_pointer);
    crate::println!("  Kernel cannot recover. System halted.");
    loop {}
}

/// Exception 13: General Protection Fault (GPF).
/// Raised on privilege level violations or illegal memory access attempts.
extern "x86-interrupt" fn general_protection_fault_handler(frame: InterruptStackFrame, error_code: u64) {
    crate::println!("\n[EXCEPTION] General Protection Fault (GPF)!");
    crate::println!("  Error code   : {:#x}", error_code);
    crate::println!("  Faulting RIP : {:#x}", frame.instruction_pointer);
    loop {}
}

/// Exception 14: Page Fault.
/// Raised when accessing an unmapped or protected virtual memory address.
extern "x86-interrupt" fn page_fault_handler(frame: InterruptStackFrame, error_code: u64) {
    // Register CR2 holds the linear faulting address
    let cr2: u64;
    unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nostack, preserves_flags)) };

    crate::println!("\n[EXCEPTION] Page Fault (invalid memory access)!");
    crate::println!("  Faulting Address (CR2) : {:#x}", cr2);
    crate::println!("  Error Code             : {:#x}", error_code);
    crate::println!("  Faulting RIP           : {:#x}", frame.instruction_pointer);
    loop {}
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
    crate::pic::send_eoi(crate::pic::IRQ_TIMER);
}

/// IRQ 1: PS/2 Keyboard Keystroke.
extern "x86-interrupt" fn keyboard_handler(_frame: InterruptStackFrame) {
    crate::keyboard::handle_interrupt();
    crate::pic::send_eoi(crate::pic::IRQ_KEYBOARD);
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
        (*idt)[crate::pic::PIC1_OFFSET as usize + crate::pic::IRQ_TIMER as usize]
            .set_handler(timer_handler as *const () as u64);
        (*idt)[crate::pic::PIC1_OFFSET as usize + crate::pic::IRQ_KEYBOARD as usize]
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
