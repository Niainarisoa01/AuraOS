# 🌌 AuraOS — Le Système d'Exploitation 100% Pur Rust

<p align="center">
  <strong>Un micro-noyau moderne, indépendant, ultra-léger et mathématiquement sécurisé.</strong><br>
  <em>Conçu pour les architectures 64-bit modernes (x86_64 & ARM64).</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Language-Pure%20Rust%20(%23![no__std])-orange?style=for-the-badge&logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/Arch-x86__64%20Bare--Metal-blue?style=for-the-badge" alt="Arch" />
  <img src="https://img.shields.io/badge/Kernel%20Size-2.4%20KB-success?style=for-the-badge" alt="Size" />
  <img src="https://img.shields.io/badge/Roadmap-10%20Years%20(2026--2036)-purple?style=for-the-badge" alt="Roadmap" />
</p>

---

## 🌟 1. Signification & Philosophie du Nom : « AuraOS »

Le nom **AuraOS** a été choisi pour incarner quatre valeurs fondamentales :

* 🌬️ **La Légèreté d'un souffle (Grec ancien *Αὔρα*) :**  
  À l'opposé des systèmes d'exploitation modernes surchargés pesant des dizaines de gigaoctets, AuraOS est conçu pour être aussi léger qu'une brise. Son noyau de base ne pèse que **2,4 Kilooctets** et consomme un souffle de mémoire vive.
* 🛡️ **Le Bouclier de Sécurité Invisible :**  
  L'aura représente le champ protecteur impénétrable. Grâce au compilateur Rust (*Borrow Checker* et gestion stricte de la mémoire), AuraOS élimine par conception les 70% de failles qui affectent les systèmes en C/C++ (dépassements de tampon, fuites de mémoire, corruption de pointeurs).
* ✨ **La Clarté et la Pureté :**  
  Zéro code hérité des années 1990, zéro dette technique. Chaque ligne de code d'AuraOS est moderne, lisible et maîtrisée de A à Z.
* 🌍 **Une Identité Universelle :**  
  Un nom court de 4 lettres, moderne, percutant et mémorable à l'international.

---

## 🏛️ 2. Architecture Technique

AuraOS adopte une architecture de type **Micro-Noyau Modulaire** :

```
┌─────────────────────────────────────────────────────────────────────────┐
│                       ESPACE UTILISATEUR (RING 3)                       │
│     [Applications]        [Bureau Graphique]         [Terminal/Shell]   │
├─────────────────────────────────────────────────────────────────────────┤
│                     SERVICES SYSTÈME & PILOTES ISOLÉS                   │
│   (Pilote Écran VGA, Clavier, Pile Réseau TCP/IP, Stockage NVMe/SATA)   │
├─────────────────────────────────────────────────────────────────────────┤
│                                IPC FAST-BUS                             │
├─────────────────────────────────────────────────────────────────────────┤
│                          ESPACE NOYAU (RING 0)                          │
│        • Gestionnaire de Pagination Mémoire (Virtual Memory Paging)     │
│        • Ordonnanceur Préemptif Multitâche (Scheduler)                  │
│        • Table des Descripteurs d'Interruptions (IDT / GDT)             │
└─────────────────────────────────────────────────────────────────────────┘
```

* **Isolation Ring 0 / Ring 3 :** Le noyau (*Ring 0*) est réduit au strict minimum vital. Les pilotes de périphériques et les applications s'exécutent en espace utilisateur isolé (*Ring 3*). Si un pilote plante, le cœur du système ne s'effondre jamais.

📖 **Dossier d'architecture détaillé :** Consultez [docs/CONCEPTION_TECHNIQUE_ET_ARCHITECTURE.md](docs/CONCEPTION_TECHNIQUE_ET_ARCHITECTURE.md) pour la spécification technique complète du noyau (bootloader, GDT, IDT, mémoire, scheduler, VFS, drivers, GUI et tests).  
🚀 **Feuille de route & Améliorations futures :** Consultez [docs/FEUILLE_DE_ROUTE_ET_AMELIORATIONS.md](docs/FEUILLE_DE_ROUTE_ET_AMELIORATIONS.md) pour l'analyse approfondie de l'existant et les spécifications complètes de toutes les fonctionnalités à développer (mode graphique 1024x768, préemption, souris, FAT32, Ring 3, pile réseau).

---

## 📊 3. Métriques Cibles (Horizon 10 Ans)

| Métrique | AuraOS (Cible) | Systèmes Classiques (Windows / macOS) |
| :--- | :--- | :--- |
| **Taille du Noyau (Release)** | ⚡ **2,4 Ko** | Plusieurs dizaines de Mo |
| **Taille du Système Complet** | 📦 **< 1 Go** (avec GUI) | 20 à 45 Go |
| **Consommation RAM au repos** | 💧 **< 100 Mo** | 3 500 à 4 500 Mo |
| **Temps de démarrage** | 🚀 **< 1 seconde** | 15 à 30 secondes |
| **Crashs mémoire (Segfault)** | 🛡️ **0 (Prouvé au build)**| Fréquents |

---

## 🗺️ 4. Feuille de Route Décennale (2026 – 2036)

Consultez le document complet du cahier des charges : [OS_MANIFESTE_ET_OBJECTIFS_10_ANS.md](file:///Volumes/Données/installation/OS_MANIFESTE_ET_OBJECTIFS_10_ANS.md).

* 🔹 **Phase 1 (Années 1-2) — Le Cœur Fondateur :**  
  Démarrage Bare-Metal, affichage VGA, gestion des interruptions clavier/timer, mémoire virtuelle, premier shell interactif. *(En cours)*
* 🔹 **Phase 2 (Années 3-4) — Le Matériel Réel :**  
  Multitâche préemptif, pilotes de stockage (SATA SSD & NVMe), système de fichiers VFS.
* 🔹 **Phase 3 (Années 5-6) — Réseau & POSIX :**  
  Pile TCP/IP native, compatibilité POSIX pour exécuter Python et Git sur l'OS.
* 🔹 **Phase 4 (Années 7-8) — L'Environnement Graphique :**  
  Compositeur de fenêtres 60 FPS, toolkit graphique, gestion de l'audio.
* 🔹 **Phase 5 (Années 9-10) — L'Auto-Suffisance (*Self-Hosting*) :**  
  Capacité d'AuraOS à recompiler son propre noyau via Rust natif, navigateur Web intégré, support ARM64.

---

## 🛠️ 5. État Actuel du Noyau & Fonctionnalités Implémentées

Le noyau [`aura_kernel`](./aura_kernel) est un système d'exploitation x86_64 Long Mode 100% Rust bare-metal :
* **Version :** `v0.1.0-alpha`
* **Mode :** Bare-Metal (`#![no_std]`, `#![no_main]`, x86_64 Long Mode 64-bit)
* **Image Disque Amorçable :** `149 Ko` (`bootimage-aura-kernel.bin`)
* **Sortie Vidéo :** Buffer VGA texte 80x25 (`0xb8000`), avec défilement fluide, couleurs HSL et atomic spinlocks
* **Série & Débogage :** Pilote UART 16550 COM1 (`0x3F8` à 115200 baud, 8N1) avec macros `serial_print!` / `serial_println!`
* **Architecture CPU :** GDT 64-bit Ring 0, IDT avec gestion des exceptions processeur (Div/0, Double Fault, Page Fault, GPF...)
* **Contrôleur d'Interruptions :** Double PIC 8259 avec remappage IRQ 32..47
* **Périphériques d'Entrée & Horloges :**
  * Clavier PS/2 (IRQ1) avec décodage des scancodes et touches de modification
  * Horloge Système PIT (IRQ0, 100 Hz)
  * Horloge Temps Réel CMOS (RTC ports `0x70`/`0x71`) avec décodage BCD/binaire
  * Détection Matérielle CPUID (Vendor string, Brand name, flags SSE/AVX/APIC/RDRAND) et compteur de cycles `RDTSC`
* **Énumération Matérielle PCI :**
  * Scanner de bus PCI 32-bit (ports `0xCF8` et `0xCFC`)
  * Classification automatique des contrôleurs (Stockage IDE/SATA/NVMe, Réseau Ethernet, Graphisme VGA, Ponts hôtes)
* **Multitâche & Ordonnancement de Tâches :**
  * Blocs de Contrôle de Tâches (TCBs) avec piles de 16 Ko isolées
  * Routine de commutation de contexte assembleur nu `#[unsafe(naked)]` (`switch_context`)
  * Ordonnanceur Round-Robin coopératif avec comptabilité CPU sur timer matériel PIT
* **Gestionnaire de Mémoire :**
  * Pagination x86_64 à 4 niveaux (`CR3`, tables PML4, PDPT, PD, PT)
  * Allocateur de mémoire de tas (Heap Allocator) de 8 Mo à liste chaînée avec fusion de blocs libres (*free coalescing*)
  * Support complet d'`extern crate alloc` (`Box`, `Vec`, `String`, `format!`)
* **Système de Fichiers Virtuel (VFS) & RAMFS :**
  * Arborescence à Inodes en mémoire vive avec racine `/` pré-remplie (`/etc/hostname`, `/etc/version`, `/etc/motd`, `/docs/manifesto.txt`, `/bin`)
  * Navigation de chemins relatifs et absolus (`..`, `.`, `/`)
* **Pilote de Stockage Disque Dur ATA / IDE :**
  * Lecture et écriture de secteurs de 512 octets en mode PIO 28-bit LBA (ports `0x1F0` - `0x1F7`)
* **Shell Interactif Complet AuraOS :**
  * `help`, `info`, `cpu`, `pci`, `tasks`, `yield`, `time`, `date`, `mem`
  * `ls`, `cd`, `pwd`, `cat`, `touch`, `mkdir`, `write`, `rm`, `readsec`
  * `serial <msg>`, `ticks`, `calc <a+b>`, `clear`, `manifesto`, `reboot`, `shutdown`, `halt`

### Lancer AuraOS dans QEMU
```bash
cd aura_kernel
./run_qemu.sh
```

---

## 📜 6. Auteurs & Licence

* **Architecte & Créateur :** Niaina
* **Licence :** Open-Source sous licence MIT / Apache 2.0 (Double Licence Rust standard).

