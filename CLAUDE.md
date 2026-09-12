# Prompt technique — Correction de I1 : Élimination du mono-rédacteur global (SERIAL / Shell / VFS) dans AuraOS

## Rôle

Tu es l'ingénieur noyau chargé de faire évoluer **AuraOS** (OS x86_64 bare-metal, Rust `no_std`, zéro dépendance externe, SMP réel 2-4 cœurs). Le noyau compile actuellement avec `cargo +nightly build` en `0 erreur / 0 warning` et passe **37/37** auto-tests matériels à chaque boot (y compris `-smp 2` et `-smp 4`). Tu dois livrer une modification qui ne casse **aucune** de ces garanties.

## Contexte du problème (I1)

Le SMP est réel : plusieurs cœurs exécutent des tâches en parallèle (scheduler per-CPU, run queues per-CPU, work stealing, reaping cross-core validé). Cependant, trois ressources globales restent **single-writer** et protégées par un **verrou global unique** :

1. **SERIAL** (sortie série `serial.rs`) — un seul spinlock protège tout le flux d'écriture.
2. **Shell interactif** — état/édition de ligne partagé, pas de séparation par cœur/session.
3. **VFS centralisé** (`fs/mod.rs`) — inodes, `/proc`, `/dev` accessibles via un verrou global unique, quel que soit le chemin ou l'inode ciblé.

**Impact mesuré/attendu :** sous charge multi-cœur (`-smp 4`), toute tâche sur n'importe quel cœur qui écrit sur la sortie série, interagit avec le shell, ou touche au VFS, doit acquérir le **même** verrou que toutes les autres — créant de la contention, un point de sérialisation artificiel, et un risque de priority inversion / starvation à mesure que le nombre de cœurs et de tâches augmente.

## Objectif de la mission

Remplacer le modèle "un seul verrou global par ressource" par un modèle **multi-rédacteurs à contention réduite, adapté par-CPU**, pour SERIAL, Shell, et VFS — **sans changer le comportement observable** des tests existants, et en ajoutant les preuves matérielles (nouveaux auto-tests) qui démontrent la réduction de contention.

---

## Contraintes non négociables

- **Zéro dépendance externe.** Pas de crate `spin`, `lock_api`, `crossbeam`, etc. Tout doit être écrit à la main en `no_std`.
- **Zéro warning.** `cargo +nightly build` doit rester `exit 0`, 0 erreur, 0 warning. Ne pas masquer avec `#[allow(dead_code)]` du code réellement mort.
- **Les 37 tests existants doivent rester au vert**, à l'identique, sur `-smp 2` et `-smp 4`.
- **Pas de régression de sécurité mémoire.** Tout `unsafe` ajouté doit être justifié par un commentaire expliquant l'invariant garanti (alignement, exclusivité, visibilité inter-cœur).
- **IRQ-safety conservée.** Comme les spinlocks actuels, toute nouvelle primitive de synchronisation doit désactiver localement les interruptions (`cli`/`sti` restauré) pendant la section critique si elle peut être prise depuis un handler ou préemptée par le timer LAPIC.
- **Pas de dépendance à x2APIC ou à du matériel non disponible sous QEMU.** Reste compatible avec l'environnement de validation actuel (`run_qemu.sh --smp`).

---

## Diagnostic préalable (à produire avant toute modification)

Avant d'écrire du code, produis un état des lieux court (commentaire de PR ou fichier `docs/I1_DIAGNOSTIC.md`) qui :

1. Liste tous les points d'appel actuels vers le verrou SERIAL, le verrou VFS, et l'état partagé du shell (grep exhaustif, avec fichier + ligne).
2. Identifie, pour chacun, si l'accès est en lecture, écriture, ou lecture-modification-écriture.
3. Identifie les invariants actuellement garantis par le verrou global (ex : ordre d'affichage série non entrelacé caractère par caractère) qui **doivent être préservés** même après la parallélisation.

---

## Spécifications techniques par ressource

### 1. SERIAL — passage à un buffer par-CPU + rédacteur unique en sortie

**Problème actuel :** chaque écriture série depuis n'importe quel cœur prend le même spinlock global, caractère par caractère ou message par message.

**Solution attendue :**

- Créer un **buffer circulaire (ring buffer) par-CPU** en mémoire (taille fixe, ex. 4 KiB), rempli **sans verrou global** — uniquement une opération atomique locale (compteur de tête/queue per-CPU) pour gérer le wrap-around.
- Un **unique "flusher"** (soit le BSP à intervalle régulier via le timer LAPIC, soit un cœur désigné) draine les buffers per-CPU dans l'ordre round-robin et écrit sur le port série physique, qui reste par nature un périphérique unique.
- Le port physique lui-même garde un verrou, mais celui-ci n'est plus jamais contesté par les producteurs — seulement par le flusher, donc contention proche de zéro.
- Gérer explicitement le cas de **buffer plein** (politique : écraser le plus ancien message avec un marqueur `[DROPPED n]`, plutôt que de bloquer un cœur producteur).
- Garantir que les messages d'un même cœur restent **dans l'ordre d'émission** (FIFO local), même si l'entrelacement entre cœurs différents n'est pas garanti à la milliseconde près.

### 2. Shell interactif — session/état par-CPU avec verrou fin sur la ligne de commande

**Problème actuel :** l'état d'édition de ligne (buffer de saisie, curseur, historique) est une structure globale unique.

**Solution attendue :**

- Séparer clairement **l'état d'entrée clavier/édition** (qui n'a de sens que sur le cœur/console qui possède le focus interactif — typiquement le BSP, car un seul clavier/écran) de **l'exécution des commandes**, qui elle peut être distribuée.
- Le verrou global du shell doit être réduit au **strict minimum de la zone critique** : uniquement pendant la mutation du buffer de ligne, pas pendant l'exécution complète d'une commande (`ps`, `mem`, `tasks`, etc.), qui elle-même peut lire des structures per-CPU sans bloquer la saisie clavier suivante.
- Si des commandes shell peuvent être lancées en tâche de fond sur un autre cœur (ex. `exec` asynchrone), leur sortie doit transiter par le nouveau pipeline SERIAL par-CPU décrit ci-dessus, pas par un accès direct concurrent à la ligne en cours d'édition.

### 3. VFS — verrouillage par inode/sous-arbre au lieu d'un verrou global

**Problème actuel :** toute opération VFS (lookup, read, write, readdir, création de fichier, génération dynamique `/proc/*`) prend le même verrou global, quel que soit le chemin touché.

**Solution attendue :**

- Passer d'un **unique `Mutex`/spinlock global sur le VFS** à un **verrou par inode** (ou au minimum par sous-arbre/répertoire), de sorte que deux opérations sur des chemins disjoints (ex. lecture de `/proc/meminfo` sur CPU#0 et écriture dans `/tmp/foo` sur CPU#1) ne s'attendent plus.
- Conserver un verrou global léger uniquement pour les opérations qui modifient la **topologie** de l'arbre (création/suppression d'inode, renommage), car celles-ci doivent rester atomiques vis-à-vis de toute traversée de chemin concurrente. Ce verrou global doit être tenu le moins longtemps possible (jamais pendant une lecture de contenu de fichier).
- Pour les pseudo-filesystems dynamiques (`/proc/uptime`, `/proc/meminfo`, `/proc/cpuinfo`, `/proc/tasks`), s'assurer que leur génération lit des données **elles-mêmes déjà per-CPU ou atomiques** (compteurs de tâches, PMM, etc.), pour ne jamais avoir besoin de prendre le verrou global d'inode juste pour produire ces vues.
- Documenter explicitement l'ordre d'acquisition des verrous (inode parent avant inode enfant, jamais l'inverse) pour exclure tout deadlock, et ajouter un commentaire de garantie dans le code.

---

## Plan d'implémentation recommandé (par étapes, chaque étape doit compiler et passer les 37 tests)

1. **Étape 0 — Diagnostic** : produire `docs/I1_DIAGNOSTIC.md` (voir section ci-dessus). Aucun changement de code.
2. **Étape 1 — SERIAL per-CPU** : implémenter les ring buffers per-CPU + flusher unique. Ajouter un test dédié (voir plus bas). Vérifier zéro régression sur les logs de boot existants (le format `[OK]`/`[FAIL]` doit rester lisible et dans l'ordre par cœur).
3. **Étape 2 — VFS par inode** : introduire le verrou par inode, migrer les opérations une par une (lookup, read, write, readdir), en gardant temporairement le verrou global uniquement pour les mutations topologiques. Ajouter un test de concurrence VFS.
4. **Étape 3 — Shell** : réduire la portée du verrou du shell à la zone d'édition de ligne uniquement. Ajouter un test qui vérifie qu'une commande longue sur un cœur n'empêche pas la saisie/traitement clavier.
5. **Étape 4 — Nettoyage & documentation** : mettre à jour `docs/AVANTAGES_ET_INCONVENIENTS.md` (ou équivalent) pour refléter que I1 est résolu, avec les nouvelles preuves matérielles.

Chaque étape doit se terminer par : `cargo +nightly build` (0 warning) + `run_qemu.sh --smp 2` et `--smp 4` (37+ tests PASS).

---

## Nouveaux auto-tests à ajouter (numérotés à partir de #38)

- **Test #38 — "SERIAL Per-CPU Buffer, No Cross-Core Blocking"** : depuis 2+ cœurs, générer un flot d'écritures série simultané et vérifier (a) qu'aucun message n'est corrompu/entrelacé caractère par caractère, (b) que le compteur de contention/attente sur le verrou du port physique reste proche de zéro comparé à l'ancien modèle (mesure de cycles ou de compteur d'itérations de spin).
- **Test #39 — "VFS Concurrent Disjoint Path Access"** : CPU#0 lit `/proc/meminfo` en boucle pendant que CPU#1 écrit dans un fichier sous un autre sous-arbre ; vérifier l'absence de blocage croisé (mesure de latence) et l'absence de corruption des deux côtés.
- **Test #40 — "VFS Topology Lock Correctness"** : création/suppression concurrente d'inodes sur deux cœurs différents dans des répertoires différents ; vérifier absence de deadlock et cohérence finale de l'arbre (pas d'inode orpheline, pas de double libération).
- **Test #41 — "Shell Responsiveness Under Background Load"** : lancer une commande longue (ou une tâche de fond équivalente) sur un cœur pendant que le buffer d'édition de ligne du shell reste manipulable sans latence anormale sur un autre chemin de code.

Chaque test doit suivre le format existant (`[PASS]`/`[FAIL]` en série, intégré à la suite lancée au boot).

---

## Critères de validation finale

- [ ] `cargo +nightly build` → exit 0, 0 erreur, 0 warning.
- [ ] 37 tests existants toujours PASS sur `-smp 2` et `-smp 4`, comportement inchangé.
- [ ] 4 nouveaux tests (#38-#41) PASS sur `-smp 2` et `-smp 4`.
- [ ] Aucun nouveau `Mutex`/spinlock global introduit pour SERIAL ou VFS (sauf le verrou topologique VFS explicitement justifié et documenté).
- [ ] Documentation des invariants de verrouillage (ordre d'acquisition, sections critiques) ajoutée en commentaires dans le code source, pas seulement en Markdown externe.
- [ ] Mise à jour de la table de statut (section I1) dans le document d'analyse globale pour refléter le passage de "constat/limite" à "corrigé, preuve matérielle #38-#41".

## Format de livraison attendu

- Diff de code complet, organisé par étape (0 à 4), chaque étape dans un commit logique séparé si possible.
- `docs/I1_DIAGNOSTIC.md` (étape 0).
- Mise à jour de la suite de tests avec les 4 nouveaux tests et leurs logs de sortie série capturés comme preuve.
- Un court résumé final (10-15 lignes) expliquant, ressource par ressource, ce qui a changé et pourquoi la contention est réduite — dans le même style factuel/byte-authoritatif que le reste de la documentation AuraOS (verdicts compilateur + verdicts matériels, pas d'affirmation non prouvée).
