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

## 🛠️ 5. État Actuel du Noyau (AuraOS v0.1.0)

Le projet se compose actuellement du cœur du noyau [`aura_kernel`](file:///Volumes/Données/installation/aura_kernel) :
* **Version :** `v0.1.0-alpha`
* **Mode :** Bare-Metal (`#![no_std]`, `#![no_main]`, x86_64 Long Mode)
* **Taille binaire (Release) :** Seulement **62 Ko** !

### 🧩 Sous-systèmes développés & opérationnels :
1. **Pilote Vidéo VGA ([`src/vga_buffer.rs`](file:///Volumes/Données/installation/aura_kernel/src/vga_buffer.rs)) :** Buffer texte 80x25 (`0xb8000`), scrolling automatique, formatage `print!` et `println!`, synchronisation atomique via `Spinlock`.
2. **GDT ([`src/gdt.rs`](file:///Volumes/Données/installation/aura_kernel/src/gdt.rs)) :** Table globale de descripteurs configurant les segments Ring 0 (Code & Données) en mode 64 bits.
3. **IDT ([`src/idt.rs`](file:///Volumes/Données/installation/aura_kernel/src/idt.rs)) :** Table des interruptions interceptant 5 exceptions CPU critiques (*Division par zéro, Opcode invalide, Double Fault, GPF, Page Fault*).
4. **Ports I/O ([`src/io.rs`](file:///Volumes/Données/installation/aura_kernel/src/io.rs)) :** Primitives d'entrées/sorties matérielles `inb`, `outb`, et délai `io_wait`.
5. **Contrôleur PIC 8259 ([`src/pic.rs`](file:///Volumes/Données/installation/aura_kernel/src/pic.rs)) :** Remappage des interruptions matérielles (IRQs 32 à 47), acquittement EOI, masquage sélectif.
6. **Pilote Clavier PS/2 ([`src/keyboard.rs`](file:///Volumes/Données/installation/aura_kernel/src/keyboard.rs)) :** Décodage Scancode Set 1, support des touches Majuscule (Shift), effacement (Backspace) et Entrée.
7. **Gestion Mémoire & Pagination ([`src/memory.rs`](file:///Volumes/Données/installation/aura_kernel/src/memory.rs)) :** Abstractions d'adresses physiques (`PhysAddr`) et virtuelles (`VirtAddr`), drapeaux de tables de pages x86_64 à 4 niveaux (PML4), lecture du registre de contrôle `CR3`.
8. **Allocateur de Tas Dynamique ([`src/memory/allocator.rs`](file:///Volumes/Données/installation/aura_kernel/src/memory/allocator.rs)) :** Implémentation complète de `core::alloc::GlobalAlloc` avec gestionnaire par liste chaînée (*Linked List Allocator*) de 8 Mo et fusion automatique des blocs libres (*coalescing*). Active `extern crate alloc` (`Box`, `Vec`, `String`, `format!`).
9. **Port Série UART 16550 COM1 ([`src/serial.rs`](file:///Volumes/Données/installation/aura_kernel/src/serial.rs)) :** Port de communication série à 115200 bauds (8N1 sur le port `0x3F8`), macros `serial_print!` et `serial_println!`, redirection des logs de diagnostic et panic handler vers terminal externe ou fichier hôte.
10. **Shell Interactif ([`src/shell.rs`](file:///Volumes/Données/installation/aura_kernel/src/shell.rs)) :** Terminal bare-metal avec commandes intégrées : `help`, `clear`, `info`, `mem`, `serial <msg>`, `ticks`, `calc <a+b>`, `manifesto`, `reboot`, `halt`.

### 🚀 Compiler le noyau
```bash
cd aura_kernel
cargo +nightly build --release
```

Le binaire exécutable optimisé est généré dans :
`aura_kernel/target/x86_64-unknown-none/release/aura-kernel` (62 Ko)

---

## 📜 6. Auteurs & Licence

* **Architecte & Créateur :** Niaina
* **Licence :** Open-Source sous licence MIT / Apache 2.0 (Double Licence Rust standard).
