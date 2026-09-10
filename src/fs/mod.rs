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
            InodeKind::DevNull | InodeKind::DevZero | InodeKind::DevRandom => 0,
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
}

impl Vfs {
    /// Creates a new VFS initialized with root directory `/`.
    pub const fn new() -> Self {
        Vfs {
            inodes: Vec::new(),
            current_inode: 0,
        }
    }

    /// Initializes the root filesystem and populates standard system files.
    pub fn init(&mut self) {
        self.inodes.clear();

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
    pub fn resolve_path(&self, path: &str) -> Result<usize, &'static str> {
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
                _ => return Err("Not a directory in path"),
            };

            match next {
                Some(id) => curr = id,
                None => return Err("File or directory not found"),
            }
        }

        Ok(curr)
    }

    /// Creates a new directory inside the specified parent directory Inode.
    pub fn mkdir_at(&mut self, parent_id: usize, name: &str) -> Result<usize, &'static str> {
        if !self.inodes[parent_id].is_dir() {
            return Err("Parent is not a directory");
        }

        // Check for duplicates
        if let InodeKind::Directory { children } = &self.inodes[parent_id].kind {
            for &child_id in children {
                if self.inodes[child_id].name == name {
                    return Err("Entry already exists");
                }
            }
        }

        let new_id = self.inodes.len();
        self.inodes.push(Inode {
            id: new_id,
            parent_id,
            name: String::from(name),
            kind: InodeKind::Directory { children: Vec::new() },
        });

        if let InodeKind::Directory { children } = &mut self.inodes[parent_id].kind {
            children.push(new_id);
        }

        Ok(new_id)
    }

    /// Creates a new regular file with initial content inside the parent directory Inode.
    pub fn create_file_at(&mut self, parent_id: usize, name: &str, content: &[u8]) -> Result<usize, &'static str> {
        if !self.inodes[parent_id].is_dir() {
            return Err("Parent is not a directory");
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
                        return Err("Target exists and is a directory");
                    }
                }
            }
        }

        let new_id = self.inodes.len();
        self.inodes.push(Inode {
            id: new_id,
            parent_id,
            name: String::from(name),
            kind: InodeKind::File {
                content: content.to_vec(),
            },
        });

        if let InodeKind::Directory { children } = &mut self.inodes[parent_id].kind {
            children.push(new_id);
        }

        Ok(new_id)
    }

    /// Lists entries in the specified directory Inode.
    pub fn list_directory(&self, dir_id: usize) -> Result<Vec<DirectoryEntry>, &'static str> {
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
            _ => Err("Not a directory"),
        }
    }

    /// Creates a dynamic procedural file with a generator function (e.g. /proc files).
    pub fn create_proc_file_at(
        &mut self,
        parent_id: usize,
        name: &str,
        generator: fn() -> Vec<u8>,
    ) -> Result<usize, &'static str> {
        if !self.inodes[parent_id].is_dir() {
            return Err("Parent is not a directory");
        }
        let new_id = self.inodes.len();
        self.inodes.push(Inode {
            id: new_id,
            parent_id,
            name: String::from(name),
            kind: InodeKind::ProcFile { generator },
        });

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
    ) -> Result<usize, &'static str> {
        if !self.inodes[parent_id].is_dir() {
            return Err("Parent is not a directory");
        }
        let new_id = self.inodes.len();
        self.inodes.push(Inode {
            id: new_id,
            parent_id,
            name: String::from(name),
            kind,
        });

        if let InodeKind::Directory { children } = &mut self.inodes[parent_id].kind {
            children.push(new_id);
        }

        Ok(new_id)
    }

    /// Reads content from the specified file Inode (static or dynamic pseudo-file).
    pub fn read_file(&self, file_id: usize) -> Result<Cow<'_, [u8]>, &'static str> {
        match &self.inodes[file_id].kind {
            InodeKind::File { content } => Ok(Cow::Borrowed(content.as_slice())),
            InodeKind::ProcFile { generator } => Ok(Cow::Owned(generator())),
            InodeKind::DevNull => Ok(Cow::Borrowed(&[])),
            InodeKind::DevZero => Ok(Cow::Owned(alloc::vec![0u8; 64])),
            InodeKind::DevRandom => Ok(Cow::Owned(dev_random_generator())),
            InodeKind::Directory { .. } => Err("Cannot read directory as file"),
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
    pub fn remove_entry(&mut self, target_id: usize) -> Result<(), &'static str> {
        if target_id == 0 {
            return Err("Cannot remove root directory");
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
            return Err("Entry not found in parent");
        }

        // Free memory by clearing content/children of all removed inodes
        for &id in &to_clean {
            match &mut self.inodes[id].kind {
                InodeKind::File { content } => {
                    content.clear();
                    content.shrink_to_fit();
                }
                InodeKind::Directory { children } => {
                    children.clear();
                    children.shrink_to_fit();
                }
                _ => {}
            }
            self.inodes[id].name.clear();
            self.inodes[id].name.shrink_to_fit();
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
    // PIT tick rate is ~18.2 Hz (approx 55 ms per tick)
    let seconds = ticks / 18;
    let ms = (ticks % 18) * 55;
    alloc::format!("uptime: {}.{:03} seconds ({} timer ticks)\n", seconds, ms, ticks).into_bytes()
}

fn proc_meminfo_generator() -> Vec<u8> {
    alloc::format!(
        "MemTotal:        8192 kB\n\
         MemFree:         4096 kB\n\
         HeapCapacity:    8192 kB (8 MiB Coalescing Allocator)\n\
         PagingModel:     4-Level x86_64 Long Mode (PML4)\n"
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
    let mut bytes = alloc::vec::Vec::with_capacity(32);
    let mut seed = crate::arch::cpuid::rdtsc();
    for _ in 0..32 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        bytes.push((seed & 0xFF) as u8);
    }
    bytes
}
