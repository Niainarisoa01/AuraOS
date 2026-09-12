# 🌌 AuraOS v0.1.0-alpha — Avantages et Inconvénients

> **Date d'analyse :** 11 Septembre 2026  
> **Version analysée :** v0.1.0-alpha  
> **Métriques :** 12 239 lignes de Rust · 43 fichiers · 275 Ko (release) · 0 erreur · 0 warning

---

## ✅ AVANTAGES (15)

---

### 🏗️ A1 — Architecture Modulaire Exemplaire

Le code source est organisé en **9 modules indépendants** avec séparation claire des responsabilités :

```
src/
├── arch/       → 11 fichiers (CPU, GDT, IDT, PIC, ACPI, APIC, syscall)
├── drivers/    → 12 fichiers (VGA, serial, keyboard, mouse, PCI, ATA, e1000)
├── memory/     →  4 fichiers (heap, paging, user space isolation)
├── task/       →  2 fichiers (scheduler préemptif, IPC message passing)
├── fs/         →  3 fichiers (VFS/RAMFS, FAT32 parser, ELF64 loader)
├── net/        →  6 fichiers (ethernet, ARP, IPv4, ICMP, UDP)
├── gui/        →  1 fichier  (canvas 2D, alpha blending)
├── shell/      →  1 fichier  (shell interactif 25+ commandes)
└── tests/      →  1 fichier  (32 tests automatisés)
```

Chaque sous-système peut évoluer indépendamment sans casser les autres. Le couplage inter-modules est minimal et passe par des interfaces publiques bien définies.

---

### 🏗️ A2 — Zéro Dépendance Externe (hors bootloader)

Le projet n'a qu'**une seule dépendance** : `bootloader 0.9.x`. Tout le reste est implémenté **from scratch** en Rust pur :

- Allocateur mémoire à liste chaînée avec coalescing
- Spinlock avec sécurité d'interruption
- 12 drivers matériels (VGA, serial, clavier, souris, PCI, ATA, PIT, CMOS, e1000, framebuffer, BGA, mouse)
- Pile réseau complète (Ethernet/ARP/IPv4/ICMP/UDP)
- Système de fichiers virtuel avec /proc et /dev
- Shell interactif avec 25+ commandes
- Suite de 32 tests automatisés

**Impact :** Contrôle total sur chaque ligne de code, aucune vulnérabilité de supply chain, compréhension complète du système.

---

### 🏗️ A3 — Documentation de Qualité Professionnelle

- Chaque fichier `.rs` commence par un bloc `//!` détaillé expliquant son rôle et son fonctionnement
- Le `README.md` est exhaustif (138 lignes) avec badges, schéma d'architecture ASCII, feuille de route décennale
- 3 documents de conception dédiés dans `docs/` :
  - `CONCEPTION_TECHNIQUE_ET_ARCHITECTURE.md` — Spécification technique complète
  - `FEUILLE_DE_ROUTE_ET_AMELIORATIONS.md` — Analyse de l'existant et améliorations
  - `OS_MANIFESTE_ET_OBJECTIFS_10_ANS.md` — Vision et objectifs long terme
- Commentaires inline précis sur chaque constante hardware (ports I/O, bits de registres, offsets mémoire)

---

### 🛡️ A4 — Sécurité Mémoire Garantie par le Compilateur Rust

En utilisant Rust `#![no_std]` au lieu de C/C++, AuraOS **élimine par construction** les classes de vulnérabilités suivantes :

| Vulnérabilité | C/C++ Kernel | AuraOS (Rust) |
|:---|:---:|:---:|
| Buffer overflow | Fréquent | **Impossible** (bounds checking) |
| Use-after-free | Fréquent | **Impossible** (ownership) |
| Double-free | Fréquent | **Impossible** (move semantics) |
| Null pointer deref | Fréquent | **Impossible** (Option\<T\>) |
| Data races | Fréquent | **Impossible** (Send/Sync) |
| Dangling pointers | Fréquent | **Impossible** (lifetimes) |

Selon les études Microsoft et Google, cela élimine environ **70% des CVE** qui affectent les systèmes d'exploitation en C/C++.

---

### 🛡️ A5 — Spinlock IRQ-Safe avec Pattern RAII

Le `Spinlock<T>` dans `sync.rs` est un modèle de conception exemplaire :

- **Sauvegarde automatique** de l'état des interruptions (RFLAGS.IF) avant `cli`
- **Restauration automatique** via `Drop` sur le `SpinlockGuard` en sortie de scope
- `try_lock()` non-bloquant avec restauration correcte en cas d'échec
- `force_unlock()` pour le panic handler (évite les deadlocks secondaires)
- Empêche les **deadlocks ISR** : un handler d'interruption ne peut pas interrompre un code tenant un lock

---

### 🛡️ A6 — Panic Handler Robuste et Informatif

Le gestionnaire de panic du kernel :

1. Désactive immédiatement les interruptions (`cli`) pour éviter toute corruption
2. Force le déverrouillage des périphériques de sortie (VGA + série)
3. Affiche un diagnostic complet sur **deux canaux** (écran VGA et port série COM1)
4. Entre dans une boucle HLT infinie pour un arrêt propre sans triple fault

---

### ⚙️ A7 — Multitâche Préemptif Fonctionnel

Le scheduler implémente un vrai système multitâche :

- **TCB (Task Control Blocks)** avec piles isolées de 16 KiB par tâche
- **Context switch en assembleur naked** (`switch_context`) : sauvegarde/restauration des 7 registres callee-saved + RFLAGS
- **Round-Robin avec quantum de temps** : 10 ticks PIT = 100 ms par tranche
- **Préemption matérielle** via PIT Timer IRQ0 quand le quantum expire
- **Sleep avec réveil automatique** : `sleep_ms()`, `sleep_ticks()` avec comptabilité par tick
- **Gestion d'états complète** : Ready → Running → Sleeping(wake_tick) → Dead

---

### ⚙️ A8 — Interface Syscall/Sysret x86_64 Native

Le mécanisme de syscall utilise la voie matérielle rapide du processeur :

- Configuration des 4 MSRs : EFER (SCE), STAR (segments), LSTAR (entry point), FMASK (IF mask)
- Trampoline naked avec switch de pile user → kernel
- 8 syscalls implémentés : `exit`, `write`, `getpid`, `yield`, `sleep`, `time`, `send`, `recv`
- Convention ABI compatible Linux (mêmes numéros de syscall et registres d'arguments)
- Compteur atomique de syscalls pour diagnostics

---

### ⚙️ A9 — Isolation Ring 0 / Ring 3 Complète

La chaîne complète de séparation des privilèges est implémentée :

- **GDT** avec segments User Code/Data (DPL=3) et Kernel Code/Data (DPL=0)
- **TSS** avec RSP0 pour les transitions automatiques Ring 3 → Ring 0
- **AddressSpace** avec PML4 dédiés par processus (isolation mémoire matérielle)
- `iretq` pour la transition initiale vers Ring 3
- `sysretq` pour le retour en Ring 3 après un syscall
- **CR3 switching** automatique lors des changements de contexte

---

### ⚙️ A10 — Pile Réseau Multi-Couche (5 protocoles)

Une pile réseau fonctionnelle en 836 lignes couvrant 5 protocoles :

| Couche | Protocole | Fonctionnalités |
|:---|:---|:---|
| Driver | Intel e1000 | Ring buffers TX/RX, MMIO, MAC address read |
| L2 | Ethernet | Frame parsing, EtherType dispatch |
| L2.5 | ARP | Request/Reply, table de cache MAC |
| L3 | IPv4 | Header build/parse, checksum, TTL |
| L3.5 | ICMP | Echo Request/Reply (ping) |
| L4 | UDP | Datagrammes, ports source/dest |

---

### ⚙️ A11 — VFS Riche avec /proc et /dev

Le système de fichiers n'est pas un simple stub mais un VFS complet :

- **Arborescence d'inodes** avec navigation `/`, `..`, `.`
- **`/proc/` dynamique** : `uptime`, `meminfo`, `cpuinfo`, `tasks` — générés à chaque lecture
- **`/dev/` virtuel** : `null`, `zero`, `random`, `urandom` — comportement POSIX
- **`/bin/` exécutables** : `hello`, `counter`, `init` — vrais ELF64
- **`/etc/` configuration** : `hostname`, `version`, `motd`
- **CRUD complet** : `touch`, `mkdir`, `write`, `rm`, `cat`, `ls`, `cd`, `pwd`

---

### ⚙️ A12 — Suite de 32 Tests Automatisés

1 275 lignes de tests couvrant **chaque sous-système** du kernel :

| # | Test | Sous-système |
|:---:|:---|:---|
| 1-2 | Paging 4-Level Indexing & Edge Cases | memory/paging |
| 3-5 | Heap Allocator, Stress (500 allocs), Fragmentation | memory/allocator |
| 6-7 | VFS CRUD & Directory Hierarchy | fs |
| 8-9 | CMOS RTC & CPUID Decoding | drivers, arch |
| 10-12 | Scheduler, Spinlock, Dynamic Strings | task, sync |
| 13-14 | Graphics Canvas & Edge Cases | gui |
| 15-18 | PCI, Disk MBR, Disk I/O, VFS Storage | drivers |
| 19-22 | Timekeeping, Preemption, Synchronization, Deadlock | task, arch |
| 23-24 | Shell Parsing & Environment | shell |
| 25-26 | Double-Buffer Canvas & Window Compositor | gui |
| 27-28 | ELF64 Parser & Ring 3 Execution | fs/elf |
| 29-30 | NIC Detection & Packet Serialization | net |
| 31-32 | ACPI Tables & MADT Core Enumeration | arch/acpi |

Exécutés automatiquement à chaque boot avec rapport PASS/FAIL sur écran et port série.

---

### ⚙️ A13 — Shell Interactif Complet (25+ commandes)

| Catégorie | Commandes |
|:---|:---|
| **Fichiers** | `ls`, `cd`, `pwd`, `cat`, `touch`, `mkdir`, `write`, `rm` |
| **Système** | `help`, `info`, `cpu`, `pci`, `tasks`, `mem`, `time`, `date`, `ticks` |
| **Contrôle** | `reboot`, `shutdown`, `halt`, `clear`, `yield` |
| **Réseau** | `ping`, `arp` |
| **Utilitaires** | `calc`, `serial`, `readsec`, `manifesto` |

---

### ⚙️ A14 — ACPI & Power Management Complet

Le parseur ACPI (503 lignes) implémente :

- Scan RSDP dans EBDA (0x80000-0x9FC00) et BIOS ROM (0xE0000-0xFFFFF)
- Parsing RSDT (32-bit) et XSDT (64-bit) avec validation de checksum
- FADT : registres PM1a/PM1b pour soft power-off via ACPI S5
- DSDT AML : extraction du package `_S5` (SLP_TYPa, SLP_TYPb)
- MADT : énumération des cores CPU (Local APIC) et I/O APICs
- Local APIC : initialisation MMIO, SVR, TPR, EOI

---

### ⚙️ A15 — Binaire Extrêmement Léger

| Métrique | AuraOS | Linux minimal | Windows |
|:---|:---:|:---:|:---:|
| Taille kernel (release) | **275 Ko** | ~5 Mo | ~30 Mo |
| Image bootable | **~149 Ko** | ~50 Mo | ~5 Go |
| Dépendances Cargo | **1** | N/A | N/A |
| Runtime externe (libc, libstd) | **0** | glibc | ntdll |

---

---

## ❌ INCONVÉNIENTS (25)

---

### 🔴 Bugs Confirmés — Priorité Critique

---

#### I1 — `/proc/uptime` Calcule avec la Mauvaise Fréquence PIT

**Fichier :** `src/fs/mod.rs` (lignes 428-431)

```rust
// Code actuel (INCORRECT) :
let seconds = ticks / 18;       // ← Utilise 18.2 Hz (fréquence BIOS par défaut)
let ms = (ticks % 18) * 55;

// Code correct :
let seconds = ticks / 100;      // ← Le PIT est reconfiguré à 100 Hz dans pit.rs
let ms = (ticks % 100) * 10;
```

**Impact :** Les temps affichés par `cat /proc/uptime` sont **5.5× trop lents** par rapport à la réalité. L'utilisateur voit "10 secondes" alors qu'il s'est écoulé ~1.8 seconde.

**Effort de correction :** 2 minutes.

---

#### I2 — Lecture du Numéro de Syscall Non Fiable

**Fichier :** `src/arch/syscall.rs` (lignes 181-184)

```rust
let syscall_nr: u64;
unsafe { core::arch::asm!("", out("rax") syscall_nr, options(nomem, nostack)); }
```

Après l'instruction `call {handler}` dans le trampoline `syscall_entry`, le compilateur peut avoir écrasé RAX dans le prologue de `syscall_dispatch`. Ce code fonctionne **par chance** en mode release grâce aux optimisations (inlining), mais pourrait casser silencieusement si :

- On compile en mode debug
- LLVM change son allocateur de registres
- On ajoute du code instrumenté (sanitizers, profiling)

**Impact :** En cas de casse, tous les syscalls seraient dispatchés au mauvais handler, causant un comportement indéfini complet en espace utilisateur.

**Solution recommandée :** Passer le numéro de syscall comme argument explicite via la pile kernel ou un registre callee-saved sauvegardé dans le trampoline naked.

**Effort de correction :** 30 minutes.

---

#### I3 — `static mut` dans le Chemin Critique Syscall

**Fichier :** `src/arch/syscall.rs` (lignes 103-104)

```rust
pub static mut USER_RSP_SCRATCH: u64 = 0;
pub static mut KERNEL_RSP_SCRATCH: u64 = 0;
```

`static mut` est considéré unsafe-heavy et potentiellement UB (Undefined Behavior) sous Rust 2024 si accédé de manière concurrente. Actuellement sûr car single-core avec interruptions désactivées (FMASK masque IF), mais c'est une **bombe à retardement** pour le support multi-cœur futur.

**Solution :** Migrer vers des `AtomicU64` avec `Ordering::Relaxed`, ou utiliser un `UnsafeCell` dans un wrapper `Sync`.

**Effort de correction :** 15 minutes.

---

### 🔴 Limitations Architecturales Majeures

---

#### I4 — Single-Core Uniquement — Aucun Support SMP

Le kernel détecte les cores CPU via MADT/ACPI mais **ne les utilise jamais**. Tout s'exécute sur le BSP (Bootstrap Processor). Les AP (Application Processors) restent endormis. L'ajout de SMP nécessiterait :

- Envoi de Startup IPI (SIPI) pour réveiller chaque AP
- Per-CPU data structures (pile, IDT, TSS par cœur)
- Scheduler per-CPU avec work stealing ou load balancing
- Verrous `lock`-préfixés (déjà OK grâce aux `AtomicBool`)

**Impact :** Impossible d'utiliser plus d'un cœur, même sur un processeur 8 cœurs. Les performances sont plafonnées à ~50% d'un dual-core et ~12.5% d'un octa-core.

**Effort de correction :** 1 mois de développement.

---

#### I5 — Pas de Vrai Gestionnaire de Mémoire Physique (PMM)

Le kernel utilise un **buffer statique de 8 MiB** (`static HEAP_STORAGE`) comme seule source de mémoire dynamique. Il n'y a aucune détection de la RAM physique disponible (pas de E820 / multiboot memory map).

**Conséquences :**

- Impossible d'utiliser plus de 8 MiB de RAM, même si la machine en a 16 GiB
- Pas d'allocateur de frames physiques (physical page frame allocator)
- Les pages utilisateur sont allouées **depuis le heap kernel** — mélange dangereux
- Pas de reclaim de mémoire quand un processus se termine

**Effort de correction :** 1 semaine.

---

#### I6 — Pas de Mémoire Virtuelle Dynamique

Le sous-système de paging fournit des abstractions pour les tables de pages mais ne supporte pas :

- `mmap()` / `munmap()` — allocation dynamique d'espaces virtuels
- Demand paging — allocation de frames physiques au moment du page fault
- Swap to disk — pagination de mémoire rarement utilisée vers le disque
- Copy-on-Write (COW) — optimisation critique pour `fork()`
- Guard pages — détection de stack overflow

Les mappings hardware sont faits par écriture directe dans les tables du bootloader, ce qui est fragile et non extensible.

**Effort de correction :** 2 semaines.

---

#### I7 — Pas de Véritable Modèle de Processus

Le scheduler gère des threads kernel et des tâches user-space pré-construites, mais il manque :

- `fork()` — duplication d'un processus
- `exec()` / `execve()` — remplacement de l'image d'un processus
- `waitpid()` — attente de terminaison d'un enfant
- Table de processus avec PID management et recyclage
- Hiérarchie parent-enfant
- Signaux POSIX (SIGTERM, SIGKILL, SIGCHLD, SIGSEGV)
- Exit status et code de retour

**Conséquence :** Impossible de lancer un programme depuis le shell autrement qu'en le pré-enregistrant dans le code du kernel.

**Effort de correction :** 2-3 semaines.

---

#### I8 — Bootloader Legacy — Pas de Support UEFI

`bootloader 0.9.x` utilise le **BIOS legacy boot** uniquement :

- Pas de support UEFI Secure Boot
- Pas de GOP Framebuffer natif (nécessaire pour le mode graphique haute résolution)
- Ne fonctionne pas sur les machines récentes qui n'ont plus de BIOS CSM (Compatibility Support Module)
- L'image disque est limitée au format MBR (pas de GPT)
- Pas d'accès aux services UEFI (runtime services, variables NVRAM)

**Effort de correction :** 1 semaine (migration vers `bootloader 0.11+` ou `limine`).

---

### 🟡 Limitations Fonctionnelles

---

#### I9 — Système de Fichiers Volatil (RAMFS uniquement)

Le VFS est un **RAMFS** : toutes les données sont stockées en mémoire vive et **perdues à chaque reboot**. Le parser FAT32 (883 lignes) et le driver ATA/IDE existent dans le code mais ne sont **pas connectés au VFS** comme backend de stockage persistant.

Un utilisateur qui crée des fichiers avec `touch` ou `write` les perdra au redémarrage.

**Effort de correction :** 1-2 semaines (connecter FAT32 + ATA au VFS comme backend).

---

#### I10 — Pilote de Stockage ATA/IDE en PIO — Pas de DMA

Le driver ATA utilise le mode **PIO 28-bit LBA** (Programmed I/O) :

- Le CPU est **bloqué** pendant chaque transfert de secteur (polling actif)
- Pas de DMA (Direct Memory Access) → pas de transferts zero-copy
- Performances disque catastrophiques : ~2-5 Mo/s vs ~100+ Mo/s en DMA
- Pas de support AHCI (interface SATA moderne)
- Pas de NVMe
- Limité aux disques de moins de **128 GiB** (28-bit LBA = 2²⁸ secteurs × 512 octets)

**Effort de correction :** 2 semaines (AHCI) à 1 mois (NVMe).

---

#### I11 — Pile Réseau Non Fonctionnelle en Production

La pile réseau implémente les protocoles en théorie mais présente des limitations pratiques majeures :

- Adresses IP et MAC **hardcodées** dans le code (pas de DHCP)
- **Pas de TCP** → impossible de faire du HTTP, SSH, FTP, ou tout protocole fiable
- Pas de DNS → impossible de résoudre des noms de domaine
- Pas de socket API pour les applications utilisateur
- Le driver e1000 est **spécifique à QEMU/VirtualBox** (ne fonctionne pas sur du vrai matériel Intel récent)
- Pas de gestion de la fragmentation IP
- Pas de ARP cache timeout

**Effort de correction :** TCP = 2-3 semaines, DHCP = 1 semaine, DNS = 3 jours.

---

#### I12 — GUI : Canvas 2D Basique Sans Compositeur

Le module GUI offre un canvas 2D avec alpha blending mais :

- **Pas de compositeur de fenêtres** (window manager / compositor)
- **Pas de connexion souris → GUI** : la souris est détectée (IRQ12) mais ses événements ne sont pas routés vers le canvas
- Pas de widgets (boutons, champs texte, barres de défilement, menus)
- **Mode graphique non activé au boot** : le kernel démarre en VGA texte 80×25
- Le driver BGA/framebuffer existe mais n'est **jamais appelé** dans la séquence de boot
- Pas de curseur souris visible
- Pas de rendu de polices TrueType/bitmap au-delà du mode texte

**Effort de correction :** Compositeur basique = 2 semaines, widgets = 1 mois.

---

#### I13 — Shell Monolithique et Non Extensible

Le fichier `shell/mod.rs` fait **1 174 lignes** dans un seul fichier avec un `match` géant :

- Impossible d'ajouter des commandes modulairement (plugin system)
- Pas de pipes (`cmd1 | cmd2`)
- Pas de redirection d'I/O (`>`, `>>`, `<`)
- Pas de variables d'environnement (`$PATH`, `$HOME`, `$USER`)
- Pas d'historique des commandes (flèche ↑/↓)
- Pas de complétion par tabulation (TAB)
- Pas de scripting (boucles, conditions, fonctions)
- Pas de gestion des guillemets et échappement (`"hello world"`, `\n`)

**Effort de correction :** Refactoring = 2 heures, pipes + redirection = 1 semaine.

---

#### I14 — Loader ELF Limité aux Programmes Auto-Générés

Le loader ELF peut parser les headers ELF64 et construire des mini-programmes en machine code x86_64 brut, mais :

- Ne supporte que des programmes **auto-générés en assembleur inline**
- Pas de support des bibliothèques partagées (`.so`)
- Pas de relocations dynamiques (PLT/GOT)
- Pas de linking ELF standard
- **Impossible de charger un vrai exécutable** compilé par `gcc`, `clang`, ou `rustc`
- Pas de support des sections `.bss` (données non initialisées)

**Effort de correction :** Support ELF statique complet = 1-2 semaines.

---

#### I15 — IPC : Recherche Linéaire O(n)

L'IPC Router cherche les mailboxes par scan linéaire dans un `Vec<(usize, VecDeque)>`. Avec N processus actifs, chaque opération `send()` ou `receive()` est **O(N)**.

Avec 100 processus, chaque message traverse 100 entrées dans le pire cas. Un `BTreeMap<usize, VecDeque>` (disponible via `alloc`) réduirait cela à **O(log N)**.

**Effort de correction :** 30 minutes.

---

### 🟡 Dette Technique

---

#### I16 — Code Dupliqué : `rdmsr`/`wrmsr` Définis Deux Fois

Les fonctions `rdmsr()` et `wrmsr()` sont implémentées de manière **identique** dans deux fichiers :

- `src/arch/syscall.rs` (lignes 71-100)
- `src/arch/apic.rs` (lignes 48-77)

Cela viole le principe DRY (Don't Repeat Yourself). Toute correction dans l'une doit être manuellement répliquée dans l'autre.

**Solution :** Déplacer dans `arch/io.rs` ou créer un nouveau `arch/msr.rs`.

**Effort de correction :** 15 minutes.

---

#### I17 — VFS : Fuite de Slots d'Inodes

`remove_entry()` dans `fs/mod.rs` vide le contenu des inodes supprimés mais **ne les retire pas** du `Vec<Inode>`. Les slots "morts" (nom vidé, contenu vidé) s'accumulent indéfiniment dans le vecteur :

- Les IDs d'inodes ne sont jamais réutilisés
- La mémoire des slots morts n'est jamais reclaimed
- Le `Vec` grossit monotoniquement avec chaque cycle create/delete

**Solution :** Maintenir une free-list de slots libres ou marquer les inodes comme "tombstoned" avec réutilisation.

**Effort de correction :** 1 heure.

---

#### I18 — `/proc/meminfo` Affiche des Valeurs Statiques

```rust
"MemTotal:        8192 kB\n\
 MemFree:         4096 kB\n"  // ← Hardcodé ! Jamais mis à jour
```

Les fonctions `memory::allocator::free_memory()` et `memory::allocator::used_memory()` **existent déjà** mais ne sont pas utilisées par `/proc/meminfo`. L'utilisateur voit toujours "4096 kB free" quel que soit l'état réel du heap.

**Effort de correction :** 5 minutes.

---

#### I19 — `/dev/random` : PRNG Cryptographiquement Faible

Le générateur de nombres aléatoires utilise un simple XorShift basé sur `RDTSC` :

```rust
seed ^= seed << 13;
seed ^= seed >> 7;
seed ^= seed << 17;
```

Le pattern est **entièrement prédictible** pour un attaquant connaissant la valeur initiale du TSC. Le CPUID détecte déjà si l'instruction `RDRAND` est disponible sur le processeur — elle devrait être utilisée quand présente pour un PRNG de bien meilleure qualité.

**Effort de correction :** 30 minutes.

---

#### I20 — Tâches Mortes Jamais Nettoyées du Scheduler

Dans le scheduler, les tâches avec `state == Dead` restent dans le `Vec<Task>` pour toujours :

- Leur pile de **16 KiB n'est jamais libérée**
- Les TCBs morts sont parcourus à chaque `pick_next()` (overhead O(n))
- Après beaucoup de cycles spawn → terminate, le heap se remplit de piles zombies

**Solution :** Ajouter une phase de reaping qui retire et `drop()` les tâches Dead.

**Effort de correction :** 1 heure.

---

#### I21 — Pas de Gestion d'Erreur Structurée

Le kernel utilise des `&'static str` pour les erreurs :

```rust
fn resolve_path(&self, path: &str) -> Result<usize, &'static str>
```

Au lieu d'un `enum KernelError` typé :

```rust
enum FsError { NotFound, NotADirectory, PermissionDenied, DiskFull, ... }
```

**Conséquences :**
- Impossible de faire du pattern matching sur les erreurs
- Pas de propagation via `?` avec conversion automatique
- Messages d'erreur non localisables
- Pas de codes d'erreur numériques pour les syscalls

**Effort de correction :** 3 heures.

---

### 🟢 Limitations Mineures

---

#### I22 — Pas de Logging Structuré

Les messages de debug sont des `serial_println!()` ad-hoc :

```rust
serial_println!("[OK] PIC remapped.");
serial_println!("[ACPI] Warning: RSDP not found.");
```

Il n'y a pas de niveaux de log (DEBUG, INFO, WARN, ERROR), pas de filtrage par sous-système, et pas de timestamp. Impossible de filtrer ou rediriger les logs.

**Effort de correction :** 1 heure (macro `klog!(level, subsystem, message)`).

---

#### I23 — Script QEMU Basique

Le `run_qemu.sh` ne configure pas :

- La quantité de RAM : utilise le défaut QEMU (~128 Mo), pas optimal
- Le réseau : pas de `-netdev user,id=net0 -device e1000,netdev=net0`
- Le debugging GDB : pas de `-s -S` pour attacher un débogueur
- Le test exit device : pas de `-device isa-debug-exit,iobase=0xf4` pour les tests CI
- Le nombre de CPU : pas de `-smp 2` pour tester SMP futur

**Effort de correction :** 15 minutes.

---

#### I24 — Pas d'Intégration Continue (CI/CD)

Aucun fichier `.github/workflows/` ou équivalent. Le projet ne compile pas automatiquement sur push/PR. Risque de régressions silencieuses quand plusieurs contributeurs travaillent sur des branches différentes.

Un workflow minimal (`cargo +nightly build --release` + `cargo +nightly clippy`) prendrait 15 minutes à mettre en place.

**Effort de correction :** 1 heure.

---

#### I25 — Fichier Target JSON Orphelin

Le fichier `x86_64-aura.json` existe à la racine du projet mais le `.cargo/config.toml` utilise le target built-in `x86_64-unknown-none`. Le fichier JSON custom n'est **référencé nulle part** et ne sert à rien — source de confusion pour les contributeurs.

**Solution :** Soit le supprimer, soit mettre à jour `.cargo/config.toml` pour l'utiliser.

**Effort de correction :** 1 minute.

---

---

## 📊 Tableau Récapitulatif

| # | Type | Sévérité | Description | Effort |
|:---:|:---:|:---:|:---|:---:|
| A1-A3 | ✅ | — | Architecture, indépendance, documentation | — |
| A4-A6 | ✅ | — | Sécurité mémoire, spinlock, panic handler | — |
| A7-A9 | ✅ | — | Multitâche, syscall, Ring 0/3 | — |
| A10-A12 | ✅ | — | Réseau, VFS, 32 tests | — |
| A13-A15 | ✅ | — | Shell, ACPI, binaire léger | — |
| I1 | ❌ | 🔴 Bug | `/proc/uptime` fréquence PIT | 2 min |
| I2 | ❌ | 🔴 Bug | Syscall RAX non fiable | 30 min |
| I3 | ❌ | 🔴 Bug | `static mut` dangereux | 15 min |
| I4 | ❌ | 🔴 Archi | Pas de SMP | 1 mois |
| I5 | ❌ | 🔴 Archi | Pas de PMM (8 MiB fixe) | 1 sem |
| I6 | ❌ | 🔴 Archi | Pas de VM dynamique | 2 sem |
| I7 | ❌ | 🔴 Archi | Pas de fork/exec/signals | 2-3 sem |
| I8 | ❌ | 🔴 Archi | Bootloader BIOS-only | 1 sem |
| I9 | ❌ | 🟡 Fonc | RAMFS volatil | 1-2 sem |
| I10 | ❌ | 🟡 Fonc | ATA PIO sans DMA | 2 sem |
| I11 | ❌ | 🟡 Fonc | Réseau sans TCP/DHCP | 3 sem |
| I12 | ❌ | 🟡 Fonc | GUI sans compositeur | 2 sem |
| I13 | ❌ | 🟡 Fonc | Shell monolithique | 2h-1 sem |
| I14 | ❌ | 🟡 Fonc | ELF loader limité | 1-2 sem |
| I15 | ❌ | 🟡 Fonc | IPC O(n) | 30 min |
| I16 | ❌ | 🟡 Dette | rdmsr/wrmsr dupliqué | 15 min |
| I17 | ❌ | 🟡 Dette | Fuite slots VFS | 1h |
| I18 | ❌ | 🟡 Dette | `/proc/meminfo` hardcodé | 5 min |
| I19 | ❌ | 🟡 Dette | PRNG prévisible | 30 min |
| I20 | ❌ | 🟡 Dette | Tâches mortes non nettoyées | 1h |
| I21 | ❌ | 🟡 Dette | Pas d'enum d'erreurs | 3h |
| I22 | ❌ | 🟢 Min | Pas de logging structuré | 1h |
| I23 | ❌ | 🟢 Min | QEMU script basique | 15 min |
| I24 | ❌ | 🟢 Min | Pas de CI/CD | 1h |
| I25 | ❌ | 🟢 Min | Target JSON orphelin | 1 min |

---

## 🎯 Plan d'Action Prioritaire

### Phase 0 — Corrections Immédiates (< 1 heure)
1. Corriger `/proc/uptime` (fréquence 100 Hz)
2. Corriger `/proc/meminfo` (utiliser `allocator::free_memory()`)
3. Supprimer ou utiliser `x86_64-aura.json`
4. Factoriser `rdmsr`/`wrmsr` dans un module commun

### Phase 1 — Stabilisation (1-2 semaines)
5. Sécuriser la lecture du numéro de syscall
6. Remplacer `static mut` par `AtomicU64`
7. Implémenter le reaping des tâches mortes
8. Ajouter CI GitHub Actions
9. Free-list pour les inodes VFS
10. Améliorer `run_qemu.sh`

### Phase 2 — Fondations (1-2 mois, aligné avec Feuille de Route Année 1-2)
11. Physical Memory Manager (E820 + frame allocator)
12. Connecter FAT32 + ATA au VFS (persistance)
13. TCP dans la pile réseau
14. Migration bootloader vers 0.11+ (UEFI)
15. Découper le shell en sous-modules

### Phase 3 — Maturité (3-6 mois, aligné avec Feuille de Route Année 3-4)
16. fork/exec/waitpid + signaux
17. Demand paging + mmap
18. SMP multi-cœur
19. AHCI/NVMe drivers
20. Compositeur de fenêtres GUI
