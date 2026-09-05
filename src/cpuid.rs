/// ============================================================================
/// CPUID & Hardware Feature Detection Driver
/// ============================================================================
///
/// Queries processor capabilities and hardware features using the x86 `cpuid` instruction.
/// Retrieves Vendor ID (e.g. "GenuineIntel", "AuthenticAMD"), Processor Brand String,
/// and hardware feature flags (SSE, AVX, APIC, TSC, RDRAND).

/// Result of a single `cpuid` instruction execution.
#[derive(Debug, Clone, Copy)]
pub struct CpuidResult {
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
}

/// Executes the `cpuid` instruction with the specified `eax` function and `ecx` sub-function.
pub fn cpuid(eax: u32, ecx: u32) -> CpuidResult {
    let out_eax: u32;
    let out_ebx: u32;
    let out_ecx: u32;
    let out_edx: u32;

    unsafe {
        core::arch::asm!(
            "push rbx",        // Preserve caller's rbx
            "cpuid",
            "mov {tmp_ebx:e}, ebx",
            "pop rbx",
            inout("eax") eax => out_eax,
            inout("ecx") ecx => out_ecx,
            tmp_ebx = out(reg) out_ebx,
            out("edx") out_edx,
            options(nomem, preserves_flags)
        );
    }

    CpuidResult {
        eax: out_eax,
        ebx: out_ebx,
        ecx: out_ecx,
        edx: out_edx,
    }
}

/// Information about the host processor.
pub struct CpuInfo {
    pub vendor: [u8; 12],
    pub brand: [u8; 48],
    pub has_fpu: bool,
    pub has_tsc: bool,
    pub has_apic: bool,
    pub has_sse: bool,
    pub has_sse2: bool,
    pub has_sse3: bool,
    pub has_avx: bool,
    pub has_rdrand: bool,
}

impl CpuInfo {
    /// Returns the CPU vendor as a string slice.
    pub fn vendor_str(&self) -> &str {
        core::str::from_utf8(&self.vendor).unwrap_or("Unknown")
    }

    /// Returns the processor brand name as a string slice.
    pub fn brand_str(&self) -> &str {
        let len = self.brand.iter().position(|&b| b == 0).unwrap_or(self.brand.len());
        core::str::from_utf8(&self.brand[..len]).unwrap_or("Generic x86_64 Processor").trim()
    }
}

/// Queries the processor and returns populated `CpuInfo`.
pub fn get_cpu_info() -> CpuInfo {
    // 1. Vendor string (CPUID function 0)
    let f0 = cpuid(0, 0);
    let mut vendor = [0u8; 12];
    vendor[0..4].copy_from_slice(&f0.ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&f0.edx.to_le_bytes());
    vendor[8..12].copy_from_slice(&f0.ecx.to_le_bytes());

    // 2. Feature flags (CPUID function 1)
    let f1 = cpuid(1, 0);
    let has_fpu = (f1.edx & (1 << 0)) != 0;
    let has_tsc = (f1.edx & (1 << 4)) != 0;
    let has_apic = (f1.edx & (1 << 9)) != 0;
    let has_sse = (f1.edx & (1 << 25)) != 0;
    let has_sse2 = (f1.edx & (1 << 26)) != 0;
    let has_sse3 = (f1.ecx & (1 << 0)) != 0;
    let has_avx = (f1.ecx & (1 << 28)) != 0;
    let has_rdrand = (f1.ecx & (1 << 30)) != 0;

    // 3. Processor Brand string (CPUID functions 0x80000002 .. 0x80000004)
    let mut brand = [0u8; 48];
    let ext_max = cpuid(0x80000000, 0).eax;
    if ext_max >= 0x80000004 {
        for i in 0..3 {
            let res = cpuid(0x80000002 + i, 0);
            let offset = (i as usize) * 16;
            brand[offset..offset + 4].copy_from_slice(&res.eax.to_le_bytes());
            brand[offset + 4..offset + 8].copy_from_slice(&res.ebx.to_le_bytes());
            brand[offset + 8..offset + 12].copy_from_slice(&res.ecx.to_le_bytes());
            brand[offset + 12..offset + 16].copy_from_slice(&res.edx.to_le_bytes());
        }
    }

    CpuInfo {
        vendor,
        brand,
        has_fpu,
        has_tsc,
        has_apic,
        has_sse,
        has_sse2,
        has_sse3,
        has_avx,
        has_rdrand,
    }
}

/// Reads the current CPU Time Stamp Counter (TSC) cycle count using `rdtsc`.
#[inline]
pub fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((hi as u64) << 32) | (lo as u64)
}
