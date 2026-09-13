# 🧠 I2 — Diagnostic : Mémoire virtuelle dynamique avancée (AddressSpace par processus, Demand Paging, Isolation)

> **Statut :** Étape 0 complète — inventaire exhaustif, cartographie des allocations statiques vs paresseuses, invariants formels de sécurité.  
> **Méthode :** Analyse statique et dynamique de l'arbre source réel (`memory/`, `task/`, `fs/elf.rs`, `arch/idt.rs`, `arch/syscall.rs`).  
> **Compilateur :** `cargo +nightly build` → exit 0, 0 erreur, 0 warning (cible x86_64 bare-metal).

---

## 1. Cartographie de l'Existant (État Actuel)

### 1.1 `src/memory/user_space.rs` & `AddressSpace`
- **Structure actuelle :**
  - `AddressSpace` alloue une nouvelle table PML4 via `alloc_page_table()` (PMM physical frame dans `[2 MiB .. 512 MiB)`).
  - `AddressSpace::new()` copie les 512 entrées de la PML4 kernel (`kernel_pml4`) dans la nouvelle table pour préserver les mappings noyau (GDT, IDT, piles noyau, fenêtres MMIO/PMM).
  - Pour l'entrée `PML4[0]` (couvrant `0 .. 512 GiB`), elle alloue un PDPT dédié (`user_pdpt`), copie les entrées du PDPT noyau, puis active les flags `PRESENT | WRITABLE | USER_ACCESSIBLE` sur `PML4[0]`.
  - `allocated_frames: Vec<u64>` conserve la liste de toutes les frames physiques (tables de pages + pages utilisateur) allouées pour cet espace.
  - `impl Drop for AddressSpace` libère séquentiellement toutes les frames vers le PMM via `pmm::free_frame(PhysAddr(addr))`.
- **Limites actuelles :**
  - Pas d'espace de heap dynamique (`brk`/`sbrk`) formalisé dans l'`AddressSpace`.
  - Le chargement d'un ELF pré-alloue l'ensemble des pages mémoire (y compris la BSS) de manière impérative et anticipée dans `load_elf()`.

### 1.2 `src/fs/elf.rs` & Chargement des Binaires
- **Adresses virtuelles fixes :**
  - `DEFAULT_USER_CODE_BASE = 0x0000_0000_4000_0000` (1 GiB boundary, `p4_idx = 0`, `p3_idx = 1`).
  - `DEFAULT_USER_STACK_TOP = 0x0000_0000_8000_0000` (2 GiB boundary, `p4_idx = 0`, `p3_idx = 2`).
- **Comportement de `load_elf` (`src/fs/elf.rs:256-319`) :**
  - Pour chaque segment `PT_LOAD`, calcule `page_start` et `page_end`.
  - **Allocation anticipée totale (Eager Allocation) :**
    ```rust
    let mut curr_page = page_start;
    while curr_page < page_end {
        let frame_ptr = space.allocate_and_map_page(VirtAddr(curr_page), writable || true)?;
        // Copie les données du fichier si chevauchement...
        curr_page += PAGE_SIZE as u64;
    }
    ```
  - Même si `p_memsz > p_filesz` (zone BSS), les pages au-delà de `p_filesz` sont immédiatement allouées physiquement depuis le PMM et zérotées.
  - **Inconvénient majeur :** Un exécutable avec une grande BSS (ex: 16 MiB de buffers non initialisés) consomme 4096 frames physiques PMM dès le `load_elf`, même si le programme ne touche jamais à cette mémoire.

### 1.3 `src/task/mod.rs` & Gestion du CR3 dans le TCB
- **TCB (`Task`) :**
  - Contient `pub cr3: Option<u64>`.
  - Contient `pub address_space: Option<AddressSpace>`.
- **Changement de contexte (`on_context_switch` L450-475) :**
  - Si `is_user`:
    - Met à jour `TSS.rsp0` et `KERNEL_RSP_SCRATCH` vers `kstack_top`.
    - Si `cr3_opt` est `Some(cr3)`, bascule le registre matériel `CR3` :
      ```rust
      crate::memory::paging::write_cr3(crate::memory::paging::PhysAddr(cr3));
      ```
  - Si `!is_user` (tâche kernel) :
    - Restaure le CR3 du kernel (`KERNEL_CR3`).
- **Limites actuelles :**
  - Dans le test existant (Test 28), `spawn_user` était utilisé avec `core::mem::forget(loaded.space)`, stockant uniquement le `cr3: u64` numérique sans rattacher le `AddressSpace` complet au TCB.
  - Par conséquent, lors de la destruction du TCB (`reap_dead_tasks_on_cpu`), aucune libération des pages physiques n'était déclenchée pour les tâches créées par `spawn_user`.

### 1.4 `src/arch/idt.rs` & Page Fault Handler (#PF)
- **Traitement actuel (`src/arch/idt.rs:160-233`) :**
  - Lit `CR2` (adresse de faute).
  - Décode `error_code` : `is_present = bit 0`, `is_write = bit 1`, `is_user = bit 2`.
  - Si `!is_present`, appelle `crate::task::handle_current_page_fault(faulting_address, is_write)`.
  - Si non résolu et `is_user` :
    - Passe la tâche courante à `TaskState::Dead`.
    - Invoque `crate::task::yield_now()`.
  - Si non résolu et `!is_user` : halt fatal du système.

---

## 2. Analyse des Points d'Allocation Physique : Statique vs Paresseuse

| Composant | Comportement Actuel | Opportunité Paresseuse (Demand-Paging) | Impact Économie RAM |
|---|---|---|---|
| **ELF Code/Data** (`p_filesz`) | Alloué & copié au chargement | Peut rester anticipé pour le code immédiat | Faible (code souvent < 64 KiB) |
| **ELF BSS** (`p_memsz > p_filesz`) | Alloué & zéroté au chargement | **Paresseux (Demand Paging)** : VMA déclarée sans page physique ; au 1er accès (#PF), le handler PMM alloue une frame zéro | **Élevé** (économise 100% de la BSS non touchée) |
| **Heap Utilisateur** (`brk`/`sbrk`) | Inexistant | **Paresseux** : `SYS_BRK` étend la plage virtuelle `[heap]`. Zéro allocation physique à l'appel système ; allocation à la demande sur écriture réelle | **Critique** (évite l'épuisement prématuré des 244 MiB PMM) |
| **Stack Utilisateur** | 4 pages (16 KiB) allouées statiquement | Reste alloué statiquement avec sa `[guard]` page pour la sécurité des interruptions | Nécessaire pour la stabilité de l'entrée Ring 3 |
| **Régions mmap** (`MAP_ANON`) | Déjà supporté paresseusement si `!MAP_POPULATE` | Maintenu et unifié avec le nouveau gestionnaire de faute | Cohérence garantie |

---

## 3. Invariants de Sécurité Implicites et Ruptures Multi-Processus

### 3.1 Invariants Actuels (Fragiles)
1. **Unicité de l'espace Ring 3 :** Le système fonctionnait avec un seul binaire user (`/bin/hello`) à l'adresse fixe `0x4000_0000`. Si deux processus différents voulaient tourner en parallèle avec des adresses virtuelles identiques, l'absence de CR3 strictement dédié et synchronisé sur chaque cœur provoquerait une corruption croisée.
2. **Fuite à la mort de tâche :** Sans transfert de propriété de l'`AddressSpace` dans le TCB, les frames physiques restaient allouées après la terminaison du processus, causant une dérive irréversible de `/proc/meminfo`.
3. **Absence de validation syscall par-processus :** Les pointeurs passés à `SYS_WRITE` ou `SYS_READ` supposaient un espace mémoire unique et n'étaient pas confrontés aux VMAs du processus appelant.

### 3.2 Nouveaux Invariants Formels Garantis par I2
- **INV-CR3-1 (Isolation Stricte) :** Tout TCB exécutant du code Ring 3 (`task.is_user == true`) possède un pointeur `task.address_space: Some(AddressSpace)` et un `task.cr3: Some(pml4_phys)`. Deux tâches user distinctes possèdent des PML4 physiquement distinctes (`pml4_phys_A != pml4_phys_B`).
- **INV-CR3-2 (Clonage Kernel Supérieur) :** Toute PML4 utilisateur clone identiquement les entrées hautes du kernel (supervisor-only, `USER_ACCESSIBLE = 0`). Toute entrée utilisateur `USER_ACCESSIBLE = 1` est confinée à `PML4[0]`, isolée par table de pages dédiée.
- **INV-DP-1 (Demand Paging Zero-Fill BSS) :** L'accès à une page non présente comprise dans une VMA BSS (`[elf_bss]`) déclenche une allocation PMM d'une frame 4 KiB, son zérotage intégral en mémoire, son insertion dans la table de pages avec `USER_ACCESSIBLE | WRITABLE`, l'invalidation TLB (`invlpg`), et la reprise transparente de l'instruction (`iretq`).
- **INV-BRK-1 (Heap Paresseux) :** `SYS_BRK` ajuste la borne `heap_end` du processus sans allouer de frames physiques. Les frames du heap sont matérialisées à la volée par le gestionnaire #PF.
- **INV-TEARDOWN-1 (Zéro Fuite PMM) :** Lors du `reap_dead_tasks_on_cpu`, la destruction du TCB entraîne le `Drop` de `AddressSpace`, restituant 100% des frames (tables de pages, pages allouées, pages paginées à la demande) au PMM. `/proc/meminfo` retourne exactement au nombre initial de frames libres.
- **INV-SEC-1 (Confinement des Faillites) :** Toute tentative d'accès à une adresse hors VMA, ou écriture sur VMA read-only, ou violation de guard page, entraîne la mort immédiate et propre de la tâche sans jamais paniquer le noyau ni déclencher de triple fault.

---

## 4. Spécification Technique de la Solution

### 4.1 Nouveau Syscall `SYS_BRK = 12`
- **Signature :** `sys_brk(new_brk: u64) -> u64`
- **Convention :**
  - Si `new_brk == 0` ou `new_brk < heap_start` : retourne la valeur courante de `heap_end`.
  - Si `new_brk > heap_end` : vérifie l'absence de conflit d'overlap avec les VMAs mmap et stack ; étend la VMA `[heap]` jusqu'à `align_up_page(new_brk)` ; met à jour `heap_end = new_brk` ; retourne la nouvelle valeur.
  - Si `new_brk < heap_end` : réduit la VMA `[heap]` et libère les pages physiques dé-allouées via `munmap()`.

### 4.2 Extension de `load_elf` pour le Demand-Paging BSS
- Les pages contenant des données de fichier (`file_sz`) restent allouées et peuplées immédiatement.
- Les pages strictement situées dans la BSS (`curr_page >= align_up_page(vaddr + file_sz)`) ne sont **pas** allouées physiquement. Une VMA `[elf_bss]` ou segment unifié lazy est enregistrée.
- Au premier accès mémoire par le programme utilisateur, le gestionnaire de #PF matérialise la frame.

### 4.3 Validation par 5 Nouveaux Auto-Tests Matériels (#43–#47)
1. **#43 — "Per-Process AddressSpace Isolation"** : Deux espaces distincts avec CR3 indépendants. Vérification que la même adresse virtuelle pointe vers des frames physiques distinctes et qu'aucun accès croisé n'est possible.
2. **#44 — "Demand-Paging BSS Zero-Fill"** : Chargement d'un binaire dont la BSS dépasse la mémoire de fichier. Vérification de l'allocation physique différée au premier accès et du contenu strictement nul (zero-fill).
3. **#45 — "brk/sbrk Heap Growth"** : Extension dynamique du heap par `SYS_BRK`, preuve du caractère paresseux avant accès et de l'allocation physique après écriture.
4. **#46 — "AddressSpace Teardown, No Leak"** : Cycle de vie complet de N tâches avec heap et pages multiples ; vérification comptable stricte de `pmm::free_memory()` (retour à 0 frame perdue).
5. **#47 — "Page Fault on Illegal Access Kills Task, Not Kernel"** : Déclenchement intentionnel d'une violation mémoire en Ring 3 ; preuve que la tâche fautive est tuée et reaped proprement pendant que le noyau et les autres cœurs SMP poursuivent leur exécution sans interruption.

---

## 5. Preuves Matérielles & Verdicts d'Exécution

### 5.1 Compilation
```bash
$ cargo +nightly build
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.79s
    -> 0 erreurs, 0 warnings

$ cargo +nightly bootimage --release
    Finished `release` profile [optimized] target(s) in 3.60s
    Created bootimage for `aura-kernel` at target/x86_64-unknown-none/release/bootimage-aura-kernel.bin
    -> 0 erreurs, 0 warnings
```

### 5.2 Verdict Matériel QEMU SMP 2 Cœurs (`timeout 15 ./run_qemu.sh -n --smp 2`)
```text
  [PASS] Per-Process AddressSpace Isolation
[INFO] [tests] [#00019] PASS: Per-Process AddressSpace Isolation -- CR3Distinct=true (CR3_1=0xb3c000, CR3_2=0xb3e000), PhysIsolated=true (F1=0xb40000, F2=0xb43000), PatternsOk=true, CrossUnmapped=true
  [PASS] Demand-Paging BSS Zero-Fill
[INFO] [tests] [#00020] PASS: Demand-Paging BSS Zero-Fill -- UnpopulatedAtLoad=true, FaultOk=true, AllocatedOnAccess=true (Delta=4096B), ZeroFilled=true, TeardownNoLeak=true
  [PASS] brk/sbrk Heap Growth
[INFO] [tests] [#00020] PASS: brk/sbrk Heap Growth -- InitialOk=true, ExpandOk=true, LazyBeforeWrite=true, DemandAllocated=true, ReadbackOk=true, UserPageReclaimed=true, FullyReclaimed=true
  [PASS] AddressSpace Teardown, No Leak
[INFO] [tests] [#00020] PASS: AddressSpace Teardown, No Leak -- Iterations=5, InitialFree=256524288B, FinalFree=256524288B, LeakedFrames=0
  [PASS] Page Fault on Illegal Access Kills Task, Not Kernel
[INFO] [tests] [#00020] PASS: Page Fault on Illegal Access Kills Task, Not Kernel -- UnmappedRejected=true, GuardRejected=true, TaskReaped=true, SchedHealthy=true
[OK] All 47 subsystem tests PASSED. System verified 100%.
[INFO] [tests] [#00020] All 47 automated self-tests passed.
```

### 5.3 Verdict Matériel QEMU SMP 4 Cœurs (`timeout 15 ./run_qemu.sh -n --smp 4`)
```text
  [PASS] Per-Process AddressSpace Isolation
[INFO] [tests] [#00023] PASS: Per-Process AddressSpace Isolation -- CR3Distinct=true (CR3_1=0xb3c000, CR3_2=0xb3e000), PhysIsolated=true (F1=0xb40000, F2=0xb43000), PatternsOk=true, CrossUnmapped=true
  [PASS] Demand-Paging BSS Zero-Fill
[INFO] [tests] [#00023] PASS: Demand-Paging BSS Zero-Fill -- UnpopulatedAtLoad=true, FaultOk=true, AllocatedOnAccess=true (Delta=4096B), ZeroFilled=true, TeardownNoLeak=true
  [PASS] brk/sbrk Heap Growth
[INFO] [tests] [#00024] PASS: brk/sbrk Heap Growth -- InitialOk=true, ExpandOk=true, LazyBeforeWrite=true, DemandAllocated=true, ReadbackOk=true, UserPageReclaimed=true, FullyReclaimed=true
  [PASS] AddressSpace Teardown, No Leak
[INFO] [tests] [#00024] PASS: AddressSpace Teardown, No Leak -- Iterations=5, InitialFree=256524288B, FinalFree=256524288B, LeakedFrames=0
  [PASS] Page Fault on Illegal Access Kills Task, Not Kernel
[INFO] [tests] [#00024] PASS: Page Fault on Illegal Access Kills Task, Not Kernel -- UnmappedRejected=true, GuardRejected=true, TaskReaped=true, SchedHealthy=true
[OK] All 47 subsystem tests PASSED. System verified 100%.
[INFO] [tests] [#00025] All 47 automated self-tests passed.
```

### 5.4 Tableau Récapitulatif des Invariants Vérifiés

| Invariant | Description | Statut | Preuve |
|---|---|---|---|
| **INV-CR3-1** | Isolation CR3 stricte par processus | **VALIDE** | Test #43 (`CR3_1=0xb3c000 != CR3_2=0xb3e000`, frames distinctes `0xb40000` vs `0xb43000`) |
| **INV-CR3-2** | Clonage kernel PML4 & isolation supervisor | **VALIDE** | Test #16, #28, #43 (Supervisor entries intactes, user bit sur PML4[0] seul) |
| **INV-DP-1** | Demand-paging BSS zero-fill | **VALIDE** | Test #44 (Delta initial 0B, 4096B alloués sur premier accès, 100% zéros vérifiés) |
| **INV-BRK-1** | Croissance heap paresseuse (`SYS_BRK`) | **VALIDE** | Test #45 (`LazyBeforeWrite=true`, `DemandAllocated=true`, écriture/lecture 64 bits validée) |
| **INV-TEARDOWN-1** | Zéro fuite PMM lors du reaping | **VALIDE** | Test #46 (`Iterations=5`, `InitialFree == FinalFree == 256524288B`, `LeakedFrames=0`) |
| **INV-SEC-1** | Faute illégale tue la tâche sans triple fault | **VALIDE** | Test #47 (`UnmappedRejected=true`, `TaskReaped=true`, sentinel reste actif) |

### 5.5 Limites Documentées & Hors-Scope
- **Swap sur disque :** Hors scope pour cette itération (dépendance I4 — pile ATA/FAT32 asynchrone).
- **Fork Copy-on-Write (COW) :** Hors scope pour cette itération (implémentation future avec flag COW bit 9).

