## Mission : Implémenter I4 — SMP Multi-Core dans AuraOS

Tu es un ingénieur senior spécialisé en développement de noyaux (Kernel Development), Rust `no_std`, architecture x86_64 et systèmes multiprocesseurs (SMP).

### Contexte du projet

AuraOS v0.1.0-alpha dispose déjà d'une première implémentation SMP :

* Détection des CPU via ACPI/MADT.
* Réveil des Application Processors (APs) avec INIT-SIPI-SIPI.
* Structure `PerCpu` avec GDT, TSS, IST et stacks dédiées.
* Initialisation du LAPIC sur chaque AP.
* Configuration des MSRs syscall par CPU.
* Les APs démarrent et restent actuellement en boucle `HLT`.
* Le scheduler est encore exclusivement exécuté sur le BSP.
* Les tâches ne sont pas encore distribuées entre les différents cœurs.

**Objectif : finaliser I4 et transformer AuraOS en un véritable kernel SMP multi-cœur fonctionnel.**

### Consigne principale

Analyse d'abord le code source réel du projet afin de comprendre l'architecture existante, les mécanismes de boot, le scheduler, les interruptions, les structures `Task/TCB`, le LAPIC et les données per-CPU.

Ne fais pas de long blabla théorique. Travaille directement sur le code.

### Plan d'implémentation obligatoire

#### 1. Audit de l'existant

* Identifier précisément les fichiers et modules concernés.
* Vérifier l'initialisation actuelle du BSP et des APs.
* Analyser le scheduler préemptif existant.
* Identifier les ressources globales non compatibles SMP.
* Vérifier les risques de data races, deadlocks et accès concurrents.
* Définir une stratégie de migration sans casser les fonctionnalités déjà opérationnelles.

#### 2. Scheduler per-CPU

Implémenter un scheduler indépendant pour chaque CPU :

* Une run queue par CPU.
* Attribution et suivi des tâches par CPU.
* Support du réveil et de l'endormissement des tâches.
* Context switching compatible SMP.
* Gestion correcte des tâches mortes et du reaping.
* Éviter qu'une même tâche s'exécute simultanément sur plusieurs CPU.
* Préserver le fonctionnement actuel du scheduler BSP.

#### 3. LAPIC Timer per-CPU

Mettre en place un timer local pour chaque CPU :

* Initialisation du LAPIC Timer sur le BSP et chaque AP.
* Configuration des interruptions timer indépendantes.
* Préemption indépendante sur chaque cœur.
* Gestion correcte des ticks et du quantum.
* Éviter les conflits avec le PIT global existant.
* Conserver une comptabilité temporelle cohérente.

#### 4. Distribution des tâches entre CPU

Implémenter une stratégie de répartition :

* Affectation initiale des tâches aux CPU disponibles.
* Répartition équilibrée de la charge.
* Migration des tâches entre CPU si nécessaire.
* Work stealing ou load balancing selon ce qui est adapté à l'architecture actuelle.
* Éviter les migrations dangereuses pendant un context switch.
* Garantir la cohérence des états `Ready`, `Running`, `Sleeping`, `Dead` et `Reaped`.

#### 5. Synchronisation SMP

Auditer et adapter les mécanismes de synchronisation :

* Spinlocks compatibles multi-cœurs.
* Protection des structures globales.
* Gestion des interruptions pendant la prise de locks.
* Utilisation appropriée des atomics.
* Prévention des deadlocks.
* Vérification des accès aux données per-CPU.
* Respect des contraintes Rust `no_std`.

#### 6. Syscalls et mémoire

Vérifier la compatibilité SMP des mécanismes existants :

* MSRs syscall indépendants par CPU.
* Scratchs syscall per-CPU.
* RSP0 et TSS par CPU.
* Context switching avec CR3.
* Isolation Ring 0 / Ring 3.
* Gestion des AddressSpace lors de l'exécution concurrente.

#### 7. Tests et validation

Ajouter et exécuter des tests réels :

* Détection de tous les CPU.
* Vérification que chaque AP démarre correctement.
* Vérification de l'initialisation du scheduler sur chaque CPU.
* Exécution simultanée de plusieurs tâches sur différents cœurs.
* Test de préemption multi-cœur.
* Test de migration des tâches.
* Test de synchronisation et absence de corruption mémoire.
* Test de reaping concurrent.
* Test de stabilité sous charge.

Utiliser QEMU avec plusieurs CPU (`-smp N`) et vérifier les logs série.

### Contraintes strictes

* Utiliser exclusivement Rust `no_std`.
* Ne pas remplacer inutilement l'architecture existante.
* Ne pas supprimer les fonctionnalités déjà opérationnelles.
* Ne pas utiliser de fausses implémentations ou de simples stubs.
* Ne pas masquer les erreurs de compilation ou les tests échoués.
* Respecter l'architecture modulaire du projet.
* Documenter les décisions techniques importantes.
* Ne pas introduire de dépendances externes inutiles.
* Ne pas considérer I4 comme terminé tant que les tâches ne s'exécutent pas réellement sur plusieurs CPU.

### Méthode de travail

1. Analyser le code source réel.
2. Établir un plan technique précis.
3. Implémenter directement les corrections nécessaires.
4. Compiler et corriger les erreurs.
5. Exécuter les tests automatisés.
6. Tester le boot SMP dans QEMU.
7. Vérifier les logs et les résultats.
8. Corriger les bugs détectés.
9. Fournir un rapport final concis.

### Résultat attendu

À la fin, AuraOS doit disposer d'un **SMP multi-cœur réellement fonctionnel**, avec :

* APs initialisés.
* Scheduler per-CPU opérationnel.
* LAPIC Timer par CPU.
* Exécution réelle de tâches sur plusieurs cœurs.
* Synchronisation SMP robuste.
* Tests automatisés validant le fonctionnement.
* Documentation technique mise à jour.

### Rapport final obligatoire

Présenter uniquement :

1. Les fichiers modifiés.
2. Les fonctionnalités implémentées.
3. Les tests exécutés et leurs résultats.
4. Les problèmes restant à résoudre, s'il y en a.
5. Le statut réel de I4 : terminé ou partiel.

**Commence maintenant par l'analyse du code réel, puis implémente directement I4 sans long discours.**
