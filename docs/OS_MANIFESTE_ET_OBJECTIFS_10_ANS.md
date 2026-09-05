# 🌌 MANIFESTE & CAHIER DES CHARGES : PROJET OS (HORIZON 10 ANS)

> **Nom de code du projet :** `AuraOS` *(ou le nom de votre choix)*  
> **Auteur & Architecte :** Vous  
> **Horizon temporel :** 2026 – 2036 (Feuille de route sur 10 ans)  
> **Langage principal :** 100% Pur Rust (`#![no_std]`)  
> **Architectures matérielles cibles :** `x86_64` (Intel/AMD) puis `aarch64` (ARM64)  

---

## 1. 🏛️ Philosophie et Vision Fondatrice

Le projet **AuraOS** a pour ambition de concevoir, brique par brique, un système d'exploitation indépendant, moderne, ultra-performant et mathématiquement sécurisé.

### Les 4 Piliers Inviolables :
1. **Zéro Compromis Mémoire (Pure Rust) :** 
   Éliminer définitivement la classe des 70% de vulnérabilités critiques du C/C++ (Buffer Overflow, Use-After-Free, Deadlocks de mémoire) grâce au typage strict et au *Borrow Checker* de Rust.
2. **Architecture Micro-Noyau Isolée :** 
   Le noyau ne contient que le strict nécessaire (gestion des cœurs CPU, mémoire virtuelle et communication inter-processus). Les pilotes (clavier, disque, affichage) s'exécutent en espace utilisateur (*Ring 3*). Si un pilote plante, l'OS continue de tourner sans écran bleu ni panique noyau.
3. **Légèreté Radicale & Zéro Dette Technique :** 
   Pas de code hérité vieux de 30 ans. Démarrage instantané en moins d'une seconde, consommation de RAM au repos inférieure à 100 Mo, et taille totale sur disque inférieure à 1 Go pour le système complet avec bureau graphique.
4. **Auto-Suffisance (*Self-Hosting*) à Terme :** 
   Le but ultime de la décennie est que le système soit capable de se recompiler lui-même, directement dans son propre environnement.

---

## 2. 🏗️ Architecture Technique Cible

```
┌────────────────────────────────────────────────────────────────────────┐
│                        ESPACE UTILISATEUR (RING 3)                     │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────────┐  │
│  │ Navigateur / Apps│  │  Bureau Graphique│  │ Shell & Utilitaires  │  │
│  └────────┬─────────┘  └────────┬─────────┘  └──────────┬───────────┘  │
│           │                     │                       │              │
│  ┌────────▼─────────────────────▼───────────────────────▼───────────┐  │
│  │              SERVICES SYSTÈME & PILOTES ISOLÉS                   │  │
│  │  (Pilote Audio, Pile Réseau TCP/IP, Pilote Disque NVMe/SATA, VFS)│  │
│  └──────────────────────────────┬───────────────────────────────────┘  │
└─────────────────────────────────┼──────────────────────────────────────┘
                                  │ IPC (Appels Système / Messages)
┌─────────────────────────────────▼──────────────────────────────────────┐
│                         ESPACE NOYAU (RING 0)                          │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │   • Gestionnaire de Pagination Mémoire (Virtual Memory Paging)   │  │
│  │   • Ordonnanceur Préemptif Multitâche (Scheduler Multi-Cœurs)    │  │
│  │   • Table des Descripteurs d'Interruptions (IDT / GDT)           │  │
│  │   • Bus IPC à Très Haute Vitesse (Zero-Copy Shared Memory)       │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────┘
                                    │
                         MATÉRIEL PHYSIQUE / CPU
```

---

## 3. 🗺️ La Feuille de Route Décennale (2026 – 2036)

### 🔹 Phase 1 : Le Cœur Fondateur (Années 1 à 2)
* **Objectif :** Démarrage autonome, mémoire protégée, affichage texte et ligne de commande.
* **Livrables :**
  - Chaîne de démarrage (*Bootloader*) compatible UEFI et BIOS.
  - Pilote vidéo VGA & Framebuffer pour l'affichage à l'écran.
  - Gestion des interruptions matérielles (clavier PS/2 et horloge PIT).
  - Gestionnaire de mémoire virtuelle (Pagination x86_64 à 4 niveaux).
  - Allocateur de mémoire dynamique (*Heap Allocator*) pour activer `Vec`, `String`, `Box`.
  - Shell interactif avec commandes système intégrées (`sysinfo`, `mem`, `clear`, `test`).

### 🔹 Phase 2 : Le Multitâche & Le Matériel Réel (Années 3 à 4)
* **Objectif :** Exécution concurrente sécurisée et support des disques physiques.
* **Livrables :**
  - Ordonnanceur multitâche préemptif (changement de contexte cadencé par interruptions horloge).
  - Séparation stricte entre Espace Noyau (*Ring 0*) et Espace Utilisateur (*Ring 3*).
  - Pilote de bus PCI Express et contrôleur de disque **SATA (AHCI)** et **NVMe**.
  - Système de fichiers virtuel (*VFS - Virtual File System*).
  - Lecture et écriture de fichiers réels sur une clé USB ou un SSD.

### 🔹 Phase 3 : La Pile Réseau & Compatibilité POSIX (Années 5 à 6)
* **Objectif :** Ouvrir le système sur le monde extérieur et internet.
* **Livrables :**
  - Pilotes de cartes réseau (Intel e1000 et Realtek).
  - Pile réseau native en Rust : Ethernet, ARP, IPv4/IPv6, UDP, TCP, DHCP, DNS.
  - Couche de compatibilité POSIX de base (permettant de porter des outils comme `bash`, `coreutils` et `git`).
  - Capacité d'exécuter un interpréteur Python ou un serveur Web directement sur l'OS.

### 🔹 Phase 4 : L'Ère Graphique & Multimédia (Années 7 à 8)
* **Objectif :** Sortir du mode texte pour offrir un environnement de bureau moderne.
* **Livrables :**
  - Serveur d'affichage et compositeur graphique fluide (60 FPS avec double-tampon).
  - Toolkit d'interface graphique (boutons, fenêtres déplaçables, polices vectorielles TrueType).
  - Pilote de souris USB / pavé tactile et pilote audio (Intel HD Audio / AC97).
  - Suite logicielle de base : gestionnaire de fichiers, éditeur de texte, visionneuse d'images, terminal graphique.

### 🔹 Phase 5 : L'Émancipation & Écosystème (*Self-Hosting*) (Années 9 à 10)
* **Objectif :** Rendre l'OS 100% autonome et déployable sur du vrai matériel commercial.
* **Livrables :**
  - Portage du compilateur `rustc` et du gestionnaire `cargo` sur l'OS.
  - Navigateur Web léger capable d'afficher les pages modernes et de télécharger des paquets.
  - Portage de l'architecture sur processeurs **ARM64** (Raspberry Pi et serveurs ARM).
  - Création d'un installateur officiel pour installer AuraOS comme système principal sur un PC portable.

---

## 4. 📊 Spécifications et Métriques Cibles

| Métrique | Cible Finale (À 10 Ans) | Référence Windows / macOS |
| :--- | :--- | :--- |
| **Temps de démarrage (Cold Boot)** | ⚡ **< 1 seconde** | 15 à 30 secondes |
| **Consommation RAM au repos** | 💧 **< 150 Mo** (avec bureau) | 3 500 à 4 500 Mo |
| **Taille complète sur disque** | 📦 **< 1 Go** (système complet) | 20 à 45 Go |
| **Crashs mémoire (Segfaults)** | 🛡️ **0 (Garanti par le compilateur)** | Fréquents en C/C++ |
| **Type de Noyau** | 🛡️ **Micro-Noyau Modulaire** | Monolithique / Hybride |

---

## 5. 🚀 Plan d'Action Immédiat (Sprint 1)

Voici les actions concrètes pour lancer les fondations du projet dès maintenant :
1. ✅ **Outil de base :** Valider l'environnement de développement Rust (déjà installé avec succès).
2. 🎯 **Étape A :** Ajouter la cible de compilation Bare-Metal x86_64 (`x86_64-unknown-none`).
3. 🎯 **Étape B :** Créer la structure du projet Cargo avec `#![no_std]` et `#![no_main]`.
4. 🎯 **Étape C :** Développer la fonction `_start()` et écrire les premiers octets dans la mémoire vidéo pour afficher notre premier message dans une machine virtuelle QEMU.

---
*Ce document sert de contrat d'ingénierie et de guide de référence tout au long du développement.*
