//! ============================================================================
//! AuraOS 64-bit ELF (Executable and Linkable Format) Loader & Runtime
//! ============================================================================
//!
//! Implements a pure `#![no_std]` Rust parser and loader for System V AMD64
//! ELF executables (ELF64).
//!
//! Handles:
//!   - Parsing and validating 64-bit ELF file headers (magic, endianness, arch)
//!   - Reading and inspecting program headers (`PT_LOAD`, `PT_GNU_STACK`, etc.)
//!   - Page-aligned loading of segments into an isolated `AddressSpace` (PML4)
//!   - Zero-filling `.bss` sections (where `p_memsz > p_filesz`)
//!   - User stack allocation and privilege enforcement (`USER_ACCESSIBLE`)
//!   - Dynamic generation of conforming ELF64 executables (`create_elf_binary`)
#![allow(dead_code)]

use alloc::vec;
use alloc::vec::Vec;
use crate::memory::paging::{VirtAddr, PAGE_SIZE};
use crate::memory::user_space::AddressSpace;

// ELF Magic & Identification Constants
pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
pub const ELF_CLASS_64: u8 = 2;
pub const ELF_DATA_2LSB: u8 = 1; // 2's complement, little endian
pub const ELF_VERSION_CURRENT: u8 = 1;
pub const ELF_OSABI_SYSV: u8 = 0;
pub const ELF_MACHINE_X86_64: u16 = 0x3E; // 62 = AMD x86-64

// ELF File Types
pub const ET_NONE: u16 = 0;
pub const ET_REL: u16 = 1;
pub const ET_EXEC: u16 = 2;
pub const ET_DYN: u16 = 3;

// Program Header Types
pub const PT_NULL: u32 = 0;
pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_INTERP: u32 = 3;
pub const PT_NOTE: u32 = 4;
pub const PT_SHLIB: u32 = 5;
pub const PT_PHDR: u32 = 6;
pub const PT_TLS: u32 = 7;
pub const PT_GNU_STACK: u32 = 0x6474_e551;

// Segment Permission Flags
pub const PF_X: u32 = 1 << 0; // Execute
pub const PF_W: u32 = 1 << 1; // Write
pub const PF_R: u32 = 1 << 2; // Read

/// Standard 64-bit ELF File Header (64 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elf64Header {
    pub ident: [u8; 16],
    pub elf_type: u16,
    pub machine: u16,
    pub version: u32,
    pub entry: u64,
    pub phoff: u64,
    pub shoff: u64,
    pub flags: u32,
    pub ehsize: u16,
    pub phentsize: u16,
    pub phnum: u16,
    pub shentsize: u16,
    pub shnum: u16,
    pub shstrndx: u16,
}

/// Standard 64-bit ELF Program Header (56 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elf64ProgramHeader {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

/// Errors that can occur while parsing or loading an ELF binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    BinaryTooSmall,
    InvalidMagic,
    UnsupportedClass,
    UnsupportedEndianness,
    UnsupportedVersion,
    UnsupportedMachine,
    UnsupportedType,
    InvalidProgramHeaders,
    AddressSpaceAllocationFailed,
    SegmentMappingFailed,
    NoLoadableSegments,
}

impl ElfError {
    pub fn as_str(&self) -> &'static str {
        match self {
            ElfError::BinaryTooSmall => "File is smaller than 64-byte ELF header",
            ElfError::InvalidMagic => "Invalid ELF magic signature",
            ElfError::UnsupportedClass => "Unsupported ELF class (must be 64-bit)",
            ElfError::UnsupportedEndianness => "Unsupported endianness (must be little-endian)",
            ElfError::UnsupportedVersion => "Unsupported ELF specification version",
            ElfError::UnsupportedMachine => "Unsupported architecture (must be x86_64)",
            ElfError::UnsupportedType => "Unsupported file type (must be ET_EXEC or ET_DYN)",
            ElfError::InvalidProgramHeaders => "Corrupted or out-of-bounds program headers",
            ElfError::AddressSpaceAllocationFailed => "Failed to allocate per-process AddressSpace (PML4)",
            ElfError::SegmentMappingFailed => "Failed to allocate or map physical page for segment",
            ElfError::NoLoadableSegments => "Binary contains no PT_LOAD segments",
        }
    }
}

/// A parsed ELF64 binary holding a reference to raw byte data and validated header.
#[derive(Debug, PartialEq, Eq)]
pub struct ElfBinary<'a> {
    pub data: &'a [u8],
    pub header: Elf64Header,
}

impl<'a> ElfBinary<'a> {
    /// Parses and validates the ELF64 header from raw binary slice.
    pub fn parse(data: &'a [u8]) -> Result<Self, ElfError> {
        if data.len() < core::mem::size_of::<Elf64Header>() {
            return Err(ElfError::BinaryTooSmall);
        }

        // Safely extract the 64-byte header
        let header = unsafe { core::ptr::read_unaligned(data.as_ptr() as *const Elf64Header) };

        // 1. Validate magic: 0x7F 'E' 'L' 'F'
        if header.ident[0..4] != ELF_MAGIC {
            return Err(ElfError::InvalidMagic);
        }

        // 2. Validate 64-bit class
        if header.ident[4] != ELF_CLASS_64 {
            return Err(ElfError::UnsupportedClass);
        }

        // 3. Validate 2's complement little-endian
        if header.ident[5] != ELF_DATA_2LSB {
            return Err(ElfError::UnsupportedEndianness);
        }

        // 4. Validate ELF version
        if header.ident[6] != ELF_VERSION_CURRENT || header.version != 1 {
            return Err(ElfError::UnsupportedVersion);
        }

        // 5. Validate AMD x86-64 machine type
        if header.machine != ELF_MACHINE_X86_64 {
            return Err(ElfError::UnsupportedMachine);
        }

        // 6. Validate executable or dynamic object
        if header.elf_type != ET_EXEC && header.elf_type != ET_DYN {
            return Err(ElfError::UnsupportedType);
        }

        Ok(ElfBinary { data, header })
    }

    /// Reads all program headers from the ELF binary.
    pub fn program_headers(&self) -> Result<Vec<Elf64ProgramHeader>, ElfError> {
        let phoff = self.header.phoff as usize;
        let phentsize = self.header.phentsize as usize;
        let phnum = self.header.phnum as usize;

        if phnum == 0 {
            return Ok(Vec::new());
        }

        if phentsize < core::mem::size_of::<Elf64ProgramHeader>() {
            return Err(ElfError::InvalidProgramHeaders);
        }

        let required_len = phoff.checked_add(phnum.checked_mul(phentsize).ok_or(ElfError::InvalidProgramHeaders)?)
            .ok_or(ElfError::InvalidProgramHeaders)?;

        if self.data.len() < required_len {
            return Err(ElfError::InvalidProgramHeaders);
        }

        let mut headers = Vec::with_capacity(phnum);
        for i in 0..phnum {
            let offset = phoff + i * phentsize;
            let phdr = unsafe {
                core::ptr::read_unaligned((self.data.as_ptr().add(offset)) as *const Elf64ProgramHeader)
            };
            headers.push(phdr);
        }

        Ok(headers)
    }

    /// Returns the entry point virtual address specified in the ELF header.
    pub fn entry_point(&self) -> u64 {
        self.header.entry
    }
}

/// Result of loading an ELF binary into memory.
pub struct LoadedElf {
    pub entry_point: u64,
    pub user_stack_top: u64,
    pub space: AddressSpace,
    pub segments_loaded: usize,
    pub total_bytes: usize,
}

/// Standard default base address for user executables (1 GiB boundary, above kernel identity map)
pub const DEFAULT_USER_CODE_BASE: u64 = 0x0000_0000_4000_0000;

/// Standard default top of user stack (16 KiB stack below 2 GiB boundary)
pub const DEFAULT_USER_STACK_TOP: u64 = 0x0000_0000_8000_0000;
pub const DEFAULT_USER_STACK_PAGES: usize = 4; // 16 KiB

/// Loads an ELF binary into a newly created isolated `AddressSpace`.
pub fn load_elf(data: &[u8]) -> Result<LoadedElf, ElfError> {
    let elf = ElfBinary::parse(data)?;
    let phdrs = elf.program_headers()?;

    let mut space = AddressSpace::new().ok_or(ElfError::AddressSpaceAllocationFailed)?;
    let mut segments_loaded = 0;
    let mut total_bytes = 0;

    for phdr in &phdrs {
        if phdr.p_type != PT_LOAD {
            continue;
        }

        if phdr.p_memsz == 0 {
            continue;
        }

        let file_offset = phdr.p_offset as usize;
        let file_sz = phdr.p_filesz as usize;
        let mem_sz = phdr.p_memsz as usize;
        let vaddr = phdr.p_vaddr;

        if file_sz > mem_sz {
            return Err(ElfError::InvalidProgramHeaders);
        }

        if file_offset + file_sz > data.len() {
            return Err(ElfError::InvalidProgramHeaders);
        }

        // Determine page range covering this segment
        let page_start = (vaddr / PAGE_SIZE as u64) * PAGE_SIZE as u64;
        let page_end = ((vaddr + mem_sz as u64 + PAGE_SIZE as u64 - 1) / PAGE_SIZE as u64) * PAGE_SIZE as u64;

        let writable = (phdr.p_flags & PF_W) != 0;

        // Allocate and populate each page in the virtual range
        let mut curr_page = page_start;
        while curr_page < page_end {
            // Allocate zeroed page frame in kernel space and map into AddressSpace
            let frame_ptr = space.allocate_and_map_page(VirtAddr(curr_page), writable || true)
                .ok_or(ElfError::SegmentMappingFailed)?;

            // Calculate overlap between this page [curr_page, curr_page + PAGE_SIZE)
            // and the segment data range [vaddr, vaddr + file_sz)
            let seg_start = vaddr;
            let seg_end = vaddr + file_sz as u64;

            let overlap_start = core::cmp::max(curr_page, seg_start);
            let overlap_end = core::cmp::min(curr_page + PAGE_SIZE as u64, seg_end);

            if overlap_end > overlap_start {
                let frame_dest_offset = (overlap_start - curr_page) as usize;
                let data_src_offset = file_offset + (overlap_start - seg_start) as usize;
                let copy_len = (overlap_end - overlap_start) as usize;

                unsafe {
                    core::ptr::copy_nonoverlapping(
                        data.as_ptr().add(data_src_offset),
                        frame_ptr.add(frame_dest_offset),
                        copy_len,
                    );
                }
            }

            curr_page += PAGE_SIZE as u64;
        }

        segments_loaded += 1;
        total_bytes += mem_sz;
    }

    if segments_loaded == 0 {
        return Err(ElfError::NoLoadableSegments);
    }

    // Allocate user stack
    let stack_top = VirtAddr(DEFAULT_USER_STACK_TOP);
    if !space.allocate_user_stack(stack_top, DEFAULT_USER_STACK_PAGES) {
        return Err(ElfError::SegmentMappingFailed);
    }

    Ok(LoadedElf {
        entry_point: elf.entry_point(),
        user_stack_top: DEFAULT_USER_STACK_TOP,
        space,
        segments_loaded,
        total_bytes,
    })
}

/// Loads an ELF binary and spawns it as a Ring 3 user process in the scheduler.
pub fn load_and_spawn(name: &'static str, data: &[u8]) -> Result<usize, ElfError> {
    let loaded = load_elf(data)?;
    let pml4_phys = loaded.space.pml4_phys().as_u64();
    let entry = loaded.entry_point;
    let stack_top = loaded.user_stack_top;

    // Retain address space memory for the lifetime of the process
    core::mem::forget(loaded.space);

    let pid = crate::task::spawn_user(name, entry, stack_top, pml4_phys);
    Ok(pid)
}

/// Helper function to construct a fully standard, conforming 64-bit ELF executable.
///
/// Layout:
/// - 64-byte `Elf64Header`
/// - 56-byte `Elf64ProgramHeader` (PT_LOAD)
/// - 8-byte padding to 128-byte alignment (0x80)
/// - Machine code payload + data payload starting at file offset 0x80
pub fn create_elf_binary(vaddr: u64, code: &[u8], data: &[u8]) -> Vec<u8> {
    const HEADER_OFFSET: usize = 0x80;
    let payload_len = code.len() + data.len();
    let total_file_size = HEADER_OFFSET + payload_len;

    let mut elf = vec![0u8; total_file_size];

    // 1. Build ELF64 Header
    let ehdr = Elf64Header {
        ident: [
            0x7F, b'E', b'L', b'F', // Magic
            ELF_CLASS_64,           // 64-bit
            ELF_DATA_2LSB,          // Little endian
            ELF_VERSION_CURRENT,    // Version 1
            ELF_OSABI_SYSV,         // System V ABI
            0, 0, 0, 0, 0, 0, 0, 0, // Padding
        ],
        elf_type: ET_EXEC,
        machine: ELF_MACHINE_X86_64,
        version: 1,
        entry: vaddr,
        phoff: core::mem::size_of::<Elf64Header>() as u64, // 64
        shoff: 0,
        flags: 0,
        ehsize: core::mem::size_of::<Elf64Header>() as u16,
        phentsize: core::mem::size_of::<Elf64ProgramHeader>() as u16,
        phnum: 1,
        shentsize: 64,
        shnum: 0,
        shstrndx: 0,
    };

    // 2. Build Program Header (PT_LOAD)
    let phdr = Elf64ProgramHeader {
        p_type: PT_LOAD,
        p_flags: PF_R | PF_W | PF_X,
        p_offset: HEADER_OFFSET as u64,
        p_vaddr: vaddr,
        p_paddr: vaddr,
        p_filesz: payload_len as u64,
        p_memsz: payload_len as u64,
        p_align: PAGE_SIZE as u64,
    };

    unsafe {
        // Copy ELF Header
        core::ptr::copy_nonoverlapping(
            &ehdr as *const Elf64Header as *const u8,
            elf.as_mut_ptr(),
            core::mem::size_of::<Elf64Header>(),
        );

        // Copy Program Header
        core::ptr::copy_nonoverlapping(
            &phdr as *const Elf64ProgramHeader as *const u8,
            elf.as_mut_ptr().add(core::mem::size_of::<Elf64Header>()),
            core::mem::size_of::<Elf64ProgramHeader>(),
        );

        // Copy Code payload
        if !code.is_empty() {
            core::ptr::copy_nonoverlapping(
                code.as_ptr(),
                elf.as_mut_ptr().add(HEADER_OFFSET),
                code.len(),
            );
        }

        // Copy Data payload
        if !data.is_empty() {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                elf.as_mut_ptr().add(HEADER_OFFSET + code.len()),
                data.len(),
            );
        }
    }

    elf
}

/// Builds the `/bin/hello` 64-bit ELF executable.
/// When executed in Ring 3:
///   1. Issues SYS_WRITE to print greeting to console & serial
///   2. Issues SYS_GETPID to query its process ID
///   3. Issues SYS_WRITE to print success confirmation
///   4. Issues SYS_EXIT(0) to terminate cleanly
pub fn build_hello_elf() -> Vec<u8> {
    let vaddr = DEFAULT_USER_CODE_BASE;
    let msg1 = b"Hello from AuraOS Ring 3 Userspace (ELF64)!\n";
    let msg2 = b"[OK] Process PID acquired, exiting cleanly.\n";

    let mut data = Vec::new();
    data.extend_from_slice(msg1);
    data.extend_from_slice(msg2);

    let code_len = 71usize;
    let msg1_addr = vaddr + code_len as u64;
    let msg2_addr = msg1_addr + msg1.len() as u64;

    let mut code = Vec::with_capacity(code_len);

    // 1. write(1, msg1, len)
    code.extend_from_slice(&[0xb8, 0x04, 0x00, 0x00, 0x00]); // mov eax, 4 (SYS_WRITE)
    code.extend_from_slice(&[0xbf, 0x01, 0x00, 0x00, 0x00]); // mov edi, 1 (stdout)
    code.extend_from_slice(&[0x48, 0xbe]);                    // mov rsi, imm64
    code.extend_from_slice(&msg1_addr.to_le_bytes());
    code.extend_from_slice(&[0xba, msg1.len() as u8, 0x00, 0x00, 0x00]); // mov edx, len
    code.extend_from_slice(&[0x0f, 0x05]);                    // syscall

    // 2. getpid()
    code.extend_from_slice(&[0xb8, 0x27, 0x00, 0x00, 0x00]); // mov eax, 39 (SYS_GETPID)
    code.extend_from_slice(&[0x0f, 0x05]);                    // syscall

    // 3. write(1, msg2, len)
    code.extend_from_slice(&[0xb8, 0x04, 0x00, 0x00, 0x00]); // mov eax, 4 (SYS_WRITE)
    code.extend_from_slice(&[0xbf, 0x01, 0x00, 0x00, 0x00]); // mov edi, 1 (stdout)
    code.extend_from_slice(&[0x48, 0xbe]);                    // mov rsi, imm64
    code.extend_from_slice(&msg2_addr.to_le_bytes());
    code.extend_from_slice(&[0xba, msg2.len() as u8, 0x00, 0x00, 0x00]); // mov edx, len
    code.extend_from_slice(&[0x0f, 0x05]);                    // syscall

    // 4. exit(0)
    code.extend_from_slice(&[0xb8, 0x01, 0x00, 0x00, 0x00]); // mov eax, 1 (SYS_EXIT)
    code.extend_from_slice(&[0x48, 0x31, 0xff]);             // xor rdi, rdi
    code.extend_from_slice(&[0x0f, 0x05]);                    // syscall

    create_elf_binary(vaddr, &code, &data)
}

/// Builds the `/bin/counter` 64-bit ELF executable.
/// Demonstrates preemptive multitasking and cooperative yields (`SYS_YIELD`) in Ring 3.
pub fn build_counter_elf() -> Vec<u8> {
    let vaddr = DEFAULT_USER_CODE_BASE;
    let msg1 = b"[Counter] Ring 3 loop started.\n";
    let msg2 = b"[Counter] Ring 3 iterations finished!\n";

    let mut data = Vec::new();
    data.extend_from_slice(msg1);
    data.extend_from_slice(msg2);

    let code_len = 81usize;
    let msg1_addr = vaddr + code_len as u64;
    let msg2_addr = msg1_addr + msg1.len() as u64;

    let mut code = Vec::with_capacity(code_len);

    // 1. write(1, msg1, len)
    code.extend_from_slice(&[0xb8, 0x04, 0x00, 0x00, 0x00]); // mov eax, 4 (SYS_WRITE)
    code.extend_from_slice(&[0xbf, 0x01, 0x00, 0x00, 0x00]); // mov edi, 1 (stdout)
    code.extend_from_slice(&[0x48, 0xbe]);                    // mov rsi, imm64
    code.extend_from_slice(&msg1_addr.to_le_bytes());
    code.extend_from_slice(&[0xba, msg1.len() as u8, 0x00, 0x00, 0x00]); // mov edx, len
    code.extend_from_slice(&[0x0f, 0x05]);                    // syscall

    // 2. Loop 3 times with SYS_YIELD
    code.extend_from_slice(&[0xbb, 0x03, 0x00, 0x00, 0x00]); // mov ebx, 3
    // .loop:
    code.extend_from_slice(&[0xb8, 0x18, 0x00, 0x00, 0x00]); // mov eax, 24 (SYS_YIELD)
    code.extend_from_slice(&[0x0f, 0x05]);                    // syscall
    code.extend_from_slice(&[0x48, 0xff, 0xcb]);             // dec rbx
    code.extend_from_slice(&[0x75, 0xf4]);                    // jnz .loop (-12)

    // 3. write(1, msg2, len)
    code.extend_from_slice(&[0xb8, 0x04, 0x00, 0x00, 0x00]); // mov eax, 4 (SYS_WRITE)
    code.extend_from_slice(&[0xbf, 0x01, 0x00, 0x00, 0x00]); // mov edi, 1 (stdout)
    code.extend_from_slice(&[0x48, 0xbe]);                    // mov rsi, imm64
    code.extend_from_slice(&msg2_addr.to_le_bytes());
    code.extend_from_slice(&[0xba, msg2.len() as u8, 0x00, 0x00, 0x00]); // mov edx, len
    code.extend_from_slice(&[0x0f, 0x05]);                    // syscall

    // 4. exit(0)
    code.extend_from_slice(&[0xb8, 0x01, 0x00, 0x00, 0x00]); // mov eax, 1 (SYS_EXIT)
    code.extend_from_slice(&[0x48, 0x31, 0xff]);             // xor rdi, rdi
    code.extend_from_slice(&[0x0f, 0x05]);                    // syscall

    create_elf_binary(vaddr, &code, &data)
}

/// Builds the `/bin/init` microkernel init daemon ELF64 executable.
pub fn build_init_elf() -> Vec<u8> {
    let vaddr = DEFAULT_USER_CODE_BASE;
    let banner = b"=== AuraOS Microkernel Init (PID 1 Service) ===\n";
    let status = b"[Init] Core userspace server initialized.\n";

    let mut data = Vec::new();
    data.extend_from_slice(banner);
    data.extend_from_slice(status);

    let code_len = 71usize;
    let banner_addr = vaddr + code_len as u64;
    let status_addr = banner_addr + banner.len() as u64;

    let mut code = Vec::with_capacity(code_len);

    // 1. write(1, banner, len)
    code.extend_from_slice(&[0xb8, 0x04, 0x00, 0x00, 0x00]);
    code.extend_from_slice(&[0xbf, 0x01, 0x00, 0x00, 0x00]);
    code.extend_from_slice(&[0x48, 0xbe]);
    code.extend_from_slice(&banner_addr.to_le_bytes());
    code.extend_from_slice(&[0xba, banner.len() as u8, 0x00, 0x00, 0x00]);
    code.extend_from_slice(&[0x0f, 0x05]);

    // 2. getpid()
    code.extend_from_slice(&[0xb8, 0x27, 0x00, 0x00, 0x00]);
    code.extend_from_slice(&[0x0f, 0x05]);

    // 3. write(1, status, len)
    code.extend_from_slice(&[0xb8, 0x04, 0x00, 0x00, 0x00]);
    code.extend_from_slice(&[0xbf, 0x01, 0x00, 0x00, 0x00]);
    code.extend_from_slice(&[0x48, 0xbe]);
    code.extend_from_slice(&status_addr.to_le_bytes());
    code.extend_from_slice(&[0xba, status.len() as u8, 0x00, 0x00, 0x00]);
    code.extend_from_slice(&[0x0f, 0x05]);

    // 4. exit(0)
    code.extend_from_slice(&[0xb8, 0x01, 0x00, 0x00, 0x00]);
    code.extend_from_slice(&[0x48, 0x31, 0xff]);
    code.extend_from_slice(&[0x0f, 0x05]);

    create_elf_binary(vaddr, &code, &data)
}

