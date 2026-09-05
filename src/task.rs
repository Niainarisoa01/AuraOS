/// ============================================================================
/// Kernel Multitasking & Task Context Switching (TCB & Scheduler)
/// ============================================================================
///
/// Implements pure Rust cooperative multitasking with Task Control Blocks (TCB),
/// stack allocation, round-robin scheduling, and assembly context switching.
///
/// In x86_64 Long Mode System V ABI:
/// - Callee-saved registers: r15, r14, r13, r12, rbx, rbp, rflags
/// - Context switch saves caller registers on current stack and restores target stack.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::vga_buffer::Spinlock;

const STACK_SIZE: usize = 16 * 1024; // 16 KiB per kernel thread stack

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Sleeping,
    Dead,
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Ready => "READY",
            TaskState::Running => "RUNNING",
            TaskState::Sleeping => "SLEEPING",
            TaskState::Dead => "DEAD",
        }
    }
}

/// Task Control Block (TCB)
#[allow(dead_code)]
pub struct Task {
    pub id: usize,
    pub name: &'static str,
    pub rsp: usize,
    pub stack: Option<Box<[u8]>>,
    pub state: TaskState,
    pub ticks: u64,
}

impl Task {
    /// Creates a new kernel task with its own dedicated 16 KiB stack.
    pub fn new(id: usize, name: &'static str, entry_point: extern "C" fn()) -> Self {
        let stack = vec![0u8; STACK_SIZE].into_boxed_slice();
        let stack_top = stack.as_ptr() as usize + STACK_SIZE;

        // Ensure 16-byte alignment
        let aligned_top = stack_top & !0xF;

        // Construct initial stack frame for switch_context:
        // [top - 8]:  RIP (entry_point)
        // [top - 16]: R15 (0)
        // [top - 24]: R14 (0)
        // [top - 32]: R13 (0)
        // [top - 40]: R12 (0)
        // [top - 48]: RBX (0)
        // [top - 56]: RBP (0)
        // [top - 64]: RFLAGS (0x202 = Interrupts Enabled)
        let frame_ptr = (aligned_top - 64) as *mut u64;
        unsafe {
            *frame_ptr.add(7) = entry_point as usize as u64; // RIP
            *frame_ptr.add(6) = 0;                           // R15
            *frame_ptr.add(5) = 0;                           // R14
            *frame_ptr.add(4) = 0;                           // R13
            *frame_ptr.add(3) = 0;                           // R12
            *frame_ptr.add(2) = 0;                           // RBX
            *frame_ptr.add(1) = 0;                           // RBP
            *frame_ptr.add(0) = 0x202;                       // RFLAGS (IF=1)
        }

        let initial_rsp = (aligned_top - 64) as usize;

        Task {
            id,
            name,
            rsp: initial_rsp,
            stack: Some(stack),
            state: TaskState::Ready,
            ticks: 0,
        }
    }

    /// Creates the root task representing the kernel shell / boot thread.
    pub fn root(name: &'static str) -> Self {
        Task {
            id: 0,
            name,
            rsp: 0,
            stack: None, // Uses bootloader stack
            state: TaskState::Running,
            ticks: 0,
        }
    }
}

/// Assembly routine for low-level context switching between two task stacks.
#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(old_rsp: *mut usize, new_rsp: usize) {
    core::arch::naked_asm!(
        // Save callee-saved registers of current task
        "pushfq",
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // Save current stack pointer in *old_rsp (RDI)
        "mov [rdi], rsp",

        // Switch to target stack pointer (RSI)
        "mov rsp, rsi",

        // Restore callee-saved registers from target stack
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "popfq",

        // Return into target task instruction pointer
        "ret",
    );
}

pub struct Scheduler {
    pub tasks: Vec<Task>,
    pub current: usize,
}

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            tasks: Vec::new(),
            current: 0,
        }
    }

    pub fn pick_next(&self) -> Option<usize> {
        if self.tasks.is_empty() {
            return None;
        }

        let total = self.tasks.len();
        for i in 1..=total {
            let idx = (self.current + i) % total;
            if self.tasks[idx].state == TaskState::Ready {
                return Some(idx);
            }
        }

        // If current is still ready/running, keep running it
        if self.tasks[self.current].state == TaskState::Running
            || self.tasks[self.current].state == TaskState::Ready
        {
            Some(self.current)
        } else {
            None
        }
    }
}

pub static SCHEDULER: Spinlock<Scheduler> = Spinlock::new(Scheduler::new());
static NEXT_TASK_ID: AtomicUsize = AtomicUsize::new(1);
pub static SENTINEL_HEARTBEATS: AtomicUsize = AtomicUsize::new(0);

/// Sentinel background task: periodically sends serial debug heartbeats
pub extern "C" fn sentinel_task_entry() {
    loop {
        let count = SENTINEL_HEARTBEATS.fetch_add(1, Ordering::SeqCst) + 1;
        crate::serial_println!("[AuraOS Sentinel] Heartbeat #{} - Kernel healthy", count);

        // Yield CPU back to other tasks
        yield_now();
    }
}

/// Initializes multitasking and registers the root shell task and sentinel task.
pub fn init() {
    let mut sched = SCHEDULER.lock();
    // Task 0: Root shell thread
    sched.tasks.push(Task::root("kernel-shell"));

    // Task 1: Sentinel background worker
    let tid = NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst);
    sched.tasks.push(Task::new(tid, "sentinel-worker", sentinel_task_entry));

    crate::serial_println!("[Multitasking] Scheduler initialized with {} tasks", sched.tasks.len());
}

/// Spawns a new kernel thread.
#[allow(dead_code)]
pub fn spawn(name: &'static str, entry_point: extern "C" fn()) -> usize {
    let tid = NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst);
    let task = Task::new(tid, name, entry_point);
    let mut sched = SCHEDULER.lock();
    sched.tasks.push(task);
    tid
}

/// Cooperatively yields execution to the next ready task.
pub fn yield_now() {
    let (old_rsp_ptr, new_rsp) = {
        let mut sched = SCHEDULER.lock();
        if sched.tasks.len() < 2 {
            return;
        }

        let curr_idx = sched.current;
        sched.tasks[curr_idx].ticks += 1;

        if let Some(next_idx) = sched.pick_next() {
            if next_idx == curr_idx {
                return;
            }

            if sched.tasks[curr_idx].state == TaskState::Running {
                sched.tasks[curr_idx].state = TaskState::Ready;
            }
            sched.tasks[next_idx].state = TaskState::Running;
            sched.current = next_idx;

            let old_ptr = &mut sched.tasks[curr_idx].rsp as *mut usize;
            let new_rsp = sched.tasks[next_idx].rsp;
            (old_ptr, new_rsp)
        } else {
            return;
        }
    };

    unsafe {
        switch_context(old_rsp_ptr, new_rsp);
    }
}

/// Called on every hardware timer PIT tick (IRQ0) to update CPU accounting.
pub fn timer_tick() {
    let mut sched = SCHEDULER.lock();
    if !sched.tasks.is_empty() {
        let curr = sched.current;
        sched.tasks[curr].ticks += 1;
    }
}

