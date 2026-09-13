//! ============================================================================
//! Kernel Multitasking & Task Context Switching (TCB & SMP Scheduler)
//! ============================================================================
//!
//! Implements pure Rust cooperative and preemptive multitasking with:
//! - Per-CPU independent run-queues and scheduler state (`CpuScheduler`).
//! - Task Control Blocks (TCB) and kernel/user stack allocation.
//! - Round-robin scheduling with dynamic quantum accounting.
//! - Assembly context switching (`switch_context`).
//! - Multi-core load-balanced task distribution and work stealing.
//! - Per-CPU Local APIC timer preemption.

use crate::sync::Spinlock;
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

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
    /// Owned per-process address space (freed automatically when task is reaped)
    pub address_space: Option<crate::memory::user_space::AddressSpace>,
}

/// Trampoline executed when a user-space task is scheduled for the first time.
extern "C" fn user_task_trampoline() {
    let cpu_id = crate::arch::smp::current_cpu();
    let cpu_id = if cpu_id < crate::arch::smp::MAX_CPUS { cpu_id } else { 0 };
    let (entry, stack_top) = {
        let sched = CPU_SCHEDULERS[cpu_id].lock();
        let curr = sched.current;
        (
            sched.tasks[curr].user_entry,
            sched.tasks[curr].user_stack_top,
        )
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
            *frame_ptr.add(8) = 0; // ABI alignment padding
            *frame_ptr.add(7) = entry_point as *const () as usize as u64; // RIP
            *frame_ptr.add(6) = 0x202; // RFLAGS (IF=1)
            *frame_ptr.add(5) = 0; // RBP
            *frame_ptr.add(4) = 0; // RBX
            *frame_ptr.add(3) = 0; // R12
            *frame_ptr.add(2) = 0; // R13
            *frame_ptr.add(1) = 0; // R14
            *frame_ptr.add(0) = 0; // R15
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
            priority: 10,
            is_user: false,
            cr3: None,
            kernel_stack_top: 0,
            user_entry: 0,
            user_stack_top: 0,
            address_space: None,
        }
    }

    /// Creates a new user-space (Ring 3) task.
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
            *frame_ptr.add(8) = 0; // ABI alignment padding
            *frame_ptr.add(7) = user_task_trampoline as *const () as usize as u64; // RIP -> trampoline
            *frame_ptr.add(6) = 0x202; // RFLAGS (IF=1)
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
            priority: 10,
            is_user: true,
            cr3: Some(cr3),
            kernel_stack_top: kstack_top as u64,
            user_entry: entry_point,
            user_stack_top,
            address_space: None,
        }
    }

    /// Creates a new user-space (Ring 3) task with an owned per-process AddressSpace.
    pub fn new_user_with_space(
        id: usize,
        name: &'static str,
        entry_point: u64,
        user_stack_top: u64,
        space: crate::memory::user_space::AddressSpace,
    ) -> Self {
        let cr3 = space.pml4_phys().as_u64();
        let mut task = Self::new_user(id, name, entry_point, user_stack_top, cr3);
        task.address_space = Some(space);
        task
    }

    /// Creates the root task representing the kernel shell / boot thread on the BSP.
    pub fn root(name: &'static str) -> Self {
        Self::root_for_cpu(0, name)
    }

    /// Creates a root/idle task for a specific CPU core.
    pub fn root_for_cpu(cpu_id: usize, name: &'static str) -> Self {
        Task {
            id: if cpu_id == 0 { 0 } else { 1000 + cpu_id },
            name,
            rsp: 0,
            stack: None, // Uses the CPU's own boot/kernel stack
            state: TaskState::Running,
            ticks: 0,
            quantum_remaining: DEFAULT_QUANTUM,
            quantum: DEFAULT_QUANTUM,
            priority: 0, // Highest priority
            is_user: false,
            cr3: None,
            kernel_stack_top: 0,
            user_entry: 0,
            user_stack_top: 0,
            address_space: None,
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

/// Independent run-queue and scheduler state for a single CPU core.
pub struct CpuScheduler {
    #[allow(dead_code)]
    pub cpu_id: usize,
    pub tasks: Vec<Task>,
    pub current: usize,
    pub ticks: u64,
}

#[allow(dead_code)]
pub type Scheduler = CpuScheduler;

impl CpuScheduler {
    pub const fn new(cpu_id: usize) -> Self {
        CpuScheduler {
            cpu_id,
            tasks: Vec::new(),
            current: 0,
            ticks: 0,
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

/// Per-CPU array of independent schedulers (1 per core up to MAX_CPUS = 8).
pub static CPU_SCHEDULERS: [Spinlock<CpuScheduler>; crate::arch::smp::MAX_CPUS] = [
    Spinlock::new(CpuScheduler::new(0)),
    Spinlock::new(CpuScheduler::new(1)),
    Spinlock::new(CpuScheduler::new(2)),
    Spinlock::new(CpuScheduler::new(3)),
    Spinlock::new(CpuScheduler::new(4)),
    Spinlock::new(CpuScheduler::new(5)),
    Spinlock::new(CpuScheduler::new(6)),
    Spinlock::new(CpuScheduler::new(7)),
];

/// Transparent compatibility wrapper for existing `SCHEDULER.lock()` callers.
/// Directs calls to `CPU_SCHEDULERS[0]` (BSP scheduler).
pub struct SchedulerCompat;

impl core::ops::Deref for SchedulerCompat {
    type Target = Spinlock<CpuScheduler>;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &CPU_SCHEDULERS[0]
    }
}

pub static SCHEDULER: SchedulerCompat = SchedulerCompat;

static NEXT_TASK_ID: AtomicUsize = AtomicUsize::new(1);
pub static SENTINEL_HEARTBEATS: AtomicUsize = AtomicUsize::new(0);

/// Sentinel background task: periodically sends serial debug heartbeats
pub extern "C" fn sentinel_task_entry() {
    loop {
        let count = SENTINEL_HEARTBEATS.fetch_add(1, Ordering::SeqCst) + 1;
        crate::serial_println!("[AuraOS Sentinel] Heartbeat #{} - Kernel healthy", count);

        // Sleep for ~2 seconds (approx. 200 PIT ticks at 100Hz)
        sleep_ms(2000);
    }
}

/// Demo background worker task: runs for 5 steps with intervals, then terminates cleanly.
pub extern "C" fn demo_worker_entry() {
    let cpu_id = crate::arch::smp::current_cpu();
    let cpu_id = if cpu_id < crate::arch::smp::MAX_CPUS { cpu_id } else { 0 };
    let my_id = {
        let sched = CPU_SCHEDULERS[cpu_id].lock();
        sched.tasks[sched.current].id
    };
    crate::serial_println!("[Worker #{}] Started background execution on CPU #{}.", my_id, cpu_id);
    crate::println!("[Worker #{}] Started background execution on CPU #{}.", my_id, cpu_id);
    for i in 1..=5 {
        crate::serial_println!(
            "[Worker #{}] Iteration {}/5 on CPU #{} — performing work...",
            my_id,
            i,
            crate::arch::smp::current_cpu()
        );
        sleep_ms(600);
    }
    let fin_cpu = crate::arch::smp::current_cpu();
    crate::serial_println!(
        "[Worker #{}] Completed all work on CPU #{}. Terminating cleanly.",
        my_id,
        fin_cpu
    );
    crate::println!(
        "[Worker #{}] Completed all work on CPU #{}. Terminating cleanly.",
        my_id,
        fin_cpu
    );
    {
        let cpu = if fin_cpu < crate::arch::smp::MAX_CPUS { fin_cpu } else { 0 };
        let mut sched = CPU_SCHEDULERS[cpu].lock();
        let curr = sched.current;
        sched.tasks[curr].state = TaskState::Dead;
    }
    yield_now();
}

/// Initializes multitasking on the BSP and registers the root shell task and sentinel task.
pub fn init() {
    let mut sched = CPU_SCHEDULERS[0].lock();
    // Task 0: Root shell thread on BSP
    sched.tasks.push(Task::root("kernel-shell"));

    // Task 1: Sentinel background worker
    let tid = NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst);
    sched
        .tasks
        .push(Task::new(tid, "sentinel-worker", sentinel_task_entry));

    crate::serial_println!(
        "[Multitasking] BSP Scheduler initialized with {} tasks",
        sched.tasks.len()
    );
}

/// Initializes the scheduler on an Application Processor (AP).
/// Called on the AP before entering its idle scheduling loop.
pub fn init_ap(cpu_id: usize) {
    if cpu_id >= crate::arch::smp::MAX_CPUS {
        return;
    }
    let mut sched = CPU_SCHEDULERS[cpu_id].lock();
    if sched.tasks.is_empty() {
        let name = match cpu_id {
            1 => "idle-cpu-1",
            2 => "idle-cpu-2",
            3 => "idle-cpu-3",
            4 => "idle-cpu-4",
            5 => "idle-cpu-5",
            6 => "idle-cpu-6",
            7 => "idle-cpu-7",
            _ => "idle-cpu-ap",
        };
        sched.tasks.push(Task::root_for_cpu(cpu_id, name));
        sched.current = 0;
    }
    crate::serial_println!("[SMP] CPU #{} scheduler initialized (idle task ready)", cpu_id);
}

/// Main idle scheduling loop for an Application Processor.
/// The AP runs this function continuously, switching to ready tasks
/// or waiting for work in low-power HLT mode.
pub fn ap_idle_loop(cpu_id: usize) -> ! {
    crate::serial_println!("[SMP] CPU #{} entered scheduler loop", cpu_id);
    loop {
        let (has_ready, count) = {
            let sched = CPU_SCHEDULERS[cpu_id].lock();
            let has = sched.tasks.iter().enumerate().any(|(i, t)| {
                i != sched.current && t.state == TaskState::Ready
            });
            (has, sched.tasks.len())
        };

        if has_ready {
            yield_now();
        } else if count > 1 {
            try_steal_task(cpu_id);
            unsafe {
                core::arch::asm!("sti; hlt", options(nomem, nostack));
            }
        } else {
            // Only idle task on this core: sleep until interrupted by timer or IPI
            unsafe {
                core::arch::asm!("sti; hlt", options(nomem, nostack));
            }
        }
    }
}

/// Updates hardware TSS.rsp0, syscall scratch, and CR3 during a context switch for the current CPU.
#[inline]
pub fn on_context_switch(is_user: bool, kstack_top: u64, cr3_opt: Option<u64>) {
    let cpu_id = crate::arch::smp::current_cpu();
    let cpu_id = if cpu_id < crate::arch::smp::MAX_CPUS { cpu_id } else { 0 };
    if is_user {
        crate::arch::gdt::set_tss_rsp0(kstack_top);
        crate::arch::syscall::set_kernel_rsp(kstack_top);
        unsafe {
            let per_cpu = &mut (*crate::arch::smp::PER_CPU.get())[cpu_id];
            per_cpu.tss.rsp0 = kstack_top;
            per_cpu.kernel_rsp_scratch = kstack_top;
        }
        if let Some(cr3) = cr3_opt {
            unsafe {
                crate::memory::paging::write_cr3(crate::memory::paging::PhysAddr(cr3));
            }
        }
    } else {
        let kcr3 =
            crate::memory::user_space::KERNEL_CR3.load(core::sync::atomic::Ordering::Relaxed);
        if kcr3 != 0 {
            unsafe {
                crate::memory::paging::write_cr3(crate::memory::paging::PhysAddr(kcr3));
            }
        }
    }
}

/// Selects the best CPU core to assign a new task to (least loaded online CPU).
pub fn choose_target_cpu() -> usize {
    let online = crate::arch::smp::cpu_count();
    if online <= 1 {
        return 0;
    }
    let num_cpus = online.min(crate::arch::smp::MAX_CPUS);
    let mut best_cpu = 0;
    let mut min_load = usize::MAX;

    for cpu in 0..num_cpus {
        let sched = CPU_SCHEDULERS[cpu].lock();
        let load = sched.tasks.len();
        if load < min_load {
            min_load = load;
            best_cpu = cpu;
        }
    }
    best_cpu
}

/// Spawns a new kernel thread and attaches it to the least-loaded CPU's run queue.
pub fn spawn(name: &'static str, entry_point: extern "C" fn()) -> usize {
    let tid = NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst);
    let task = Task::new(tid, name, entry_point);
    let target_cpu = choose_target_cpu();
    let mut sched = CPU_SCHEDULERS[target_cpu].lock();
    sched.tasks.push(task);
    crate::serial_println!("[Multitasking] Spawned task '{}' (TID {}) on CPU #{}", name, tid, target_cpu);
    tid
}

/// Spawns a new user space task with its own address space and entry point.
#[allow(dead_code)]
pub fn spawn_user(name: &'static str, entry_point: u64, user_stack_top: u64, cr3: u64) -> usize {
    let tid = NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst);
    let task = Task::new_user(tid, name, entry_point, user_stack_top, cr3);
    let target_cpu = choose_target_cpu();
    let mut sched = CPU_SCHEDULERS[target_cpu].lock();
    sched.tasks.push(task);
    crate::serial_println!("[Multitasking] Spawned user task '{}' (TID {}) on CPU #{}", name, tid, target_cpu);
    tid
}

/// Spawns a new user space task with an owned AddressSpace.
pub fn spawn_user_with_space(
    name: &'static str,
    entry_point: u64,
    user_stack_top: u64,
    space: crate::memory::user_space::AddressSpace,
) -> usize {
    let tid = NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst);
    let task = Task::new_user_with_space(tid, name, entry_point, user_stack_top, space);
    let target_cpu = choose_target_cpu();
    let mut sched = CPU_SCHEDULERS[target_cpu].lock();
    sched.tasks.push(task);
    crate::serial_println!(
        "[Multitasking] Spawned user task '{}' (TID {}) with owned AddressSpace on CPU #{}",
        name,
        tid,
        target_cpu
    );
    tid
}

/// Demand paging fault handler called by the Page Fault (#PF) ISR.
/// Resolves unmapped pages for the currently running task if an AddressSpace is present.
pub fn handle_current_page_fault(fault_addr: u64, is_write: bool) -> bool {
    let cpu_id = crate::arch::smp::current_cpu();
    let cpu_id = if cpu_id < crate::arch::smp::MAX_CPUS { cpu_id } else { 0 };

    let mut sched = CPU_SCHEDULERS[cpu_id].lock();
    let curr = sched.current;
    if curr < sched.tasks.len() {
        if let Some(ref mut space) = sched.tasks[curr].address_space {
            return space.handle_demand_fault(fault_addr, is_write);
        }
    }
    false
}

/// Checks whether the fault address falls in a guard page of the current task.
pub fn is_current_guard_page(fault_addr: u64) -> bool {
    let cpu_id = crate::arch::smp::current_cpu();
    let cpu_id = if cpu_id < crate::arch::smp::MAX_CPUS { cpu_id } else { 0 };

    let sched = CPU_SCHEDULERS[cpu_id].lock();
    let curr = sched.current;
    if curr < sched.tasks.len() {
        if let Some(ref space) = sched.tasks[curr].address_space {
            return space.is_guard_page(fault_addr);
        }
    }
    false
}

/// Invokes heap break adjustment (brk) for the currently running task on the active CPU core.
#[allow(dead_code)]
pub fn current_task_brk(new_brk: u64) -> u64 {
    let cpu_id = crate::arch::smp::current_cpu();
    let cpu_id = if cpu_id < crate::arch::smp::MAX_CPUS { cpu_id } else { 0 };

    let mut sched = CPU_SCHEDULERS[cpu_id].lock();
    let curr = sched.current;
    if curr < sched.tasks.len() {
        if let Some(ref mut space) = sched.tasks[curr].address_space {
            return space.brk(new_brk);
        }
    }
    u64::MAX
}


/// Attempts to steal a ready task from another CPU.
/// To avoid deadlocks, locks are acquired one at a time and never held nested.
pub fn try_steal_task(my_cpu: usize) -> bool {
    let online = crate::arch::smp::cpu_count();
    if online <= 1 {
        return false;
    }
    let num_cpus = online.min(crate::arch::smp::MAX_CPUS);

    for offset in 1..num_cpus {
        let victim_cpu = (my_cpu + offset) % num_cpus;
        let mut stolen_task = None;

        // 1. Inspect victim queue with victim lock
        {
            let mut victim_sched = CPU_SCHEDULERS[victim_cpu].lock();
            if victim_sched.tasks.len() > 2 {
                let curr = victim_sched.current;
                let mut steal_idx = None;
                for (i, t) in victim_sched.tasks.iter().enumerate() {
                    // Do not steal root/idle task (idx 0), current running task, or user tasks
                    if i != 0 && i != curr && t.state == TaskState::Ready && !t.is_user {
                        steal_idx = Some(i);
                        break;
                    }
                }
                if let Some(idx) = steal_idx {
                    let task = victim_sched.tasks.remove(idx);
                    if victim_sched.current > idx {
                        victim_sched.current -= 1;
                    }
                    stolen_task = Some(task);
                }
            }
        } // Victim lock released here!

        // 2. Insert into local queue with local lock (NO nested locks)
        if let Some(mut task) = stolen_task {
            task.quantum_remaining = task.quantum;
            let mut my_sched = CPU_SCHEDULERS[my_cpu].lock();
            crate::serial_println!(
                "[SMP] Work stealing: CPU #{} stole task '{}' (TID {}) from CPU #{}",
                my_cpu,
                task.name,
                task.id,
                victim_cpu
            );
            my_sched.tasks.push(task);
            return true;
        }
    }
    false
}

/// Marks a task for termination by TID across any CPU run-queue and reaps it.
pub fn kill_task(tid: usize) -> bool {
    let online = crate::arch::smp::cpu_count();
    let num_cpus = online.max(1).min(crate::arch::smp::MAX_CPUS);
    let mut found_cpu = None;

    for cpu in 0..num_cpus {
        let mut sched = CPU_SCHEDULERS[cpu].lock();
        for task in sched.tasks.iter_mut() {
            if task.id == tid {
                task.state = TaskState::Dead;
                found_cpu = Some(cpu);
                break;
            }
        }
        if found_cpu.is_some() {
            break;
        }
    }

    if let Some(cpu) = found_cpu {
        reap_dead_tasks_on_cpu(cpu);
        true
    } else {
        false
    }
}

/// Reaps dead tasks across all CPU run queues.
#[allow(dead_code)]
pub fn reap_dead_tasks() -> usize {
    let online = crate::arch::smp::cpu_count();
    let num_cpus = online.max(1).min(crate::arch::smp::MAX_CPUS);
    let mut total_reaped = 0;

    for cpu in 0..num_cpus {
        total_reaped += reap_dead_tasks_on_cpu(cpu);
    }
    total_reaped
}

/// Reaps dead tasks on a specific CPU run queue.
pub fn reap_dead_tasks_on_cpu(cpu_id: usize) -> usize {
    if cpu_id >= crate::arch::smp::MAX_CPUS {
        return 0;
    }
    let mut sched = CPU_SCHEDULERS[cpu_id].lock();
    let mut reaped = 0;
    let mut i = 0;

    while i < sched.tasks.len() {
        if i == sched.current {
            i += 1;
            continue;
        }

        if sched.tasks[i].state == TaskState::Dead {
            let dead_task = sched.tasks.remove(i);
            reaped += 1;
            crate::serial_println!("[Multitasking] Reaped dead task '{}' (TID {}) on CPU #{}", dead_task.name, dead_task.id, cpu_id);
            if i < sched.current {
                sched.current -= 1;
            }
        } else {
            i += 1;
        }
    }

    if reaped > 0 {
        crate::klog!(
            Info,
            "scheduler",
            "CPU #{}: Reaped {} dead task(s), {} remaining",
            cpu_id,
            reaped,
            sched.tasks.len()
        );
    }
    reaped
}

/// Cooperatively yields the remaining CPU timeslice to the next ready task.
pub fn yield_now() {
    let cpu_id = crate::arch::smp::current_cpu();
    let cpu_id = if cpu_id < crate::arch::smp::MAX_CPUS { cpu_id } else { 0 };

    // Atomically check IF bit in RFLAGS and disable interrupts
    let interrupts_were_enabled = {
        let rflags: u64;
        unsafe {
            core::arch::asm!("nop", "pushfq", "pop {}", "cli", out(reg) rflags);
        }
        (rflags & (1 << 9)) != 0
    };

    // If only the current/idle task is ready on this CPU and we have tasks, attempt work stealing
    {
        let sched = CPU_SCHEDULERS[cpu_id].lock();
        let has_other_ready = sched.tasks.iter().enumerate().any(|(i, t)| {
            i != sched.current && t.state == TaskState::Ready
        });
        let should_steal = !has_other_ready && sched.tasks.len() > 1;
        drop(sched);
        if should_steal {
            try_steal_task(cpu_id);
        }
    }

    let result = {
        let mut sched = CPU_SCHEDULERS[cpu_id].lock();
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

/// Puts the calling task to sleep for a specified number of timer ticks.
pub fn sleep_ticks(ticks: u64) {
    if ticks == 0 {
        yield_now();
        return;
    }

    let cpu_id = crate::arch::smp::current_cpu();
    let cpu_id = if cpu_id < crate::arch::smp::MAX_CPUS { cpu_id } else { 0 };

    let current_ticks = crate::arch::idt::ticks();
    let target_tick = current_ticks + ticks;

    {
        let mut sched = CPU_SCHEDULERS[cpu_id].lock();
        let curr = sched.current;
        sched.tasks[curr].state = TaskState::Sleeping(target_tick);
    }

    loop {
        yield_now();

        let still_sleeping = {
            let sched = CPU_SCHEDULERS[cpu_id].lock();
            let curr = sched.current;
            matches!(sched.tasks[curr].state, TaskState::Sleeping(t) if crate::arch::idt::ticks() < t)
        };

        if !still_sleeping {
            break;
        }
        // If still sleeping and no other task is ready, sleep CPU until next interrupt
        unsafe {
            core::arch::asm!("sti; hlt", options(nomem, nostack));
        }
    }
}

/// Puts the calling task to sleep for approximately the requested number of milliseconds.
pub fn sleep_ms(ms: u64) {
    // PIT tick rate is 100 Hz (10 ms per tick)
    let ticks = if ms == 0 { 0 } else { ((ms + 9) / 10).max(1) };
    sleep_ticks(ticks);
}

/// Called on every hardware timer PIT tick (IRQ0 on BSP) to update global system accounting.
pub fn timer_tick() {
    let current_ticks = crate::arch::idt::ticks();
    let online = crate::arch::smp::cpu_count();
    let num_cpus = online.max(1).min(crate::arch::smp::MAX_CPUS);

    for cpu_id in 0..num_cpus {
        let mut sched = CPU_SCHEDULERS[cpu_id].lock();
        // Wake up any task on this CPU whose sleep target tick has arrived
        for task in sched.tasks.iter_mut() {
            if let TaskState::Sleeping(wake_tick) = task.state {
                if current_ticks >= wake_tick {
                    task.state = TaskState::Ready;
                }
            }
        }
    }

    // On CPU 0, also update its quantum and ticks
    let mut sched0 = CPU_SCHEDULERS[0].lock();
    if !sched0.tasks.is_empty() {
        let curr = sched0.current;
        if curr < sched0.tasks.len() {
            sched0.tasks[curr].ticks += 1;
            if sched0.tasks[curr].quantum_remaining > 0 {
                sched0.tasks[curr].quantum_remaining -= 1;
            }
        }
    }
}

/// Called from Local APIC timer interrupt handler on the CURRENT CPU core.
pub fn smp_timer_tick() {
    let cpu_id = crate::arch::smp::current_cpu();
    let cpu_id = if cpu_id < crate::arch::smp::MAX_CPUS { cpu_id } else { 0 };
    let current_ticks = crate::arch::idt::ticks();

    {
        let mut sched = CPU_SCHEDULERS[cpu_id].lock();
        sched.ticks += 1;
        if !sched.tasks.is_empty() {
            let curr = sched.current;
            if curr < sched.tasks.len() {
                sched.tasks[curr].ticks += 1;
                if sched.tasks[curr].quantum_remaining > 0 {
                    sched.tasks[curr].quantum_remaining -= 1;
                }
            }

            // Wake up any task whose sleep target tick has arrived
            for task in sched.tasks.iter_mut() {
                if let TaskState::Sleeping(wake_tick) = task.state {
                    if current_ticks >= wake_tick {
                        task.state = TaskState::Ready;
                    }
                }
            }
        }
    }

    // Periodically reap dead tasks on this CPU
    if current_ticks.is_multiple_of(25) {
        reap_dead_tasks_on_cpu(cpu_id);
    }

    // Attempt preemptive scheduling on this CPU
    preempt_schedule_on_cpu(cpu_id);
}

/// Preemptively context switches if the current task's quantum has expired on CPU 0.
pub fn preempt_schedule() -> bool {
    preempt_schedule_on_cpu(0)
}

/// Preemptive scheduler for a specific CPU core.
pub fn preempt_schedule_on_cpu(cpu_id: usize) -> bool {
    let result = {
        let mut sched = CPU_SCHEDULERS[cpu_id].lock();
        if sched.tasks.is_empty() || sched.tasks.len() < 2 {
            return false;
        }

        let curr_idx = sched.current;
        if curr_idx >= sched.tasks.len() {
            return false;
        }

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

/// Returns the total number of tasks currently allocated across all online CPUs.
pub fn total_task_count() -> usize {
    let online = crate::arch::smp::cpu_count();
    let num_cpus = online.clamp(1, crate::arch::smp::MAX_CPUS);
    let mut total = 0;
    for c in 0..num_cpus {
        total += CPU_SCHEDULERS[c].lock().tasks.len();
    }
    total
}
