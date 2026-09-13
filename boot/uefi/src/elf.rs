//! ============================================================================
//! Minimal ELF64 Loader for UEFI Boot
//! ============================================================================
//!
//! Parses an ELF64 executable image and copies its LOAD segments into physical
//! memory at their designated physical addresses, initializing BSS to zero.

#![allow(dead_code)]

pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
pub const ELF_CLASS_64: u8 = 2;
pub const ELF_DATA_LSB: u8 = 1;
pub const EM_X86_64: u16 = 0x3E;
pub const PT_LOAD: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Elf64Header {
    pub e_ident: [u8; 16],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
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

/// Parses the ELF header and copies all PT_LOAD segments to their physical addresses.
/// Returns the 64-bit entry point virtual/physical address on success.
pub fn load_elf(elf_bytes: &[u8]) -> Result<u64, &'static str> {
    if elf_bytes.len() < core::mem::size_of::<Elf64Header>() {
        return Err("ELF buffer too small for header");
    }

    let header = unsafe { &*(elf_bytes.as_ptr() as *const Elf64Header) };

    if header.e_ident[0..4] != ELF_MAGIC {
        return Err("Invalid ELF magic");
    }
    if header.e_ident[4] != ELF_CLASS_64 {
        return Err("Not a 64-bit ELF");
    }
    if header.e_ident[5] != ELF_DATA_LSB {
        return Err("Not a little-endian ELF");
    }
    if header.e_machine != EM_X86_64 {
        return Err("Not an x86_64 ELF");
    }

    let ph_offset = header.e_phoff as usize;
    let ph_count = header.e_phnum as usize;
    let ph_entry_size = header.e_phentsize as usize;

    for i in 0..ph_count {
        let entry_offset = ph_offset + i * ph_entry_size;
        if entry_offset + core::mem::size_of::<Elf64ProgramHeader>() > elf_bytes.len() {
            return Err("Program header out of bounds");
        }

        let ph = unsafe { &*(elf_bytes.as_ptr().add(entry_offset) as *const Elf64ProgramHeader) };

        if ph.p_type == PT_LOAD {
            let file_offset = ph.p_offset as usize;
            let file_size = ph.p_filesz as usize;
            let mem_size = ph.p_memsz as usize;
            let dest_addr = ph.p_paddr;

            if file_offset + file_size > elf_bytes.len() {
                return Err("Segment file data out of bounds");
            }

            // Copy file data to destination memory
            unsafe {
                let dest_ptr = dest_addr as *mut u8;
                core::ptr::copy_nonoverlapping(
                    elf_bytes.as_ptr().add(file_offset),
                    dest_ptr,
                    file_size,
                );

                // Zero-fill remaining BSS memory if mem_size > file_size
                if mem_size > file_size {
                    core::ptr::write_bytes(
                        dest_ptr.add(file_size),
                        0,
                        mem_size - file_size,
                    );
                }
            }
        }
    }

    Ok(header.e_entry)
}
