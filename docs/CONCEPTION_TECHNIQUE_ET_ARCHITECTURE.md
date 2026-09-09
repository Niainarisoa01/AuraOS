# 📐 AuraOS — Dossier de Conception Technique & Architecture Système

> **Document de Référence Technique — Version 0.1.0 (Bare-Metal x86_64)**  
> **Auteur & Architecte Système :** Niaina  
> **Statut :** Noyau fonctionnel, testé et validé sur machine physique et QEMU (15/15 tests réussis à 100%)  
> **Dépôt Git Officiel :** [https://github.com/Niainarisoa01/AuraOS.git](https://github.com/Niainarisoa01/AuraOS.git)

---

## 1. 🌌 Philosophie & Principes Fondamentaux de Conception

AuraOS est conçu selon quatre principes architecturaux stricts :

1. **100% Pur Rust (`#![no_std]`, `#![no_main]`) :**
   - Élimination par conception de 70% des vulnérabilités critiques inhérentes au C/C++ (dépassements de tampon, lectures sauvages *Use-After-Free*, déréférencements de pointeurs nuls, courses critiques de concurrence).
   - Gestion de mémoire assistée par le *Borrow Checker* avec abstractions à coût nul (*Zero-Cost Abstractions*).
2. **Architecture Micro-Noyau Hybride & Modulaire :**
   - Le cœur du système (*Ring 0*) assure les primitives minimales : pagination, commutation de contexte, gestion des interruptions et IPC.
   - Les sous-systèmes (pilotes, VFS, interface graphique) sont isolés dans des modules aux interfaces rigoureusement définies.
3. **Zéro Dette Technique Héritée :**
   - Aucune dépendance envers du code historique des années 1980/1990. 
   - Utilisation native du mode 64-bit Long Mode d'Intel/AMD sans rétrocompatibilité 16-bit en temps réel.
4. **Validation et Diagnostics Embarqués (Self-Testing Kernel) :**
   - Le noyau intègre sa propre suite de diagnostics autonomes exécutée à chaque démarrage, garantissant l'intégrité de la mémoire, de la pile, du système de fichiers et des pilotes.

---

## 2. 🏛️ Schéma d'Architecture Globale du Système

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       ESPACE UTILISATEUR (RING 3 - CIBLE)                   │
│      [Applications Graphiques]       [Utilitaires CLI]       [Serveurs IPC] │
├─────────────────────────────────────────────────────────────────────────────┤
│                          COUCHE LOGICIELLE & INTERFACES                     │
│   ┌───────────────────────────────────┐ ┌─────────────────────────────────┐ │
│   │   Aura Desktop Compositor (GUI)   │ │      AuraShell Interactif       │ │
│   │  - Fond Cosmique Dégradé          │ │  - 25+ commandes intégrées     │ │
│   │  - Dock Flottant Glassmorphism    │ │  - Historique & Buffer Ligne    │ │
│   │  - Fenêtres feux tricolores (🔴🟡🟢)│ │  - Diagnostics temps réel       │ │
│   └───────────────────────────────────┘ └─────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────────────┤
│                         SOUS-SYSTÈMES DU NOYAU (RING 0)                     │
│   ┌────────────────────────┐ ┌────────────────────┐ ┌───────────────────┐   │
│   │ Virtual File System    │ │ Task Scheduler     │ │ 2D Graphics Engine│   │
│   │ - RAMFS en Inodes      │ │ - TCB 16 KiB Stack │ │ - 32bpp TrueColor │   │
│   │ - Arborescence / & ..  │ │ - ABI System V     │ │ - Alpha Blending  │   │
│   │ - ATA PIO 28-bit Block │ │ - switch_context   │ │ - Bitmap Font 8x8 │   │
│   └────────────────────────┘ └────────────────────┘ └───────────────────┘   │
├─────────────────────────────────────────────────────────────────────────────┤
│                      GESTION MÉMOIRE & SYNCHRONISATION                      │
│   ┌────────────────────────┬──────────────────────┬─────────────────────┐   │
│   │ 8 MiB Kernel Heap      │ Paging 4 Niveaux     │ Spinlock Interrupt- │   │
│   │ - Rust alloc (Box/Vec) │ - PML4 / PDPT / PD/PT│   Safe (CLI/STI)    │   │
│   └────────────────────────┴──────────────────────┴─────────────────────┘   │
├─────────────────────────────────────────────────────────────────────────────┤
│                 COUCHE D'ABSTRACTION MATÉRIELLE (HAL & DRIVERS)             │
│   ┌───────────────┬──────────────┬──────────────┬─────────────┬─────────┐   │
│   │ VGA Text Mode │ Serial COM1  │ Clavier PS/2 │ PCI Bus Scan│ CMOS RTC│   │
│   │ MMIO 0xb8000  │ UART 0x3F8   │ IRQ 1 (8042) │ Config Space│ Horloge │   │
│   └───────────────┴──────────────┴──────────────┴─────────────┴─────────┘   │
├─────────────────────────────────────────────────────────────────────────────┤
│                     CŒUR MATÉRIEL CPU x86_64 (BARE-METAL)                   │
│   ┌────────────────────────┬──────────────────────┬─────────────────────┐   │
│   │ Global Descriptor Table│ Interrupt Descriptor │ Dual 8259 PIC       │   │
│   │ (GDT 64-bit Ring 0)    │ Table (IDT 256 ISRs) │ Remappage IRQ 32-47 │   │
│   └────────────────────────┴──────────────────────┴─────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. 🔌 Chaîne de Démarrage & Édition de Liens (Boot & Linker)

### 3.1. Amorçage via Bootloader 0.9.x
1. Le BIOS/UEFI charge le bootloader qui initialise le processeur, active la ligne A20, bascule en mode protégé (32-bit), puis configure la première table de pages et saute en **Long Mode 64-bit**.
2. Le bootloader parse le binaire ELF `aura-kernel` et charge ses segments `LOAD` en mémoire physique et virtuelle.
3. Le bootloader transmet le contrôle au point d'entrée `_start` du noyau (`src/main.rs`).

### 3.2. Script d'Édition de Liens : `linker.ld`
Pour empêcher le panic classique `PageAlreadyMapped` du bootloader, le linker script impose une **séparation stricte de chaque segment LOAD sur des frontières de pages physiques de 4 KiB (0x1000)** :

```ld
ENTRY(_start)

SECTIONS
{
    . = 1M; /* Charge le noyau à 1 MiB (évite la zone réservée < 1 MiB) */

    .text : ALIGN(4K)
    {
        *(.text .text.*)
    }

    . = ALIGN(4K);

    .rodata : ALIGN(4K)
    {
        *(.rodata .rodata.*)
    }

    . = ALIGN(4K);

    .data : ALIGN(4K)
    {
        *(.data .data.*)
        *(.got .got.*)
        *(.dynamic)
    }

    . = ALIGN(4K);

    .bss : ALIGN(4K)
    {
        *(.bss .bss.*)
    }

    /DISCARD/ :
    {
        *(.eh_frame)
        *(.eh_frame_hdr)
        *(.comment)
    }
}
```

### 3.3. Configuration Cargo & Drapeau `-znorelro`
Dans `.cargo/config.toml` :
```toml
[build]
target = "x86_64-unknown-none"

[unstable]
build-std = ["core", "compiler_builtins", "alloc"]
build-std-features = ["compiler-builtins-mem"]

[target.x86_64-unknown-none]
rustflags = [
    "-C", "relocation-model=static",
    "-C", "link-arg=-Tlinker.ld",
    "-C", "link-arg=-znorelro"
]
```
* **Rôle de `-znorelro` :** Élimine la création d'un segment RELRO scindé avec des permissions hybrides qui partageait des pages avec `.rodata`, garantissant ainsi que les 3 segments ELF (`R-X`, `R--`, `RW-`) possèdent des pages mémoire totalement disjointes.

---

## 4. ⚙️ Couche Bas-Niveau CPU & Matériel (`src/arch/`)

### 4.1. GDT (Global Descriptor Table — `src/arch/gdt.rs`)
La GDT configure la segmentation 64-bit :
* **Entrée 0x00 :** Descripteur nul obligatoire.
* **Entrée 0x08 :** Segment Code Ring 0 (Exécutable, Lecture, Long Mode L=1).
* **Entrée 0x10 :** Segment Données Ring 0 (Lecture/Écriture).

> **Spécificité Critique :** Le bit **Accessed** (bit 0 du champ Type) est explicitement forcé à `1` (`0b0001_1011` pour le code, `0b0001_0011` pour les données) et la table est placée en section inscriptible `#[unsafe(link_section = ".data")]`. Cela empêche le CPU x86 d'émettre un Page Fault (#PF) lors du rechargement de `CS`, `DS` et `SS`.

### 4.2. IDT (Interrupt Descriptor Table — `src/arch/idt.rs`)
La table contient 256 descripteurs d'interruption 64-bit (16 octets chacun) :
* **Exceptions CPU (0 à 31) :**
  - Interruption 0 (#DE) : Division par zéro.
  - Interruption 6 (#UD) : Instruction invalide (Opcode inconnu).
  - Interruption 8 (#DF) : Double faute critique.
  - Interruption 13 (#GP) : Fautes générales de protection.
  - Interruption 14 (#PF) : Défauts de page avec lecture immédiate du registre matériel `CR2` (adresse fautive) et du code d'erreur.
* **Interruptions Matérielles (IRQs 32 à 47) :**
  - IRQ 0 (Vecteur 32) : Timer d'horloge système (PIT 8254) incrémentant le compteur atomique de `TICKS`.
  - IRQ 1 (Vecteur 33) : Clavier PS/2 (Intel 8042).

### 4.3. Double PIC 8259A (`src/arch/pic.rs`)
Les contrôleurs d'interruption maître et esclave sont réinitialisés et remappés :
* PIC 1 (Maître) : Ports d'E/S `0x20` / `0x21` réassigné aux interruptions 32–39.
* PIC 2 (Esclave) : Ports d'E/S `0xA0` / `0xA1` réassigné aux interruptions 40–47.
* Envoi de signaux d'acquittement `EOI` (*End Of Interrupt*) à chaque traitement.

### 4.4. Télémétrie Matérielle & Détection (`cpuid.rs`, `cmos.rs`, `pci.rs`)
* **CPUID :** Identification du fondeur (*AuthenticAMD*, *GenuineIntel*), nom de marque du processeur, et comptage précis de cycles via l'instruction `RDTSC`.
* **CMOS RTC :** Lecture des registres `0x70`/`0x71` avec conversion BCD vers binaire pour extraire date et heure UTC (siècle, année, mois, jour, heure, minute, seconde).
* **Bus PCI :** Balayage complet des 256 bus, 32 slots et 8 fonctions via les ports de configuration `0xCF8`/`0xCFC` avec identification des Vendor IDs et Class IDs.

---

## 5. 🧠 Gestionnaire de Mémoire (`src/memory/`)

### 5.1. Pagination Virtuelle à 4 Niveaux (`paging.rs`)
AuraOS repose sur la pagination 64-bit avec adresses virtuelles sur 48 bits canoniques :
* Décomposition en 4 index de 9 bits chacun (512 entrées par table) :
  - **PML4** (Page Map Level 4) : bits 39–47
  - **PDPT** (Page Directory Pointer Table) : bits 30–38
  - **PD** (Page Directory) : bits 21–29
  - **PT** (Page Table) : bits 12–20
  - **Offset dans la page 4K** : bits 0–11

### 5.2. Allocateur de Tas Dynamique (Heap 8 MiB — `allocator.rs`)
* Zone mémoire réservée de **8 Mébioctets** (8 388 608 octets).
* Gestionnaire de tas dynamique avec fusion des blocs libres (*coalescing*) pour éliminer les fuites de mémoire.
* Enregistrement comme allocateur global via `#[global_allocator]`.
* Support complet de la bibliothèque standard `alloc` de Rust :
  - `Box<T>` : Allocation dynamique sur le tas.
  - `Vec<T>` : Vecteurs auto-extensibles.
  - `String` et macro `format!(...)` : Chaînes de caractères dynamiques.

---

## 6. 🧵 Multitâche & Ordonnanceur (`src/task/`)

### 6.1. Task Control Block (TCB)
Chaque fil d'exécution possède sa structure propre :
```rust
pub struct Task {
    pub id: usize,
    pub name: &'static str,
    pub rsp: usize,                  // Pointeur de pile sauvegardé
    pub stack: Option<Box<[u8]>>,    // Pile dédiée de 16 KiB
    pub state: TaskState,            // Ready, Running, Sleeping, Dead
    pub ticks: u64,                  // Comptabilisation CPU
}
```

### 6.2. Conformité Stricte à l'ABI System V x86_64
* À l'entrée d'une fonction, l'ABI impose `(RSP + 8) % 16 == 0`.
* Le cadre initial de pile de 72 octets (9 slots de 64 bits : padding, RIP, RFLAGS, RBP, RBX, R12, R13, R14, R15) est calculé par `(stack_top & !0xF) - 8`.
* Résultat : Le pointeur `rsp` de la tâche est strictement aligné sur 16 octets tout en assurant l'alignement requis par le compilateur après l'instruction `ret`.

### 6.3. Commutation de Contexte (`switch_context`)
Routine assembleur pure sauvegardant les 7 registres callee-saved sur la pile de la tâche sortante et restaurant ceux de la tâche entrante :
```rust
#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(old_rsp: *mut usize, new_rsp: usize) {
    core::arch::naked_asm!(
        "pushfq", "push rbp", "push rbx", "push r12", "push r13", "push r14", "push r15",
        "mov [rdi], rsp",
        "mov rsp, rsi",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp", "popfq",
        "ret"
    );
}
```

---

## 7. 📁 Système de Fichiers Virtuel (VFS & RAMFS — `src/fs/`)

### 7.1. Structure en Inodes
* Le VFS gère une table d'inodes virtuels en mémoire vive.
* Chaque inode représente soit un fichier régulier (contenant des octets bruts), soit un répertoire (contenant la liste des identifiants d'enfants).
* Inode racine (`/`) initialisé avec l'ID `0`.
* Support complet de la navigation hiérarchique : chemins absolus (`/home/user`), chemins relatifs (`docs`), et retour parent (`..`).
* Nettoyage récursif de la mémoire lors de la suppression de répertoires (`rm`).

### 7.2. Pilote Disque ATA / IDE PIO 28-bit (`src/drivers/ata.rs`)
* Contrôle du contrôleur de disque primaire sur le port d'E/S `0x1F0`–`0x1F7`.
* Primitives de lecture de secteurs bruts de 512 octets en mode LBA 28-bit avec scrutation d'état (*polling* BSY et DRQ).

---

## 8. 🖥️ Interface Utilisateur : Shell & Bureau Graphique

### 8.1. AuraShell Interactif (`src/shell/mod.rs`)
Console bare-metal interactive gérant l'édition de ligne, l'effacement par backspace et plus de 25 commandes intégrées :
* **Système & Matériel :** `info`, `cpu`, `pci`, `mem`, `time`, `date`, `ticks`.
* **Processus & Tâches :** `tasks`, `ps`, `yield`.
* **Fichiers & Navigation :** `ls`, `cd`, `pwd`, `cat`, `touch`, `mkdir`, `write`, `rm`, `readsec`.
* **Diagnostics & Énergie :** `test`, `gui`, `clear`, `reboot`, `shutdown`, `halt`.

### 8.2. Bureau macOS Compositor (`src/gui/mod.rs` & `src/drivers/framebuffer.rs`)
* **Moteur 2D Canvas :**
  - Rendu 32-bit TrueColor (ARGB 8888).
  - Alpha blending avec mélange pondéré des composantes Rouge, Vert, Bleu.
  - Primitives géométriques : rectangles pleins, contours, rectangles arrondis (*rounded rects*), lignes et cercles.
  - Rendu typographique bitmap basé sur police vectorielle 8x8.
* **Composants Desktop macOS :**
  - **Fond Cosmique Dégradé :** Dégradé vertical du Bleu Nuit Cosmique (`0x0A0E1A`) vers l'Obsidienne Profonde (`0x030712`) avec halo d'aurore cyan.
  - **Barre des menus supérieure :** Verre dépoli translucide (effet *Frosted Glass*), logo Aura, menus (File, Edit, View, Window, Help) et horloge UTC dynamique.
  - **Dock Flottant en bas d'écran :** Forme de pilule translucide, bordure lumineuse, icônes d'applications (Terminal, Fichiers, Paramètres, Navigateur) et indicateurs d'applications en cours.
  - **Fenêtres multi-panneaux :** Cadres à coins arrondis, ombres portées, barre de titre avec feux tricolores (Fermer 🔴, Minimiser 🟡, Maximiser 🟢).

---

## 9. 🧪 Suite d'Auto-Tests Embarquée (15/15 Validés à 100%)

À chaque amorçage, le noyau exécute automatiquement 15 tests unitaires bare-metal :

| # | Nom du Test | Objectif Validé | Résultat |
|---|-------------|-----------------|:--------:|
| 1 | **Paging 4-Level Indexing** | Décomposition PML4/PDPT/PD/PT et alignement 4K | **PASS** |
| 2 | **Paging Edge Cases** | Arithmétique d'adresses, offset 0x234, arrondi supérieur | **PASS** |
| 3 | **Heap Coalescing** | 50 allocations/désallocations avec fusion zéro-fuite | **PASS** |
| 4 | **Heap Stress Test** | 500 allocations dynamiques (16B à 4096B, pic ~1 Mo) | **PASS** |
| 5 | **Heap Fragmentation** | Résistance aux trous de fragmentation (100 slots) | **PASS** |
| 6 | **VFS Inode CRUD** | Création, écriture, lecture et suppression de fichier | **PASS** |
| 7 | **VFS Hierarchy & Rm** | Arborescence imbriquée, `..` et suppression récursive | **PASS** |
| 8 | **CMOS RTC Range** | Validité du calendrier matériel (Année >= 2026, UTC) | **PASS** |
| 9 | **CPUID & RDTSC** | Fondeur processeur (len=12), marque, delta cycles TSC | **PASS** |
| 10 | **Multitasking ABI Stack** | Alignement pile 16B & conformité System V ABI | **PASS** |
| 11 | **Spinlock Symmetry** | Exclusion mutuelle, try_lock, et symétrie sous interruptions | **PASS** |
| 12 | **Dynamic Strings & Vec** | Réallocation dynamique, croissance de vecteurs de chaînes | **PASS** |
| 13 | **2D Canvas & Alpha Blend** | Mélange de couleurs alpha 0%, 50%, 100% et police bitmap | **PASS** |
| 14 | **Graphics Edge Cases** | Découpe de pixels hors écran, rectangles nuls/géants | **PASS** |
| 15 | **PCI Bus Validation** | Découverte matérielle valide (6 périphériques PCI) | **PASS** |

---

## 10. 🗺️ Prochaines Étapes de Développement (Roadmap)

1. **Pilote Vidéo Bochs BGA / VBE (1024x768x32bpp) :**
   - Basculer le contrôleur PCI `[00:02.0]` en mode graphique haute définition pour afficher directement le bureau macOS à l'écran.
2. **Ordonnanceur Préemptif :**
   - Basculement automatique de tâche cadencé par l'interruption PIT IRQ 0 (quantum de temps).
3. **Pilote de Souris PS/2 (IRQ 12) :**
   - Réception des paquets souris et déplacement d'un curseur matériel sur le bureau.
4. **Persistance Disque ATA & FAT32 :**
   - Écriture d'une partition persistante conservant les fichiers après extinction.
5. **Espace Utilisateur Ring 3 & Syscalls :**
   - Configuration du TSS, isolation des processus et exécution de binaires ELF utilisateurs.
