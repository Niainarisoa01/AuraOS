# Diagnostic Préalable — Chantier I3 : Support de Démarrage UEFI (en complément du BIOS/Legacy)

**Date :** 12 Septembre 2026  
**Projet :** AuraOS (x86_64 bare-metal, Rust `#![no_std]`, zéro dépendance externe)  
**Auteur :** Ingénieur Noyau AuraOS  

---

## 1. Cartographie du Chemin de Démarrage BIOS Actuel

Le démarrage actuel d'AuraOS repose sur la suite `bootloader 0.9.23` + l'outil hôte `bootimage`.

### 1.1 Séquence Séquentielle Étape par Étape

1. **Secteur de Boot MBR (Stage 1 — 16-bit Real Mode, `0x7C00`)** :
   - Le BIOS charge le premier secteur de 512 octets à l'adresse physique `0x0000:0x7C00`.
   - Ce code initialise les segments (`DS=ES=SS=0`, pile sous `0x7C00`).
   - Il utilise les interruptions BIOS pour charger le second étage (`Stage 2`) depuis les secteurs consécutifs du disque via `INT 13h Extensions` (LBA).

2. **Stage 2 — Énumération Matérielle BIOS & Préparation Long Mode** :
   - **Découverte de la mémoire physique (E820)** : Le chargeur invoque le service BIOS `INT 15h, AX=E820h` en boucle pour obtenir la liste des descripteurs de plages physiques (`base`, `length`, `type`).
   - **Passage en Mode Protégé 32 bits** :
     - Chargement d'une GDT 32 bits temporaire.
     - Activation de la ligne A20 (via le contrôleur clavier ou le port Fast A20 `0x92`).
     - Bascule du bit `CR0.PE = 1` (Protection Enable).
     - Saut lointain (`jmp 0x08:protected_mode`) vers le code 32 bits.

3. **Stage 3 — Établissement de la Pagination 4 Niveaux & Bascule Long Mode** :
   - Allocation et initialisation des tables de pages x86_64 (PML4, PDPT, PD, PT) dans les pages de mémoire basse (`0x1000..0x5000`).
   - Mise en place d'un mapping d'identité (identity-mapping) pour les premiers mégaoctets et chargement du binaire ELF du noyau.
   - Les segments `LOAD` du binaire ELF sont copiés en mémoire physique selon les en-têtes de programme ELF (pour `aura-kernel` : `.text` à `0x100000`, `.rodata` à `0x146000`, `.data`/`.bss` à `0x152000`).
   - Activation du bit `CR4.PAE = 1` (Physical Address Extension).
   - Configuration du MSR `EFER` (Extended Feature Enable Register, `0xC0000080`) avec `LME = 1` (Long Mode Enable).
   - Chargement de l'adresse physique de la PML4 dans le registre `CR3`.
   - Activation de la pagination avec `CR0.PG = 1`.
   - Saut lointain vers le segment de code 64 bits (`CS = 0x18`, 64-bit code descriptor).

4. **Point d'Entrée Rust Initial (`_start`)** :
   - Le chargeur 64 bits prépare sur la pile la structure `bootloader::bootinfo::BootInfo` contenant la carte mémoire E820 convertie (`boot_info.memory_map`).
   - Le pointeur `&'static BootInfo` est placé dans le registre `%rdi` (conforme à l'ABI System V AMD64).
   - Saut vers le symbole `_start` du noyau (`src/main.rs:34`).

---

## 2. Points de Couplage avec le Noyau

L'audit du code noyau (`src/`) met en évidence les points de couplage suivants avec le mode de démarrage :

| Composant | État Initial hérité du Bootloader | Action du Noyau (`src/main.rs`) | Dépendance BIOS Directe ? |
|:---|:---|:---|:---:|
| **GDT / TSS** | GDT temporaire basique fournie par le bootloader | `arch::gdt::init()` réinstalle une GDT kernel complète + TSS propre | ❌ Non (réinitialisé par le noyau) |
| **IDT** | IDT non configurée ou minimale | `arch::idt::init()` installe l'IDT 64 bits complète et charge `LIDT` | ❌ Non (réinitialisé par le noyau) |
| **PIC 8259** | Vecteurs BIOS legacy (0x08..0x0F) | `arch::pic::init()` remappe les IRQ matérielles vers 0x20..0x2F puis masque | ⚠️ Présence du PIC supposée |
| **PIT 8254** | Fréquence BIOS par défaut (18.2 Hz) | `drivers::pit::init()` reprogramme le canal 0 à 100 Hz | ❌ Non (I/O ports standards 0x40-0x43) |
| **PMM** | `boot_info.memory_map` (E820) en RDI | `memory::pmm::init(boot_info)` lit directement le type `bootloader::bootinfo::BootInfo` | 🔴 **Oui (Couplage fort au format E820 bootloader)** |
| **Pagination Initiale** | Identity mapping partiel (0..2 MiB + kernel) | `memory::paging::init_hardware_mappings()` complète PML4[0] (2..512 MiB en huge pages 2 MiB, MMIO 3..4 GiB en huge page 1 GiB) | 🟡 **Oui (Suppose la topologie PML4[0] existante)** |
| **ACPI / MADT** | Aucune aide du bootloader | `arch::acpi::init()` scanne la mémoire EBDA (`0x80000..0xA0000`) et BIOS ROM (`0xE0000..0x100000`) à la recherche de `"RSD PTR "` | 🟡 **Oui (Le scan BIOS échoue si le RSDP n'est pas dans l'EBDA/ROM)** |

---

## 3. Analyse d'Indépendance : Ce qui Change vs Ce qui Reste Identique

### 3.1 Ce qui est DÉJÀ Indépendant du Mode de Boot (Inchangé)
- **SMP multi-cœur** (`arch::smp.rs`) : Découverte des cœurs via la table ACPI MADT, réveil par séquences IPI standardisées (`INIT-SIPI-SIPI`), GDT/TSS et ordonnanceurs per-CPU.
- **Local APIC & Timer LAPIC** (`arch::apic.rs`) : MMIO à `0xFEE00000`, calibrage et interruptions périodiques (Vecteur 0x40).
- **Gestionnaire d'Espace Utilisateur & Ring 3** (`memory::user_space.rs`, `arch::syscall.rs`) : PML4 dédiée par processus, demand-paging BSS et heap `brk`.
- **Système de Fichiers & Pilotes** (`fs/`, `drivers/`) : VFS, RAMFS, ATA PIO, Serial COM1, Clavier PS/2, PCI, RTL/Intel e1000, compositeur BGA.
- **Suite de Tests Matériels** (`tests/mod.rs`) : Les 47 auto-tests existants testent les mécanismes internes du noyau et sont agnostiques au bootloader dès lors que la mémoire et les interruptions sont actives.

### 3.2 Ce qui est Strictement Lié au BIOS (À Généraliser pour UEFI)
1. **Source de la Carte Mémoire** :
   - BIOS : `bootloader::bootinfo::MemoryMap` dérivée de l'E820.
   - UEFI : Table de descripteurs `EFI_MEMORY_DESCRIPTOR` obtenue via `GetMemoryMap`.
   - **Solution** : Abstraction `MemoryRegion` unifiée avec les variantes `Usable`, `Reserved`, `AcpiReclaimable`, etc.
2. **Localisation du Pointeur RSDP ACPI** :
   - BIOS : Scan manuel d'adresses physiques basses (EBDA / ROM).
   - UEFI : Pointeur direct fourni dans `EFI_SYSTEM_TABLE.ConfigurationTable` sous le GUID `EFI_ACPI_20_TABLE_GUID` ou `ACPI_10_TABLE_GUID`.
   - **Solution** : Passer `Option<u64>` (adresse physique du RSDP) dans la structure `BootInfo`. Si fourni (UEFI), `arch::acpi::init_with_rsdp` utilise directement cette adresse. Si absent (BIOS), scan EBDA/ROM en repli transparent.
3. **Structure d'Amorçage `BootInfo`** :
   - Actuellement spécifique à la crate `bootloader`.
   - **Solution** : Créer un module unifié `src/boot/` exposant `BootInfo`, `BootMethod`, `MemoryRegion`.

---

## 4. Comparaison des États Processeur à l'Entrée du Chargeur

| Paramètre | Entrée Trampoline BIOS (16-bit) | Entrée Chargeur UEFI (`efi_main`) |
|:---|:---|:---|
| **Mode Processeur** | 16-bit Real Mode | **64-bit Long Mode natif** |
| **Pagination** | Désactivée (`CR0.PG = 0`) | **Active (`CR0.PG = 1`), 4 niveaux** |
| **Mapping Mémoire** | Aucun (Adresses physiques directes < 1 Mo) | **Identity-mapping 1:1 de toute la RAM physique** |
| **Interruptions** | Interrompables via BIOS IVT (16 bits) | Actives via firmware UEFI, arrêtées à `ExitBootServices` |
| **Accès Disque / Fichiers** | Secteurs bruts via INT 13h | `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL` ou ELF embarqué |
| **Table ACPI** | Non fournie (scan nécessaire) | **Fournie via `EFI_CONFIGURATION_TABLE`** |
| **Sortie Console Débogage** | BIOS INT 10h ou port série direct | `EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL` et port série direct |

**Conclusion architecturale :**  
L'état fourni par le firmware UEFI x86_64 est considérablement plus proche de l'état attendu par `kernel_main` que le mode réel BIOS 16 bits. Le chargeur UEFI n'a pas besoin de trampoline de mode protégé ni de commutation de mode CPU. Il a uniquement besoin de :
1. Allouer et copier les segments de l'`aura-kernel` à l'adresse 1 MiB (`0x100000`).
2. Récupérer le pointeur ACPI RSDP.
3. Récupérer la carte mémoire et appeler `ExitBootServices`.
4. Convertir la carte mémoire en `[MemoryRegion]`.
5. S'assurer d'une table des pages identité cohérente couvrant le noyau et le PMM (`0..512 MiB` + MMIO `3..4 GiB`).
6. Transférer le contrôle à `kernel_main(&boot_info)` via l'ABI System V.

---

## 5. Invariants de Sécurité et Garanties de Non-Régression

1. **Isolation et Zéro Dépendance** :
   - Le noyau `src/` ne référence aucune crate UEFI.
   - Le chargeur `boot/uefi/` définit ses types ABI UEFI à la main en Rust `#![no_std]`, respectant la contrainte stricte de zéro dépendance externe.
2. **Préservation Intégrale du Boot BIOS** :
   - La fonction `_start` reste présente pour le chemin `bootloader 0.9.23`. Elle effectue la conversion en microsecondes et appelle `kernel_main`.
   - `run_qemu.sh` conserve son comportement exact et continue de valider les 47 tests sur 2 et 4 cœurs.
3. **PMM Stable et Équilibré** :
   - Le PMM traite la carte mémoire unifiée sans altération de ses compteurs (`total_memory`, `free_memory`).
   - La mémoire déclarée comme `EfiBootServicesCode`/`Data` ou `EfiConventionalMemory` est reconnue comme utilisable après `ExitBootServices`.

---

## 6. Rapport d'Implémentation et Validation Matérielle

### 6.1 Synthèse de l'Implémentation Réalisée
1. **Module de Convergence (`src/boot/`)** :
   - `BootInfo`, `BootMethod` (`Bios`, `Uefi`), `MemoryRegion`, `MemoryRegionKind` définis avec zéro dépendance.
   - Constante magique de validation `BOOT_INFO_MAGIC = 0x41555241_5F4F5321` (`"AURA_OS!"`).
   - Adaptateur BIOS transparent (`src/boot/bios.rs`) convertissant la carte mémoire `bootloader` E820.
   - PMM découplé de la crate `bootloader` et initialisé à partir de `&[MemoryRegion]`.
   - Point d'entrée `_start` avec détection dynamique de signature pour acheminer directement vers `kernel_main`.
   - ACPI étendu avec `init_with_rsdp(rsdp_override: Option<u64>)` permettant l'injection directe de l'adresse RSDP découverte par l'UEFI.

2. **Bootloader UEFI Autonome (`boot/uefi/`)** :
   - Crate indépendante `#![no_std]` compilée pour la cible `x86_64-unknown-uefi`.
   - Définitions complètes de l'ABI UEFI 2.x (tables système, boot services, types mémoire, GUIDs ACPI).
   - Chargeur ELF64 interne copiant les segments `PT_LOAD` à `0x100000` et initialisant le BSS.
   - Construction des tables de pages 4 niveaux (`0..512 MiB` + MMIO `3..4 GiB`), transition hors des boot services et saut vers le noyau avec `%rdi` pointant vers la structure unifiée `BootInfo`.

3. **Banc de Tests Étendu (Tests #48 à #50)** :
   - **Test 48** : `UEFI Boot Path Reaches kernel_main & BootMethod Detection` (valide l'arrivée dans `kernel_main`, la signature magique et le boot method actif).
   - **Test 49** : `Unified Memory Map Consistency & PMM Initialization` (valide l'absence de chevauchement, la présence de mémoire utilisable et la cohérence PMM `total_memory > 0`, `free_memory <= total_memory`).
   - **Test 50** : `ACPI/MADT via UEFI RSDP & SMP Core Enumeration` (valide la résolution du RSDP via table EFI ou EBDA, la découverte de la MADT et l'énumération multi-cœurs).

4. **Automatisation QEMU** :
   - Script exécutable `run_qemu_uefi.sh` assurant la compilation release, la préparation de l'ESP FAT et le lancement sous QEMU avec firmware OVMF (`/usr/share/edk2/x64/OVMF.4m.fd`).

### 6.2 Résultats d'Exécution Matérielle

#### A. Démarrage BIOS Legacy (`./run_qemu.sh --nographic -smp 2` et `-smp 4`)
```text
  [PASS] Page Fault on Illegal Access Kills Task, Not Kernel
  [PASS] UEFI Boot Path Reaches kernel_main & BootMethod Detection
         Detail: Method=Bios, MagicMatch=true, MapNonEmpty=true, RegionsCount=22
  [PASS] Unified Memory Map Consistency & PMM Initialization
         Detail: Regions=22, Usable=1, NonOverlap=true, TotalMem=244MiB, FreeMem=244MiB
  [PASS] ACPI/MADT via UEFI RSDP & SMP Core Enumeration
         Detail: BootMethod=Bios, RSDP=0xf64f0, MADT=0xffe22a4, Cores=2, BSP_En=true (Cores=4 en -smp 4)
[OK] All 50 subsystem tests PASSED. System verified 100%.
[OK] Boot Mode : BIOS / Legacy (Unified BootInfo)
[OK] RAM       : Physical RAM 244 MiB detected (E820), 244 MiB free frames via PMM
```

#### B. Démarrage Natif UEFI (`./run_qemu_uefi.sh --nographic -smp 2` et `-smp 4`)
```text
  [PASS] Page Fault on Illegal Access Kills Task, Not Kernel
  [PASS] UEFI Boot Path Reaches kernel_main & BootMethod Detection
         Detail: Method=Uefi, MagicMatch=true, MapNonEmpty=true, RegionsCount=98
  [PASS] Unified Memory Map Consistency & PMM Initialization
         Detail: Regions=98, Usable=84, NonOverlap=true, TotalMem=249MiB, FreeMem=230MiB
  [PASS] ACPI/MADT via UEFI RSDP & SMP Core Enumeration
         Detail: BootMethod=Uefi, RSDP=0xf77e014, MADT=0xf778000, Cores=2, BSP_En=true (Cores=4 en -smp 4)
[OK] All 50 subsystem tests PASSED. System verified 100%.
[OK] Boot Mode : Native UEFI (Unified BootInfo)
[OK] RAM       : Physical RAM 249 MiB detected (UEFI), 230 MiB free frames via PMM
```

**Verdict :** Les deux chemins d'amorçage coexistent harmonieusement sans interférence, atteignent `kernel_main` et passent **50/50** auto-tests avec **100% de succès** sur 2 et 4 cœurs.
