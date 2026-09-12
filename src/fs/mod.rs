//! ============================================================================
//! AuraOS Virtual File System (VFS) & In-Memory RAM Disk (RAMFS)
//! ============================================================================
//!
//! Implements an Inode-based hierarchical Virtual File System in pure Rust.
//! Provides directory navigation, file creation, reading, writing, and deletion
//! without any external crates.

use alloc::string::String;
use alloc::vec::Vec;
use crate::sync::Spinlock;

use alloc::borrow::Cow;

pub use errors::VfsError;

pub mod errors;
pub mod fat32;
pub mod elf;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    File,
    Directory,
}

pub enum InodeKind {
    File { content: Vec<u8> },
    Directory { children: Vec<usize> },
    ProcFile { generator: fn() -> Vec<u8> },
    DevNull,
    DevZero,
    DevRandom,
    Tombstone,
}

#[allow(dead_code)]
pub struct Inode {
    pub id: usize,
    pub parent_id: usize,
    pub name: String,
    pub kind: InodeKind,
}

impl Inode {
    pub fn is_dir(&self) -> bool {
        matches!(self.kind, InodeKind::Directory { .. })
    }

    pub fn size(&self) -> usize {
        match &self.kind {
            InodeKind::File { content } => content.len(),
            InodeKind::Directory { children } => children.len(),
            InodeKind::ProcFile { .. } => 0,
            InodeKind::DevNull | InodeKind::DevZero | InodeKind::DevRandom | InodeKind::Tombstone => 0,
        }
    }
}

pub struct DirectoryEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: usize,
}

pub struct Vfs {
    pub inodes: Vec<Inode>,
    pub current_inode: usize,
    pub free_inodes: Vec<usize>,
}

impl Vfs {
    /// Creates a new VFS initialized with root directory `/`.
    pub const fn new() -> Self {
        Vfs {
            inodes: Vec::new(),
            current_inode: 0,
            free_inodes: Vec::new(),
        }
    }

    /// Allocates an inode ID, reusing a tombstone slot if available or allocating a new slot.
    fn allocate_inode(&mut self, parent_id: usize, name: &str, kind: InodeKind) -> usize {
        if let Some(reused_id) = self.free_inodes.pop() {
            self.inodes[reused_id] = Inode {
                id: reused_id,
                parent_id,
                name: String::from(name),
                kind,
            };
            reused_id
        } else {
            let new_id = self.inodes.len();
            self.inodes.push(Inode {
                id: new_id,
                parent_id,
                name: String::from(name),
                kind,
            });
            new_id
        }
    }

    /// Initializes the root filesystem and populates standard system files.
    pub fn init(&mut self) {
        self.inodes.clear();
        self.free_inodes.clear();

        // Inode 0: Root directory "/"
        self.inodes.push(Inode {
            id: 0,
            parent_id: 0,
            name: String::from("/"),
            kind: InodeKind::Directory { children: Vec::new() },
        });

        self.current_inode = 0;

        // Create standard system hierarchy: /etc, /docs, /bin, /proc, /dev
        let etc_id = self.mkdir_at(0, "etc").unwrap_or(0);
        let docs_id = self.mkdir_at(0, "docs").unwrap_or(0);
        let bin_id = self.mkdir_at(0, "bin").unwrap_or(0);
        let proc_id = self.mkdir_at(0, "proc").unwrap_or(0);
        let dev_id = self.mkdir_at(0, "dev").unwrap_or(0);

        // Populate /bin with genuine 64-bit ELF executables
        let hello_elf = elf::build_hello_elf();
        let _ = self.create_file_at(bin_id, "hello", &hello_elf);

        let counter_elf = elf::build_counter_elf();
        let _ = self.create_file_at(bin_id, "counter", &counter_elf);

        let init_elf = elf::build_init_elf();
        let _ = self.create_file_at(bin_id, "init", &init_elf);

        // Populate system configuration and documentation files
        let _ = self.create_file_at(etc_id, "hostname", b"auraos-baremetal");
        let _ = self.create_file_at(
            etc_id,
            "version",
            b"AuraOS v0.1.0-alpha (x86_64 Long Mode Pure Rust Kernel)",
        );
        let _ = self.create_file_at(
            etc_id,
            "motd",
            b"Welcome to AuraOS! Type 'help' for commands, 'ls' to inspect files.",
        );

        let manifesto = b"========================================\n\
                          AuraOS 10-Year Vision & Core Principles\n\
                          ========================================\n\
                          1. 100% Pure Rust bare-metal kernel.\n\
                          2. Ultra-lightweight footprint (< 1 MB core).\n\
                          3. Microkernel isolation with Ring 0/Ring 3.\n\
                          4. Zero legacy technical debt.\n";
        let _ = self.create_file_at(docs_id, "manifesto.txt", manifesto);

        // Populate /proc dynamic system metrics
        let _ = self.create_proc_file_at(proc_id, "uptime", proc_uptime_generator);
        let _ = self.create_proc_file_at(proc_id, "meminfo", proc_meminfo_generator);
        let _ = self.create_proc_file_at(proc_id, "cpuinfo", proc_cpuinfo_generator);
        let _ = self.create_proc_file_at(proc_id, "tasks", proc_tasks_generator);

        // Populate /dev virtual devices
        let _ = self.create_device_file_at(dev_id, "null", InodeKind::DevNull);
        let _ = self.create_device_file_at(dev_id, "zero", InodeKind::DevZero);
        let _ = self.create_device_file_at(dev_id, "random", InodeKind::DevRandom);
        let _ = self.create_device_file_at(dev_id, "urandom", InodeKind::DevRandom);
    }

    /// Resolves a path string starting from either root (if path starts with '/')
    /// or from the current working directory.
    pub fn resolve_path(&self, path: &str) -> Result<usize, VfsError> {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Ok(self.current_inode);
        }

        let (mut curr, path_to_parse) = if let Some(stripped) = trimmed.strip_prefix('/') {
            (0, stripped)
        } else {
            (self.current_inode, trimmed)
        };

        for segment in path_to_parse.split('/') {
            if segment.is_empty() || segment == "." {
                continue;
            }

            if segment == ".." {
                curr = self.inodes[curr].parent_id;
                continue;
            }

            // Look for matching child in current directory
            let next = match &self.inodes[curr].kind {
                InodeKind::Directory { children } => {
                    let mut found = None;
                    for &child_id in children {
                        if self.inodes[child_id].name == segment {
                            found = Some(child_id);
                            break;
                        }
                    }
                    found
                }
                _ => return Err(VfsError::NotADirectory),
            };

            match next {
                Some(id) => curr = id,
                None => return Err(VfsError::NotFound),
            }
        }

        Ok(curr)
    }

    /// Creates a new directory inside the specified parent directory Inode.
    pub fn mkdir_at(&mut self, parent_id: usize, name: &str) -> Result<usize, VfsError> {
        if !self.inodes[parent_id].is_dir() {
            return Err(VfsError::NotADirectory);
        }

        // Check for duplicates
        if let InodeKind::Directory { children } = &self.inodes[parent_id].kind {
            for &child_id in children {
                if self.inodes[child_id].name == name {
                    return Err(VfsError::AlreadyExists);
                }
            }
        }

        let new_id = self.allocate_inode(
            parent_id,
            name,
            InodeKind::Directory { children: Vec::new() },
        );

        if let InodeKind::Directory { children } = &mut self.inodes[parent_id].kind {
            children.push(new_id);
        }

        Ok(new_id)
    }

    /// Creates a new regular file with initial content inside the parent directory Inode.
    pub fn create_file_at(&mut self, parent_id: usize, name: &str, content: &[u8]) -> Result<usize, VfsError> {
        if !self.inodes[parent_id].is_dir() {
            return Err(VfsError::NotADirectory);
        }

        // Check if file already exists; if so, overwrite its content
        if let InodeKind::Directory { children } = &self.inodes[parent_id].kind {
            for &child_id in children {
                if self.inodes[child_id].name == name {
                    if let InodeKind::File { content: file_content } = &mut self.inodes[child_id].kind {
                        file_content.clear();
                        file_content.extend_from_slice(content);
                        return Ok(child_id);
                    } else {
                        return Err(VfsError::IsDirectory);
                    }
                }
            }
        }

        let new_id = self.allocate_inode(
            parent_id,
            name,
            InodeKind::File {
                content: content.to_vec(),
            },
        );

        if let InodeKind::Directory { children } = &mut self.inodes[parent_id].kind {
            children.push(new_id);
        }

        Ok(new_id)
    }

    /// Lists entries in the specified directory Inode.
    pub fn list_directory(&self, dir_id: usize) -> Result<Vec<DirectoryEntry>, VfsError> {
        match &self.inodes[dir_id].kind {
            InodeKind::Directory { children } => {
                let mut list = Vec::new();
                for &child_id in children {
                    let child = &self.inodes[child_id];
                    list.push(DirectoryEntry {
                        name: child.name.clone(),
                        is_dir: child.is_dir(),
                        size: child.size(),
                    });
                }
                Ok(list)
            }
            _ => Err(VfsError::NotADirectory),
        }
    }

    /// Creates a dynamic procedural file with a generator function (e.g. /proc files).
    pub fn create_proc_file_at(
        &mut self,
        parent_id: usize,
        name: &str,
        generator: fn() -> Vec<u8>,
    ) -> Result<usize, VfsError> {
        if !self.inodes[parent_id].is_dir() {
            return Err(VfsError::NotADirectory);
        }
        let new_id = self.allocate_inode(parent_id, name, InodeKind::ProcFile { generator });

        if let InodeKind::Directory { children } = &mut self.inodes[parent_id].kind {
            children.push(new_id);
        }

        Ok(new_id)
    }

    /// Creates a virtual device file (e.g. /dev/null, /dev/zero, /dev/random).
    pub fn create_device_file_at(
        &mut self,
        parent_id: usize,
        name: &str,
        kind: InodeKind,
    ) -> Result<usize, VfsError> {
        if !self.inodes[parent_id].is_dir() {
            return Err(VfsError::NotADirectory);
        }
        let new_id = self.allocate_inode(parent_id, name, kind);

        if let InodeKind::Directory { children } = &mut self.inodes[parent_id].kind {
            children.push(new_id);
        }

        Ok(new_id)
    }

    /// Reads content from the specified file Inode (static or dynamic pseudo-file).
    pub fn read_file(&self, file_id: usize) -> Result<Cow<'_, [u8]>, VfsError> {
        match &self.inodes[file_id].kind {
            InodeKind::File { content } => Ok(Cow::Borrowed(content.as_slice())),
            InodeKind::ProcFile { generator } => Ok(Cow::Owned(generator())),
            InodeKind::DevNull => Ok(Cow::Borrowed(&[])),
            InodeKind::DevZero => Ok(Cow::Owned(alloc::vec![0u8; 64])),
            InodeKind::DevRandom => Ok(Cow::Owned(dev_random_generator())),
            InodeKind::Directory { .. } => Err(VfsError::IsDirectory),
            InodeKind::Tombstone => Err(VfsError::Deleted),
        }
    }

    /// Computes the absolute path string of an Inode by traversing up to root.
    pub fn get_path(&self, mut node_id: usize) -> String {
        if node_id == 0 {
            return String::from("/");
        }

        let mut segments = Vec::new();
        while node_id != 0 {
            segments.push(self.inodes[node_id].name.as_str());
            node_id = self.inodes[node_id].parent_id;
        }

        let mut path = String::new();
        for segment in segments.iter().rev() {
            path.push('/');
            path.push_str(segment);
        }
        path
    }

    /// Removes an entry from its parent directory and frees its resources.
    /// For directories, recursively cleans up all children.
    pub fn remove_entry(&mut self, target_id: usize) -> Result<(), VfsError> {
        if target_id == 0 {
            return Err(VfsError::RootProtected);
        }

        // Recursively collect all descendant IDs to clean up
        let mut to_clean = Vec::new();
        self.collect_descendants(target_id, &mut to_clean);
        to_clean.push(target_id);

        // Remove target from parent's children list
        let parent_id = self.inodes[target_id].parent_id;
        if let InodeKind::Directory { children } = &mut self.inodes[parent_id].kind
            && let Some(pos) = children.iter().position(|&id| id == target_id)
        {
            children.remove(pos);
        } else {
            return Err(VfsError::NotFound);
        }

        // Free memory by marking removed inodes as tombstones and adding to free-list
        for &id in &to_clean {
            self.inodes[id].kind = InodeKind::Tombstone;
            self.inodes[id].name.clear();
            self.inodes[id].name.shrink_to_fit();
            self.inodes[id].parent_id = 0;
            self.free_inodes.push(id);
        }

        Ok(())
    }

    /// Recursively collects all descendant inode IDs of a directory.
    fn collect_descendants(&self, node_id: usize, result: &mut Vec<usize>) {
        if let InodeKind::Directory { children } = &self.inodes[node_id].kind {
            for &child_id in children {
                result.push(child_id);
                self.collect_descendants(child_id, result);
            }
        }
    }
}

pub static VFS: Spinlock<Vfs> = Spinlock::new(Vfs::new());

/// Initializes the global Virtual File System and root RAM disk.
pub fn init() {
    let mut vfs = VFS.lock();
    vfs.init();
    crate::serial_println!("[VFS] Virtual File System & RAMFS mounted at '/' with {} inodes", vfs.inodes.len());
}

// ============================================================================
// Dynamic Pseudo-File Generators (/proc and /dev)
// ============================================================================

fn proc_uptime_generator() -> Vec<u8> {
    let ticks = crate::arch::idt::ticks();
    // PIT is configured at 100 Hz (10 ms per tick) in drivers/pit.rs
    let freq = crate::drivers::pit::TARGET_FREQUENCY as u64;
    let seconds = ticks / freq;
    let ms = (ticks % freq) * (1000 / freq);
    alloc::format!("uptime: {}.{:03} seconds ({} timer ticks @ {} Hz)\n", seconds, ms, ticks, freq).into_bytes()
}

fn proc_meminfo_generator() -> Vec<u8> {
    let total_kb = crate::memory::allocator::HEAP_SIZE / 1024;
    let free_kb = crate::memory::allocator::free_memory() / 1024;
    let used_kb = crate::memory::allocator::used_memory() / 1024;

    // Physical memory stats from the PMM (real E820 values).
    // Integer MiB: floor for total/free, ceiling for used so the three lines
    // add up (245 = 244 free + 1 used) under the no_std f64-less formatting.
    let phys_total_mb = crate::memory::pmm::total_memory() / (1024 * 1024);
    let phys_free_mb = crate::memory::pmm::free_memory() / (1024 * 1024);
    let phys_used_mb = crate::memory::pmm::used_memory().div_ceil(1024 * 1024);

    alloc::format!(
        "MemTotal:        {} kB\n\
         MemFree:         {} kB\n\
         MemUsed:         {} kB\n\
         HeapCapacity:    {} kB ({} MiB Coalescing Allocator)\n\
         PhysicalTotal:   {} MiB (E820 memory map)\n\
         PhysicalFree:    {} MiB\n\
         PhysicalUsed:    {} MiB\n\
         PMMRange:        2 MiB .. 512 MiB (identity-mapped frames)\n\
         PagingModel:     4-Level x86_64 Long Mode (PML4)\n",
        total_kb, free_kb, used_kb, total_kb, total_kb / 1024,
        phys_total_mb, phys_free_mb, phys_used_mb,
    ).into_bytes()
}

fn proc_cpuinfo_generator() -> Vec<u8> {
    let cpu = crate::arch::cpuid::get_cpu_info();
    alloc::format!(
        "processor:       0\n\
         vendor_id:       {}\n\
         model name:      {}\n\
         architecture:    x86_64 (64-bit Bare-Metal Long Mode)\n",
        cpu.vendor_str(),
        cpu.brand_str()
    ).into_bytes()
}

fn proc_tasks_generator() -> Vec<u8> {
    let sched = crate::task::SCHEDULER.lock();
    let mut out = alloc::format!("{:<4} {:<18} {:<16} {:<10}\n", "PID", "NAME", "STATE", "TICKS");
    out.push_str("------------------------------------------------------\n");
    for task in &sched.tasks {
        let state_str = match task.state {
            crate::task::TaskState::Ready => alloc::format!("READY"),
            crate::task::TaskState::Running => alloc::format!("RUNNING"),
            crate::task::TaskState::Sleeping(w) => alloc::format!("SLEEP(tick={})", w),
            crate::task::TaskState::Dead => alloc::format!("DEAD"),
        };
        out.push_str(&alloc::format!("{:<4} {:<18} {:<16} {:<10}\n", task.id, task.name, state_str, task.ticks));
    }
    out.into_bytes()
}

fn dev_random_generator() -> Vec<u8> {
    let cpu = crate::arch::cpuid::get_cpu_info();
    let mut bytes = alloc::vec::Vec::with_capacity(32);

    if cpu.has_rdrand {
        // Use hardware RDRAND for cryptographically stronger randomness
        for _ in 0..4 {
            let val: u64;
            let ok: u8;
            unsafe {
                core::arch::asm!(
                    "rdrand {val}",
                    "setc {ok}",
                    val = out(reg) val,
                    ok = out(reg_byte) ok,
                    options(nomem, nostack)
                );
            }
            if ok != 0 {
                bytes.extend_from_slice(&val.to_le_bytes());
            } else {
                // RDRAND failed, fill with TSC-seeded fallback
                let fb = crate::arch::cpuid::rdtsc().wrapping_mul(6364136223846793005).wrapping_add(1);
                bytes.extend_from_slice(&fb.to_le_bytes());
            }
        }
    } else {
        // Fallback: improved XorShift64* seeded from RDTSC
        let mut seed = crate::arch::cpuid::rdtsc();
        if seed == 0 { seed = 0xDEAD_BEEF_CAFE_BABE; }
        for _ in 0..4 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed = seed.wrapping_mul(0x2545F4914F6CDD1D); // XorShift64*
            bytes.extend_from_slice(&seed.to_le_bytes());
        }
    }

    bytes.truncate(32);
    bytes
}
