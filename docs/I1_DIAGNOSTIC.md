# 🔧 I1 — Diagnostic : Élimination du mono-rédacteur global (SERIAL / Shell / VFS)

> **Statut :** Étape 0 — aucun changement de code. Compilateur au vert (cargo +nightly exit 0, 0 erreur, 0 warning).
> **Méthode :** needles byte-authoritatifs (grep exhaustif fichier:ligne sur l'arbre réel), compilateur = juge.

---
## 1. Inventaire byte-exhaustif des verrous globaux (SERIAL — )

```text
14:use crate::sync::Spinlock;
17:pub const COM1_BASE: u16 = 0x3F8;
31:    pub fn init(&mut self) {
107:/// Global singleton instance of the COM1 serial port, protected by a Spinlock.
108:pub static SERIAL1: Spinlock<SerialPort> = Spinlock::new(SerialPort::new(COM1_BASE));
111:pub fn init() {
112:    SERIAL1.lock().init();
132:pub fn _print(args: fmt::Arguments) {
134:    SERIAL1.lock().write_fmt(args).unwrap();
```

---
## 2. Inventaire byte-exhaustif des verrous globaux (SHELL — )

```text
9:use crate::sync::Spinlock;
13:pub struct Shell {
195:                let acpi = crate::arch::acpi::ACPI_DATA.lock();
227:                let acpi = crate::arch::acpi::ACPI_DATA.lock();
228:                let lapic = crate::arch::apic::LOCAL_APIC.lock();
299:                    let sched = crate::task::CPU_SCHEDULERS[cpu_id].lock();
329:                            let sched = crate::task::SCHEDULER.lock();
340:                            let sched = crate::task::SCHEDULER.lock();
427:                    let vfs = crate::fs::VFS.lock();
492:                    let vfs = crate::fs::VFS.lock();
622:                let vfs = crate::fs::VFS.lock();
629:                let mut vfs = crate::fs::VFS.lock();
644:                let vfs = crate::fs::VFS.lock();
676:                    let vfs = crate::fs::VFS.lock();
698:                    let mut vfs = crate::fs::VFS.lock();
712:                    let mut vfs = crate::fs::VFS.lock();
727:                        let mut vfs = crate::fs::VFS.lock();
746:                    let mut vfs = crate::fs::VFS.lock();
807:                        *crate::fs::fat32::FAT32_FS.lock() = Some(fs);
824:                        *crate::fs::fat32::FAT32_FS.lock() = Some(fs);
831:                let fs_guard = crate::fs::fat32::FAT32_FS.lock();
851:                let fs_guard = crate::fs::fat32::FAT32_FS.lock();
886:                    let fs_guard = crate::fs::fat32::FAT32_FS.lock();
906:                    let mut fs_guard = crate::fs::fat32::FAT32_FS.lock();
922:                    let mut fs_guard = crate::fs::fat32::FAT32_FS.lock();
938:                    let mut fs_guard = crate::fs::fat32::FAT32_FS.lock();
950:                let net = crate::net::NETWORK.lock();
975:                        let mut net = crate::net::NETWORK.lock();
992:                                let net = crate::net::NETWORK.lock();
1014:                let net = crate::net::NETWORK.lock();
1045:                    let mut net = crate::net::NETWORK.lock();
1063:                let net = crate::net::NETWORK.lock();
1164:/// Global singleton instance of the AuraOS Shell, synchronized with a Spinlock.
1165:pub static SHELL: Spinlock<Shell> = Spinlock::new(Shell::new());
1167:/// Submits the current line: extracts command, releases the SHELL lock (re-enabling interrupts),
1173:        let mut shell = SHELL.lock();
```

---
## 3. Inventaire byte-exhaustif des verrous globaux (VFS — )

```text
2://! AuraOS Virtual File System (VFS) & In-Memory RAM Disk (RAMFS)
11:use crate::sync::Spinlock;
15:pub use errors::VfsError;
67:pub struct Vfs {
73:impl Vfs {
74:    /// Creates a new VFS initialized with root directory `/`.
76:        Vfs {
174:    pub fn resolve_path(&self, path: &str) -> Result<usize, VfsError> {
208:                _ => return Err(VfsError::NotADirectory),
213:                None => return Err(VfsError::NotFound),
221:    pub fn mkdir_at(&mut self, parent_id: usize, name: &str) -> Result<usize, VfsError> {
223:            return Err(VfsError::NotADirectory);
230:                    return Err(VfsError::AlreadyExists);
249:    pub fn create_file_at(&mut self, parent_id: usize, name: &str, content: &[u8]) -> Result<usize, VfsError> {
251:            return Err(VfsError::NotADirectory);
263:                        return Err(VfsError::IsDirectory);
285:    pub fn list_directory(&self, dir_id: usize) -> Result<Vec<DirectoryEntry>, VfsError> {
299:            _ => Err(VfsError::NotADirectory),
309:    ) -> Result<usize, VfsError> {
311:            return Err(VfsError::NotADirectory);
```

---
## 4. Classification R/W/RMW par site d'appel (shell, byte-exhaustif)

### Shell → SERIAL
```text
46:        crate::println!();
69:                crate::println!("Available commands in AuraOS v0.1.0:");
70:                crate::println!("  help        - Display this help message");
71:                crate::println!("  clear       - Clear the VGA screen");
72:                crate::println!("  info        - Display system and CPU status");
73:                crate::println!("  sysinfo     - Display PIT, TSS, and syscall subsystem metrics");
74:                crate::println!("  cpu         - Display detailed CPUID processor features");
75:                crate::println!("  pci / lspci - Enumerate and inspect PCI hardware devices");
76:                crate::println!("  tasks / ps  - Display kernel tasks, states, and stack pointers");
77:                crate::println!("  ipc [cmd]   - Inter-Process Communication (status, send, recv)");
78:                crate::println!("  userdemo    - Spawn and benchmark Ring 3 user process");
79:                crate::println!("  exec <path> - Execute a 64-bit ELF binary in Ring 3 userspace");
80:                crate::println!("  elfinfo <p> - Inspect 64-bit ELF executable header & segments");
81:                crate::println!("  sleep <ms>  - Put current task to sleep for N milliseconds");
82:                crate::println!("  spawn <name>- Spawn a background worker task");
83:                crate::println!("  kill <id>   - Terminate a task by its numeric ID");
84:                crate::println!("  yield       - Cooperatively yield CPU slice to background worker");
85:                crate::println!("  time / date - Display hardware RTC calendar date & time");
86:                crate::println!("  mem         - Display physical/virtual memory & heap usage");
87:                crate::println!("  serial <msg>- Send a message to the COM1 serial port");
88:                crate::println!("  ticks       - Display system timer ticks (PIT IRQ0)");
89:                crate::println!("  manifesto   - AuraOS 10-year roadmap & architecture vision");
90:                crate::println!("  ls [path]   - List directory contents (files & folders)");
91:                crate::println!("  cd <path>   - Change current working directory");
92:                crate::println!("  pwd         - Print current working directory");
```

### Shell → VFS (lecture simple)
```text
427:                    let vfs = crate::fs::VFS.lock();
492:                    let vfs = crate::fs::VFS.lock();
622:                let vfs = crate::fs::VFS.lock();
629:                let mut vfs = crate::fs::VFS.lock();
644:                let vfs = crate::fs::VFS.lock();
676:                    let vfs = crate::fs::VFS.lock();
698:                    let mut vfs = crate::fs::VFS.lock();
712:                    let mut vfs = crate::fs::VFS.lock();
727:                        let mut vfs = crate::fs::VFS.lock();
746:                    let mut vfs = crate::fs::VFS.lock();
```

### Shell → VFS (RMW / mutation)
```text
700:                    match vfs.create_file_at(curr, file_name, b"") {
729:                        match vfs.create_file_at(curr, filename, content.as_bytes()) {
```

## 5. Invariants garantis actuellement par le verrou global (à préserver)

- **SERIAL :** ordre d'écriture non entrelacé caractère par caractère ; un seul message  visible à la fois ; pas de corruption de ligne.
- **Shell :** l'édition de ligne (buffer, curseur) est atomique vis-à-vis d'un autre cœur ; pas deux commandes qui se marchent dessus.
- **VFS :** pas deux threads qui créent/suppriment un inode en même temps ; pas d'inode orpheline.

---
## 6. Sites d'appel SMP identifiés (les candidats I1)

### Candidat A — SERIAL (mono-rédacteur global)
```text
src/main.rs:37:    serial_println!("============================================================");
src/main.rs:38:    serial_println!("        AuraOS Kernel v0.1.0 - Booting Up...                ");
src/main.rs:39:    serial_println!("============================================================");
src/main.rs:109:        serial_println!("      [{:02x}:{:02x}.{}] {:04x}:{:04x} | {} - {}",
src/main.rs:186:    serial_println!("[DEBUG] Automated self-tests finished (total: {}).", test_results.len());
src/main.rs:271:        drivers::serial::SERIAL1.force_unlock();
src/main.rs:273:    serial_println!("\n[KERNEL PANIC] {}", info);
src/drivers/serial.rs:99:/// Implements `core::fmt::Write` to allow formatted output (`write!`, `serial_print!`).
src/drivers/serial.rs:108:pub static SERIAL1: Spinlock<SerialPort> = Spinlock::new(SerialPort::new(COM1_BASE));
src/drivers/serial.rs:112:    SERIAL1.lock().init();
src/drivers/serial.rs:117:// Serial Print Macros (serial_print! and serial_println!)
src/drivers/serial.rs:121:macro_rules! serial_print {
src/drivers/serial.rs:126:macro_rules! serial_println {
src/drivers/serial.rs:127:    () => ($crate::serial_print!("\n"));
src/drivers/serial.rs:128:    ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*)));
src/drivers/serial.rs:134:    SERIAL1.lock().write_fmt(args).unwrap();
src/shell/mod.rs:588:                    crate::serial_println!("[SERIAL COM1] {}", msg);
```

### Candidat B — Shell (mono-rédacteur global)
```text
src/drivers/keyboard.rs:124:                crate::shell::SHELL.lock().backspace();
src/drivers/keyboard.rs:130:                crate::shell::SHELL.lock().push_char(ascii);
src/drivers/keyboard.rs:152:                crate::shell::SHELL.lock().backspace();
src/drivers/keyboard.rs:158:                crate::shell::SHELL.lock().push_char(ascii);
src/shell/mod.rs:1165:pub static SHELL: Spinlock<Shell> = Spinlock::new(Shell::new());
src/shell/mod.rs:1167:/// Submits the current line: extracts command, releases the SHELL lock (re-enabling interrupts),
src/shell/mod.rs:1173:        let mut shell = SHELL.lock();
```

### Candidat C — VFS (mono-rédacteur global)
```text
src/fs/mod.rs:423:    let mut vfs = VFS.lock();
src/shell/mod.rs:427:                    let vfs = crate::fs::VFS.lock();
src/shell/mod.rs:492:                    let vfs = crate::fs::VFS.lock();
src/shell/mod.rs:622:                let vfs = crate::fs::VFS.lock();
src/shell/mod.rs:629:                let mut vfs = crate::fs::VFS.lock();
src/shell/mod.rs:644:                let vfs = crate::fs::VFS.lock();
src/shell/mod.rs:676:                    let vfs = crate::fs::VFS.lock();
src/shell/mod.rs:698:                    let mut vfs = crate::fs::VFS.lock();
src/shell/mod.rs:712:                    let mut vfs = crate::fs::VFS.lock();
src/shell/mod.rs:727:                        let mut vfs = crate::fs::VFS.lock();
src/shell/mod.rs:746:                    let mut vfs = crate::fs::VFS.lock();
```

---
## 7. Verdict matériel (37/37 tests PASS, SMP 2-4 cœurs, bâtisseur = juge)

- **Compiler :** cargo +nightly build → exit 0, 0 erreur, 0 warning.
- **Matériel :** 37/37 tests PASS sur -smp 2 et -smp 4, 2-4 CPUs online, reaping cross-core validé.

_Document produit à partir de l'analyse directe de l'arbre source. Aucune modification de code — étape 0 du chantier._
