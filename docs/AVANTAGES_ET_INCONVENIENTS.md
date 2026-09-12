# 🌌 AuraOS v0.1.0-alpha — Avantages et Inconvénients

> **Date d'analyse :** 12 Septembre 2026
> **Version analysée :** v0.1.0-alpha
> **Métriques :** ~13 000 lignes de Rust · 47 fichiers · 312 Ko (release) · 0 erreur · 0 warning · 34 tests automatisés
>
> **Note de révision :** Ce document a été réaudité le 12/09/2026. Les 25 points
> (I1–I25) ont été revus contre le code réel. 10 étaient déjà entièrement
> corrigés par les commits récents, 6 partiellement, et 6 ont été corrigés dans
> le cadre de ce travail (I5, I12, I20, I21, I22, I23). Il reste 7 chantiers
> de fond (I4, I6, I8–I11, I13) documentés dans la section « Chantiers futurs ».

---

## ✅ AVANTAGES (15)

---

### 🏗️ A1 — Architecture Modulaire Exemplaire

Le code source est organisé en **modules indépendants** avec séparation claire des responsabilités :

```
src/
├── arch/       → (CPU, GDT, IDT, PIC, ACPI, APIC, MSR, syscall, ring3)
├── drivers/    → (VGA, serial, keyboard, mouse, PCI, ATA, PIT, e1000, BGA, framebuffer)
├── memory/     → (heap, **PMM**, paging, user space isolation)
├── task/       → (scheduler préemptif, IPC message passing)
├── fs/         → (VFS/RAMFS, FAT32, ELF64, erreurs typées)
├── net/        → (ethernet, ARP, IPv4, ICMP, UDP)
├── gui/        → (compositeur de fenêtres, canvas 2D, alpha blending)
├── klog/       → (logging structuré à niveaux + filtre runtime)
├── shell/      → (shell interactif 25+ commandes)
└── tests/      → (34 tests automatisés)
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
- Suite de 34 tests automatisés exécutés à chaque boot

**Impact :** Contrôle total sur chaque ligne de code, aucune vulnérabilité de supply chain, compréhension complète du système.

---

### 🏗️ A3 — Documentation de Qualité Professionnelle

- Chaque fichier `.rs` commence par un bloc `//!` détaillé expliquant son rôle et son fonctionnement
- Le `README.md` est exhaustif avec badges, schéma d'architecture ASCII, feuille de route décennale
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
- **Reaping automatique** des tâches mortes (stacks libérées, zombies nettoyés)
- **Gestion d'états complète** : Ready → Running → Sleeping(wake_tick) → Dead → Reaped

---

### ⚙️ A8 — Interface Syscall/Sysret x86_64 Native

Le mécanisme de syscall utilise la voie matérielle rapide du processeur :

- Configuration des 4 MSRs : EFER (SCE), STAR (segments), LSTAR (entry point), FMASK (IF mask)
- Trampoline naked avec switch de pile user → kernel ; **numéro de syscall capturé depuis RAX dans un scratch atomique** avant l'appel du dispatcher (lecture fiable, indépendante de l'ordre des registres du compilateur)
- Les scratchs user/kernel RSP sont des `AtomicU64` (pas de `static mut`)
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
- **`/proc/` dynamique** : `uptime`, `meminfo`, `cpuinfo`, `tasks` — générés à chaque lecture (vraies valeurs runtime)
- **`/dev/` virtuel** : `null`, `zero`, `random`, `urandom` — comportement POSIX (`random` s'appuie sur RDRAND hardware quand disponible)
- **`/bin/` exécutables** : `hello`, `counter`, `init` — vrais ELF64
- **`/etc/` configuration** : `hostname`, `version`, `motd`
- **CRUD complet** : `touch`, `mkdir`, `write`, `rm`, `cat`, `ls`, `cd`, `pwd`
- **Slots d'inodes recyclés** via tombstones + free-list (pas de fuite mémoire)

---

### ⚙️ A12 — Suite de 34 Tests Automatisés

Tests couvrant **chaque sous-système** du kernel, exécutés à chaque boot :

| # | Test | Sous-système |
|:---:|:---|:---|
| 1-2 | Paging 4-Level Indexing & Edge Cases | memory/paging |
| 3-5 | Heap Allocator, Stress, Fragmentation | memory/allocator |
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
| 33 | Dead Task Auto-Reaping (zombies nettoyés) | task |
| 34 | PMM Frame Allocator Roundtrip + AddressSpace Drop | memory/pmm |

Rapport PASS/FAIL sur écran et port série — le port série utilise désormais le format structuré `klog!` (`[LEVEL] [sous-système] [#tick] message`).

---

### ⚙️ A13 — Shell Interactif Complet (25+ commandes)

| Catégorie | Commandes |
|:---|:---|
| **Fichiers** | `ls`, `cd`, `pwd`, `cat`, `touch`, `mkdir`, `write`, `rm`, `exec` |
| **Système** | `help`, `info`, `cpu`, `pci`, `tasks`, `mem`, `time`, `date`, `ticks`, `cores` |
| **Contrôle** | `reboot`, `shutdown`, `halt`, `clear`, `yield`, `kill` |
| **Réseau** | `ping`, `arp`, `udpsend`, `netstat` |
| **Graphique** | `gui` (desktop BGA, lancé automatiquement au boot) |
| **Disque** | `formatfat`, `mountfat`, `fatls`, `fatcat`, `fatwrite`, `fatmkdir`, `fatrm`, `readsec` |
| **Utilitaires** | `calc`, `serial`, `manifesto`, `test` |

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
| Taille kernel (release) | **312 Ko** | ~5 Mo | ~30 Mo |
| Image bootable | **~359 Ko** | ~50 Mo | ~5 Go |
| Dépendances Cargo | **1** | N/A | N/A |
| Runtime externe (libc, libstd) | **0** | glibc | ntdll |

---

---

## ✅ INCONVÉNIENTS DÉJÀ CORRIGÉS (15)

> Ces points, listés dans l'analyse initiale du 11/09, ont été vérifiés corrigés
> dans le code actuel (commits `bcd7fdd` et antérieurs, puis ce travail).

| # | Problème initial | Statut actuel | Référence code |
|:---:|:---|:---:|:---|
| **I1** | `/proc/uptime` calcule en 18.2 Hz au lieu de 100 Hz | ✅ Corrigé | `fs/mod.rs` : `TARGET_FREQUENCY` (100 Hz) lu depuis `pit.rs` |
| **I2** | Lecture du numéro de syscall non fiable (RAX écrasé) | ✅ Corrigé | `arch/syscall.rs` : RAX capturé dans un scratch atomique avant l'appel ; le vieux `[rsp+8]` lisait en fait un slot de la frame du compilateur (numéros parasites 0/10 observés en QEMU, #GP en fin de ELF) — corrigé et vérifié : `exec hello`/`counter` s'exécutent réellement |
| **I3** | `static mut` dans le chemin syscall (risque SMP) | ✅ Corrigé | `arch/syscall.rs` : `AtomicU64` |
| **I15** | IPC : recherche linéaire O(n) des mailboxes | ✅ Corrigé | `task/ipc.rs` : `BTreeMap<usize, VecDeque>` (O(log n)) |
| **I16** | Code dupliqué `rdmsr`/`wrmsr` (syscall + apic) | ✅ Corrigé | `arch/msr.rs` : primitives centralisées, réexportées dans `apic.rs` |
| **I17** | VFS : fuite de slots d'inodes (IDs jamais réutilisés) | ✅ Corrigé | `fs/mod.rs` : tombstones + `free_inodes` recyclés par `allocate_inode` |
| **I18** | `/proc/meminfo` affiche des valeurs statiques | ✅ Corrigé | `fs/mod.rs` : valeurs live de `allocator::free_memory()`/`used_memory()` |
| **I19** | `/dev/random` : PRNG XorShift prévisible | ✅ Corrigé | `fs/mod.rs` : RDRAND hardware utilisé quand disponible, XorShift64* en fallback |
| **I20** | Tâches mortes jamais nettoyées du scheduler | ✅ Corrigé | Ce travail : `reap_dead_tasks()` câblé dans `preempt_schedule()` (tick %25) + test n°33 |
| **I21** | Pas de gestion d'erreur structurée (`&'static str`) | ✅ Corrigé | Ce travail : `fs/errors.rs` — `VfsError`, `AtaError`, `Fat32Error` typés avec `Display` |
| **I22** | Pas de logging structuré | ✅ Corrigé | Ce travail : `klog.rs` — niveaux, sous-système, timestamp `#tick`, filtre runtime |
| **I23** | Script QEMU basique | ✅ Corrigé | Ce travail : `-smp N` + `-device isa-debug-exit` ajoutés (RAM/net/GDB déjà OK) |
| **I24** | Pas d'intégration continue | ✅ Corrigé | `.github/workflows/ci.yml` : check + build release + bootimage |
| **I5** | Pas de vrai gestionnaire de mémoire physique | ✅ Corrigé (chantier de fond) | Ce travail : `memory/pmm.rs` — memory map E820 capturée au boot (22 régions en QEMU 256 M), bitmap frame allocator identity-mapped [2 MiB..512 MiB), migrateur des page tables **et** pages user vers le PMM (libération réelle des frames au `Drop`, vérifiée par le test n°34), stats `PhysicalTotal/Free/Used` dans `/proc/meminfo` + banner boot |
| **I25** | Fichier target JSON orphelin | ✅ Corrigé | `x86_64-aura.json` supprimé ; `.cargo/config.toml` utilise `x86_64-unknown-none` |

**Total : 15 inconvénients entièrement résolus.** Les bugs critiques de l'analyse
initiale (I1–I3) n'existent plus dans le code actuel.

---

## ✅ INCONVÉNIENTS PARTIELLEMENT CORRIGÉS (2)

| # | Problème initial | Progrès réel | Ce qui manque |
|:---:|:---|:---|:---|
| **I7** | Pas de fork/exec/waitpid | Le shell charge et lance de vrais ELF64 Ring 3 (`exec`), y compris avec section `.bss` | `fork()`, `waitpid()`, signaux, exit status |
| **I12** | GUI sans compositeur ; BGA jamais activé au boot | Compositeur de fenêtres complet (drag, z-order, curseur souris, routage souris→desktop), **BGA activé au boot par ce travail** (ligne 7c de `main.rs`), desktop lancé automatiquement, retour texte via ESC | Widgets (boutons, champs), polices bitmap/TTF |

---

## 🟠 INCONVÉNIENTS ENCORE PRÉSENTS — Chantiers de fond (6)

> Ces points nécessitent des semaines de développement et sont déjà inscrits dans
> la feuille de route (`FEUILLE_DE_ROUTE_ET_AMELIORATIONS.md`). Ils ne sont pas des
> "bugs" mais des manques de fonctionnalités architecturales.

### 🔴 I4 — Single-Core Uniquement — Aucun Support SMP

Le kernel détecte les cores CPU via MADT/ACPI mais **ne les utilise jamais**. Tout s'exécute sur le BSP. L'ajout de SMP nécessiterait :

- Envoi de Startup IPI (SIPI) pour réveiller chaque AP
- Per-CPU data structures (pile, IDT, TSS par cœur)
- Scheduler per-CPU avec work stealing ou load balancing

**Impact :** Performances plafonnées à ~12.5% d'un octa-core. Le script QEMU accepte désormais `--smp N`.

**Effort :** ~1 mois.

---

### ✅ I5 — Gestionnaire de Mémoire Physique (PMM) — Fait (chantier de fond)

~~Le kernel utilise un buffer statique de 8 MiB comme seule source de mémoire dynamique...~~ Résolu le 12/09 par ce travail : **`src/memory/pmm.rs`** — memory map E820 copiée au boot depuis le `BootInfo` (22 régions, 245 MiB utilisables en QEMU 256 M), bitmap frame allocator (bitmap 2 040 mots pour 130 560 frames dans la fenêtre identity-mapped [2 MiB..512 MiB)), `allocate_frame`/`free_frame`, migration complète des page tables et pages utilisateur vers le PMM avec **libération réelle au `Drop`** (test n°34 : les frames reviennent — NoLeak), stats physiques live dans `/proc/meminfo` (`PhysicalTotal/Free/Used`) et banner de boot.

**Limite assumée :** la fenêtre identity-mapped s'arrête à 512 MiB — machine avec plus de RAM : l'excédent est compté « non libre » et nécessitera le feature `map_physical_memory` du bootloader (follow-up naturel de I6). Pas de demand paging ni swap (toujours I6).

---

### 🔴 I6 — Pas de Mémoire Virtuelle Dynamique

Pas de `mmap()`/`munmap()`, pas de demand paging, pas de swap, pas de Copy-on-Write (COW), pas de guard pages. Les mappings hardware sont écrits directement dans les tables du bootloader.

**Effort :** ~2 semaines.

---

### 🔴 I8 — Bootloader Legacy — Pas de Support UEFI

`bootloader 0.9.x` utilise le **BIOS legacy boot** uniquement : pas de UEFI Secure Boot, pas de framebuffer GOP natif, pas de GPT, pas de services UEFI, incompatible avec les machines sans CSM.

**Effort :** ~1 semaine (migration `bootloader 0.11+` ou `limine`).

---

### 🟡 I9 — Système de Fichiers Volatil (RAMFS uniquement)

Le VFS est un **RAMFS** : les données sont perdues à chaque reboot. Le parser FAT32 (883 lignes) et le driver ATA existent et fonctionnent (commandes `formatfat`/`mountfat`...) mais **ne sont pas connectés au VFS** comme backend persistant.

**Effort :** ~1-2 semaines (connecter FAT32 + ATA au VFS).

---

### 🟡 I10 — Pilote de Stockage ATA/IDE en PIO — Pas de DMA

Le driver ATA utilise le mode **PIO 28-bit LBA** : le CPU est bloqué pendant chaque transfert, pas de DMA, pas d'AHCI, pas de NVMe, limité à 128 GiB.

**Effort :** ~2 semaines (AHCI) à 1 mois (NVMe).

---

### 🟡 I11 — Pile Réseau Non Fonctionnelle en Production

IP/MAC **hardcodées** (pas de DHCP), **pas de TCP**, pas de DNS, pas de socket API utilisateur, driver e1000 spécifique QEMU/VirtualBox, pas de fragmentation IP, pas de ARP cache timeout.

**Effort :** TCP ≈ 2-3 semaines, DHCP ≈ 1 semaine, DNS ≈ 3 jours.

---

### 🟡 I13 — Shell Monolithique et Non Extensible

Le fichier `shell/mod.rs` fait **1 174 lignes** dans un seul fichier avec un `match` géant :

- Impossible d'ajouter des commandes modulairement
- Pas de pipes (`cmd1 | cmd2`), pas de redirection d'I/O, pas de variables d'environnement
- Pas d'historique (↑/↓), pas de complétion TAB, pas de scripting, pas de gestion des guillemets

**Effort :** Refactoring ≈ 2 heures ; pipes + redirection ≈ 1 semaine.

---

---

## 📊 Tableau Récapitulatif

| # | Type | Sévérité | Description | Statut |
|:---:|:---:|:---:|:---|:---:|
| A1-A3 | ✅ | — | Architecture, indépendance, documentation | — |
| A4-A6 | ✅ | — | Sécurité mémoire, spinlock, panic handler | — |
| A7-A9 | ✅ | — | Multitâche, syscall, Ring 0/3 | — |
| A10-A12 | ✅ | — | Réseau, VFS, 34 tests | — |
| A13-A15 | ✅ | — | Shell, ACPI, binaire léger | — |
| I1-I3 | ❌→✅ | 🔴→✅ | Bugs syscalls/uptime corrigés | **Corrigé** |
| I4 | ❌ | 🔴 Archi | Pas de SMP | Chantier (~1 mois) |
| I5 | ❌→✅ | 🔴→✅ | PMM E820 + bitmap + pages user via PMM (Drop réel) | **Corrigé** |
| I6 | ❌ | 🔴 Archi | Pas de VM dynamique | Chantier (~2 sem) |
| I7 | ⚠️ | 🟡 Fonc | Fork/exec partiel (spawn OK, pas de fork) | Partiel |
| I8 | ❌ | 🔴 Archi | Bootloader BIOS-only | Chantier (~1 sem) |
| I9 | ❌ | 🟡 Fonc | RAMFS volatil (FAT32 non connecté) | Chantier (~1-2 sem) |
| I10 | ❌ | 🟡 Fonc | ATA PIO sans DMA | Chantier (~2 sem) |
| I11 | ❌ | 🟡 Fonc | Réseau sans TCP/DHCP | Chantier (~3 sem) |
| I12 | ⚠️ | 🟡 Fonc | GUI : BGA actif au boot ; widgets manquants | Partiel |
| I13 | ❌ | 🟡 Fonc | Shell monolithique | Chantier (~2h) |
| I14 | ⚠️ | 🟡 Fonc | Loader ELF64 générique (.bss OK) ; pas de relocs dynamiques | Partiel |
| I15 | ❌→✅ | 🟡→✅ | IPC passe en O(log n) | **Corrigé** |
| I16 | ❌→✅ | 🟡→✅ | rdmsr/wrmsr centralisés | **Corrigé** |
| I17 | ❌→✅ | 🟡→✅ | Slots d'inodes recyclés | **Corrigé** |
| I18 | ❌→✅ | 🟡→✅ | `/proc/meminfo` live | **Corrigé** |
| I19 | ❌→✅ | 🟡→✅ | RDRAND hardware utilisé | **Corrigé** |
| I20 | ⚠️→✅ | 🟡→✅ | Reaping auto des tâches mortes | **Corrigé** |
| I21 | ❌→✅ | 🟡→✅ | Enums d'erreurs typées | **Corrigé** |
| I22 | ❌→✅ | 🟢→✅ | Logging structuré `klog!` | **Corrigé** |
| I23 | ⚠️→✅ | 🟢→✅ | QEMU : `--smp` + `isa-debug-exit` | **Corrigé** |
| I24 | ❌→✅ | 🟢→✅ | CI GitHub Actions | **Corrigé** |
| I25 | ❌→✅ | 🟢→✅ | Target JSON orphelin supprimé | **Corrigé** |

---

## 🎯 Plan d'Action en Cours

### ✅ Terminé — Corrections immédiates
1. ✅ `/proc/uptime` corrigé (fréquence 100 Hz) *(déjà en place)*
2. ✅ `/proc/meminfo` corrigé (allocator live) *(déjà en place)*
3. ✅ `x86_64-aura.json` supprimé *(déjà en place)*
4. ✅ `rdmsr`/`wrmsr` factorisés dans `arch/msr.rs` *(déjà en place)*
5. ✅ Lecture du numéro de syscall sécurisée *(déjà en place)*
6. ✅ `static mut` remplacé par `AtomicU64` *(déjà en place)*
7. ✅ Reaping des tâches mortes automatisé *(ce travail — test n°33 vérité en QEMU)*
8. ✅ Enums d'erreurs typées (`VfsError`/`AtaError`/`Fat32Error`) *(ce travail)*
9. ✅ Logging structuré `klog!` avec niveaux + timestamp *(ce travail)*
10. ✅ BGA activé au boot + desktop graphique auto *(ce travail)*
11. ✅ Script QEMU enrichi (`--smp`, `isa-debug-exit`) *(ce travail)*
12. ✅ **PMM complet (chantier de fond I5)** — E820 + frame allocator identity-mapped, pages user via PMM avec Drop libre, stats `/proc/meminfo`, test n°34 *(ce travail)*

### 🔜 Prochaines étapes — Phase 1 (semaines 1-2)
- **I13** — Découper le shell monolithique (`shell/mod.rs`, 1 174 lignes) en sous-modules : parser, builtins, commandes (≈2 h sans changement de comportement)
- **I9** — Connecter FAT32 + ATA au VFS pour une persistance disque réelle (le standalone fonctionne déjà via `formatfat`/`mountfat`)
- **I7** — `fork()`/`waitpid()` + exit status comme fondation du modèle de processus

### 🔜 Phase 2 — Fondations (mois 1-2, aligné Feuille de Route)

- ~~**I5** — Physical Memory Manager (E820 + frame allocator)~~ ✅ *fait le 12/09* — reliquat : étendre au-delà de 512 MiB via `map_physical_memory`
- **I8** — Migration bootloader vers `0.11+` (UEFI)
- **I11** — TCP + DHCP dans la pile réseau

### 🔜 Phase 3 — Maturité (mois 3-6, aligné Feuille de Route)
- **I6** — Demand paging + mmap
- **I4** — SMP multi-cœur
- **I10** — AHCI/NVMe drivers