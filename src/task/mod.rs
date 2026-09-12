//! ============================================================================
//! Kernel Multitasking & Task Context Switching (TCB & Scheduler)
//! ============================================================================
//!
//! Implements pure Rust cooperative multitasking with Task Control Blocks (TCB),
//! stack allocation, round-robin scheduling, and assembly context switching.
//!
//! In x86_64 Long Mode System V ABI:
//! - Callee-saved registers: r15, r14, r13, r12, rbx, rbp, rflags
//! - Context switch saves caller registers on current stack and restores target stack.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::sync::Spinlock;

pub mod ipc;

const STACK_SIZE: usize = 16 * 1024; // 16 KiB per kernel thread stack

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Sleeping(u64), // Wake target tick
    Dead,
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Ready => "READY",
            TaskState::Running => "RUNNING",
            TaskState::Sleeping(_) => "SLEEPING",
            TaskState::Dead => "DEAD",
        }
    }
}

/// Default quantum: 10 ticks at 100Hz = 100ms time slice
pub const DEFAULT_QUANTUM: u32 = 10;

/// Task Control Block (TCB)
#[allow(dead_code)]
pub struct Task {
    pub id: usize,
    pub name: &'static str,
    pub rsp: usize,
    pub stack: Option<Box<[u8]>>,
    pub state: TaskState,
    pub ticks: u64,
    /// Remaining quantum ticks before preemption
    pub quantum_remaining: u32,
    /// Base quantum allocation (reset value)
    pub quantum: u32,
    /// Task priority (0=highest, 255=lowest)
    pub priority: u8,
    /// True if running in Ring 3 (User space)
    pub is_user: bool,
    /// Per-process page table base (CR3) if isolated
    pub cr3: Option<u64>,
    /// Kernel stack top used for Ring 3 -> Ring 0 transitions (TSS.rsp0)
    pub kernel_stack_top: u64,
    /// User code entry point (RIP)
    pub user_entry: u64,
    /// User stack pointer top (RSP)
    pub user_stack_top: u64,
}

/// Trampoline executed when a user-space task is scheduled for the first time.
extern "C" fn user_task_trampoline() {
    let (entry, stack_top) = {
        let sched = SCHEDULER.lock();
        let curr = sched.current;
        (sched.tasks[curr].user_entry, sched.tasks[curr].user_stack_top)
    };
    unsafe {
        crate::arch::ring3::enter_user_mode(entry, stack_top);
    }
}

impl Task {
    /// Creates a new kernel task with its own dedicated 16 KiB stack.
    pub fn new(id: usize, name: &'static str, entry_point: extern "C" fn()) -> Self {
        let stack = vec![0u8; STACK_SIZE].into_boxed_slice();
        let stack_top = stack.as_ptr() as usize + STACK_SIZE;

        // Ensure 16-byte alignment of the initial frame (72 bytes total frame)
        let aligned_top = (stack_top & !0xF) - 8;

        let frame_ptr = (aligned_top - 72) as *mut u64;
        unsafe {
            *frame_ptr.add(8) = 0;                                               // ABI alignment padding
            *frame_ptr.add(7) = entry_point as *const () as usize as u64;     // RIP
            *frame_ptr.add(6) = 0x202;                       // RFLAGS (IF=1)
            *frame_ptr.add(5) = 0;                           // RBP
            *frame_ptr.add(4) = 0;                           // RBX
            *frame_ptr.add(3) = 0;                           // R12
            *frame_ptr.add(2) = 0;                           // R13
            *frame_ptr.add(1) = 0;                           // R14
            *frame_ptr.add(0) = 0;                           // R15
        }

        let initial_rsp = (aligned_top - 72) as usize;

        Task {
            id,
            name,
            rsp: initial_rsp,
            stack: Some(stack),
            state: TaskState::Ready,
            ticks: 0,
            quantum_remaining: DEFAULT_QUANTUM,
            quantum: DEFAULT_QUANTUM,
            priority: 128, // Default middle priority
            is_user: false,
            cr3: None,
            kernel_stack_top: (stack_top & !0xF) as u64,
            user_entry: 0,
            user_stack_top: 0,
        }
    }

    /// Creates a user space task that will transition to Ring 3 upon scheduling.
    pub fn new_user(
        id: usize,
        name: &'static str,
        entry_point: u64,
        user_stack_top: u64,
        cr3: u64,
    ) -> Self {
        let kstack = vec![0u8; STACK_SIZE].into_boxed_slice();
        let kstack_top = kstack.as_ptr() as usize + STACK_SIZE;
        let aligned_top = (kstack_top & !0xF) - 8;

        let frame_ptr = (aligned_top - 72) as *mut u64;
        unsafe {
            *frame_ptr.add(8) = 0;                                                   // ABI alignment padding
            *frame_ptr.add(7) = user_task_trampoline as *const () as usize as u64;   // RIP -> trampoline
            *frame_ptr.add(6) = 0x202;                                   // RFLAGS (IF=1)
            *frame_ptr.add(5) = 0;
            *frame_ptr.add(4) = 0;
            *frame_ptr.add(3) = 0;
            *frame_ptr.add(2) = 0;
            *frame_ptr.add(1) = 0;
            *frame_ptr.add(0) = 0;
        }

        let initial_rsp = (aligned_top - 72) as usize;

        Task {
            id,
            name,
            rsp: initial_rsp,
            stack: Some(kstack),
            state: TaskState::Ready,
            ticks: 0,
            quantum_remaining: DEFAULT_QUANTUM,
            quantum: DEFAULT_QUANTUM,
            priority: 100, // Slightly higher priority than default worker
            is_user: true,
            cr3: Some(cr3),
            kernel_stack_top: (kstack_top & !0xF) as u64,
            user_entry: entry_point,
            user_stack_top,
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
            quantum_remaining: DEFAULT_QUANTUM,
            quantum: DEFAULT_QUANTUM,
            priority: 0, // Highest priority for shell
            is_user: false,
            cr3: None,
            kernel_stack_top: 0,
            user_entry: 0,
            user_stack_top: 0,
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

        // Sleep for ~2 seconds (approx. 36 PIT ticks)
        sleep_ms(2000);
    }
}

/// Demo background worker task: runs for 5 steps with intervals, then terminates cleanly.
pub extern "C" fn demo_worker_entry() {
    let my_id = {
        let sched = SCHEDULER.lock();
        sched.tasks[sched.current].id
    };
    crate::serial_println!("[Worker #{}] Started background execution.", my_id);
    crate::println!("[Worker #{}] Started background execution.", my_id);
    for i in 1..=5 {
        crate::serial_println!("[Worker #{}] Iteration {}/5 — performing background work...", my_id, i);
        sleep_ms(600);
    }
    crate::serial_println!("[Worker #{}] Completed all work. Terminating cleanly.", my_id);
    crate::println!("[Worker #{}] Completed all work. Terminating cleanly.", my_id);
    {
        let mut sched = SCHEDULER.lock();
        let curr = sched.current;
        sched.tasks[curr].state = TaskState::Dead;
    }
    loop {
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

/// Updates hardware TSS.rsp0 and CR3 during a context switch.
#[inline]
pub fn on_context_switch(is_user: bool, kstack_top: u64, cr3_opt: Option<u64>) {
    if is_user {
        crate::arch::gdt::set_tss_rsp0(kstack_top);
        crate::arch::syscall::set_kernel_rsp(kstack_top);
        if let Some(cr3) = cr3_opt {
            unsafe {
                crate::memory::paging::write_cr3(crate::memory::paging::PhysAddr(cr3));
            }
        }
    } else {
        let kcr3 = crate::memory::user_space::KERNEL_CR3.load(core::sync::atomic::Ordering::Relaxed);
        if kcr3 != 0 {
            unsafe {
                crate::memory::paging::write_cr3(crate::memory::paging::PhysAddr(kcr3));
            }
        }
    }
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

/// Spawns a new user space task with its own address space and entry point.
#[allow(dead_code)]
pub fn spawn_user(
    name: &'static str,
    entry_point: u64,
    user_stack_top: u64,
    cr3: u64,
) -> usize {
    let tid = NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst);
    let task = Task::new_user(tid, name, entry_point, user_stack_top, cr3);
    let mut sched = SCHEDULER.lock();
    sched.tasks.push(task);
    tid
}

/// Terminates a task by setting its state to Dead. Task 0 (kernel shell) cannot be killed.
pub fn kill_task(id: usize) -> bool {
    if id == 0 {
        return false;
    }
    let killed = {
        let mut sched = SCHEDULER.lock();
        let mut found = false;
        for task in sched.tasks.iter_mut() {
            if task.id == id && task.state != TaskState::Dead {
                task.state = TaskState::Dead;
                found = true;
                break;
            }
        }
        found
    };
    if killed {
        reap_dead_tasks();
    }
    killed
}

/// Reaps dead tasks by removing them from the scheduler and freeing their stacks.
/// Task 0 (kernel shell) and the currently running task are never reaped.
/// Returns the number of tasks reaped.
#[allow(dead_code)]
pub fn reap_dead_tasks() -> usize {
    let mut sched = SCHEDULER.lock();
    let curr = sched.current;
    let mut reaped = 0;
    let mut i = sched.tasks.len();

    // Iterate backwards to avoid index invalidation issues
    while i > 0 {
        i -= 1;
        if i == 0 || i == curr {
            continue; // Never reap task 0 or the currently running task
        }
        if sched.tasks[i].state == TaskState::Dead {
            // Drop the task (its Box<[u8]> stack will be freed)
            sched.tasks.remove(i);
            reaped += 1;
            // Adjust current index if it was after the removed element
            if curr > i {
                sched.current -= 1;
            }
        }
    }

    if reaped > 0 {
        crate::serial_println!("[Scheduler] Reaped {} dead task(s), {} remaining", reaped, sched.tasks.len());
    }
    reaped
}

/// Cooperatively yields execution to the next ready task.
pub fn yield_now() {
    let interrupts_were_enabled = {
        let rflags: u64;
        unsafe {
            core::arch::asm!("nop", "pushfq", "pop {}", "cli", out(reg) rflags);
        }
        (rflags & (1 << 9)) != 0
    };

    let result = {
        let mut sched = SCHEDULER.lock();
        if sched.tasks.len() < 2 {
            None
        } else {
            let curr_idx = sched.current;

            if let Some(next_idx) = sched.pick_next() {
                if next_idx == curr_idx {
                    None
                } else {
                    if sched.tasks[curr_idx].state == TaskState::Running {
                        sched.tasks[curr_idx].state = TaskState::Ready;
                    }
                    sched.tasks[next_idx].state = TaskState::Running;
                    sched.current = next_idx;

                    let old_ptr = &mut sched.tasks[curr_idx].rsp as *mut usize;
                    let new_rsp = sched.tasks[next_idx].rsp;
                    let is_user = sched.tasks[next_idx].is_user;
                    let kstack = sched.tasks[next_idx].kernel_stack_top;
                    let cr3 = sched.tasks[next_idx].cr3;
                    Some((old_ptr, new_rsp, is_user, kstack, cr3))
                }
            } else {
                None
            }
        }
    };

    if let Some((old_rsp_ptr, new_rsp, is_user, kstack, cr3)) = result {
        on_context_switch(is_user, kstack, cr3);
        unsafe {
            switch_context(old_rsp_ptr, new_rsp);
        }
    }

    // Restore interrupts after returning from context switch
    if interrupts_were_enabled {
        unsafe {
            core::arch::asm!("sti", options(nomem, nostack));
        }
    }
}

/// Puts the calling task to sleep for a specified number of timer ticks (PIT IRQ 0).
pub fn sleep_ticks(ticks: u64) {
    if ticks == 0 {
        yield_now();
        return;
    }

    let wake_tick = crate::arch::idt::ticks().saturating_add(ticks);

    {
        let mut sched = SCHEDULER.lock();
        let curr = sched.current;
        sched.tasks[curr].state = TaskState::Sleeping(wake_tick);
    }

    // Loop until we are woken up (timer_tick will set state back to Ready)
    loop {
        yield_now();
        let is_woken = {
            let sched = SCHEDULER.lock();
            let curr = sched.current;
            sched.tasks[curr].state != TaskState::Sleeping(wake_tick)
                || crate::arch::idt::ticks() >= wake_tick
        };
        if is_woken {
            let mut sched = SCHEDULER.lock();
            let curr = sched.current;
            sched.tasks[curr].state = TaskState::Running;
            break;
        }
        // If still sleeping and no other task is ready, sleep CPU until next interrupt
        unsafe { core::arch::asm!("sti; hlt", options(nomem, nostack)); }
    }
}

/// Puts the calling task to sleep for approximately the requested number of milliseconds.
pub fn sleep_ms(ms: u64) {
    // PIT tick rate is 100 Hz (10 ms per tick)
    let ticks = if ms == 0 {
        0
    } else {
        ((ms + 9) / 10).max(1)
    };
    sleep_ticks(ticks);
}

/// Called on every hardware timer PIT tick (IRQ0) to update CPU accounting,
/// wake sleeping tasks, and trigger preemption when quantum expires.
pub fn timer_tick() {
    let current_ticks = crate::arch::idt::ticks();
    let mut sched = SCHEDULER.lock();
    if !sched.tasks.is_empty() {
        let curr = sched.current;
        sched.tasks[curr].ticks += 1;

        // Wake up any task whose sleep target tick has arrived
        for task in sched.tasks.iter_mut() {
            if let TaskState::Sleeping(wake_tick) = task.state {
                if current_ticks >= wake_tick {
                    task.state = TaskState::Ready;
                }
            }
        }

        // Preemptive quantum accounting
        if sched.tasks[curr].quantum_remaining > 0 {
            sched.tasks[curr].quantum_remaining -= 1;
        }
    }
}

/// Called from the timer IRQ handler after timer_tick().
/// If the current task's quantum has expired, forces a context switch.
/// Returns true if a preemption occurred.
pub fn preempt_schedule() -> bool {
    let result = {
        let mut sched = SCHEDULER.lock();
        if sched.tasks.is_empty() || sched.tasks.len() < 2 {
            return false;
        }

        let curr_idx = sched.current;

        // Only preempt if quantum is exhausted
        if sched.tasks[curr_idx].quantum_remaining > 0 {
            return false;
        }

        // Reset quantum for current task
        let q = sched.tasks[curr_idx].quantum;
        sched.tasks[curr_idx].quantum_remaining = q;

        if let Some(next_idx) = sched.pick_next() {
            if next_idx == curr_idx {
                return false;
            }

            if sched.tasks[curr_idx].state == TaskState::Running {
                sched.tasks[curr_idx].state = TaskState::Ready;
            }
            sched.tasks[next_idx].state = TaskState::Running;
            // Reset next task's quantum too
            let nq = sched.tasks[next_idx].quantum;
            sched.tasks[next_idx].quantum_remaining = nq;
            sched.current = next_idx;

            let old_ptr = &mut sched.tasks[curr_idx].rsp as *mut usize;
            let new_rsp = sched.tasks[next_idx].rsp;
            let is_user = sched.tasks[next_idx].is_user;
            let kstack = sched.tasks[next_idx].kernel_stack_top;
            let cr3 = sched.tasks[next_idx].cr3;
            Some((old_ptr, new_rsp, is_user, kstack, cr3))
        } else {
            None
        }
    };

    if let Some((old_rsp_ptr, new_rsp, is_user, kstack, cr3)) = result {
        on_context_switch(is_user, kstack, cr3);
        unsafe {
            switch_context(old_rsp_ptr, new_rsp);
        }
        true
    } else {
        false
    }
}
