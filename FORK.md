# Fork freemans32/cadcraft — consignes pour les IA

Ce fichier s'adresse à toute IA ou tout agent qui travaille sur ce fork : Claude, Codex, Copilot, Cursor, Gemini, Aider, etc. Il **complète** `AGENTS.md`, qui reste la référence pour le code. En cas de désaccord, la règle la plus stricte s'applique.

Il n'existe que sur les branches personnelles (`perso`, `murs-local`). Il ne doit **jamais** figurer dans une branche destinée à une PR vers `storytold/cadcraft`.

## Qui travaille ici

- Propriétaire : `@freemans32`, technicien et référent technique CVC (climatisation, froid, ventilation).
- Il débute en programmation et veut apprendre. Explique les choix simplement, en français. Montre les commandes qu'il pourra relancer lui-même.
- Pour éditer un fichier, propose `micro` (ou `sudo micro`), pas `nano` ni `vim`.

## Règle n° 1 : valider avant d'agir

Avant toute modification de code, de branche ou de script, présente ce que tu comptes faire et **attends son accord**. Ça vaut aussi pour les commits, les pushs, les PR et les issues. Lire, analyser et compiler pour vérifier ne demande pas d'accord.

## Règle n° 2 : tenir ce fichier à jour, en permanence

À chaque étape terminée (commit, push, décision de Guillaume, question posée, problème rencontré), mets à jour la section « Où nous en sommes » et pousse-la sur `perso` dans la foulée, sans attendre la fin de la session. Une conversation peut s'arrêter à tout moment : ce fichier doit toujours permettre de reprendre là où on en était. Cette mise à jour fait partie de la tâche et ne demande pas d'accord séparé.

## Les branches

| Branche | Rôle | Règles |
|---|---|---|
| `main` | Miroir exact de `storytold/cadcraft` `main` | Jamais de commit. Avance seulement en fast-forward depuis `upstream`. |
| `feat/*` | Une fonctionnalité destinée à une PR upstream (ex. `feat/murs`) | Règles de `AGENTS.md` à la lettre. Ni `FORK.md`, ni fichier personnel, ni attribution d'agent. |
| `perso` | Version personnelle : upstream + PR upstream non fusionnées + fonctions perso (murs, gaines CVC, BIM…) | Reconstruite par assemblage. Liste des PR intégrées et écartées dans `perso/pr-integrees.tsv`. **C'est ici que vit ce fichier** : une IA qui reprend le travail lit `FORK.md` sur `perso`. |
| `murs-local` | Ancienne branche de test des murs | Remplacée par `perso`. |

Remotes : `origin` = ce fork, `upstream` = `storytold/cadcraft`. Ne pousse jamais vers `upstream`.

## Mises à jour sans perte

- Les modifications personnelles vivent dans des commits sur le fork, jamais dans le dossier d'installation.
- La mise à jour se fait par le script `cadcraft-maj` sur son PC. Il récupère `origin/perso` dans un worktree séparé, compile et installe « CADCraft Perso » à côté de la version officielle du launcher ArtCraft. Le launcher n'est jamais modifié.
- Pour réassembler `perso` : partir de `upstream/main`, fusionner les PR de `perso/pr-integrees.tsv` dans l'ordre, puis les branches `feat/*` du fork, puis ce fichier. Une PR qui casse la compilation ou les tests est retirée et notée avec la raison.
- Ne réécris l'historique de `perso` (force-push) qu'après accord.

## Compiler et tester

- `cargo xtask ci` avant chaque commit sur une branche `feat/*`.
- Polices : `CRAFT_FONTS_DIR` pointe vers un clone de `storytold/craft-fonts`. Le code doit marcher sans.
- Le disque est souvent presque plein : `CARGO_INCREMENTAL=0`, et nettoie `target/` après usage.
- Lance les tests en mode debug. En release, `dwg_exhausted_handles_are_preserved_or_refused` échoue à cause d'un bug upstream connu.

## Projets en cours

- **Murs paramétriques** (`feat/murs`) : styles multicouches (béton 200, brique + laine + BA13, ossature bois, cloisons), commandes `walladd` et `wallstyleedit`. Prévu ensuite : onglet Architecture et dialogue des styles (PR 2), raccords L/T/X (PR 3), ouvertures (portes, fenêtres, baies).
- **Gaines de ventilation** (perso uniquement) : réseaux circulaires et rectangulaires, débits par pièce, dimensionnement automatique, métré par pièce. Moteur générique, réutilisable pour les liaisons frigorifiques, les condensats, l'eau glacée et les chemins de câbles.
- **BIM** (perso uniquement) : export IFC (ISO 16739, spécification publique buildingSMART) des murs, ouvertures, pièces et gaines.

## Où nous en sommes (reprise de session)

Cette section sert à reprendre le travail dans une nouvelle conversation, avec n'importe quelle IA. Elle est mise à jour en continu (règle n° 2) : date, ce qui est fait, ce qui reste.

*Dernière mise à jour : 10 novembre 2026.*

### Fait

- **Mise à jour avec l'officiel** (10 novembre) : upstream `main` est passé de `a655cee` à `a3d9348` (8 novembre, 106 commits, beaucoup de PR fusionnées). Fait, validé par Guillaume :
  - `main` du fork avancé en fast-forward sur `a3d9348`.
  - `feat/murs` rebasé sur `a3d9348` (force-push validé), sans conflit : nouveau sommet `3d3153f`.
  - `a3d9348` fusionné dans `perso` (commit de fusion, sans réécriture). Aucun conflit, `cargo check --workspace --all-targets` passe, tous les tests de `doc`, `engine` et `io` passent en debug (dont les 39 tests des murs). `cargo xtask ci` complet pas relancé.
- **PR 1 des murs** sur `origin/feat/murs` (sommet `3d3153f`, basé sur upstream `a3d9348`). Pas encore proposée upstream.
  - Nouveaux fichiers : `crates/doc/src/wall.rs` (+ `wall/tests.rs`, 19 tests), `crates/doc/src/wall_library.rs` (6 styles), `crates/engine/src/cmd/arch.rs` (`walladd`, `wallstyleedit`), `crates/engine/tests/it/walls.rs` (16 tests), `crates/io/tests/it/wall_roundtrip.rs`.
  - Stockage : chaque mur est un bloc anonyme et un INSERT, avec des xdata d'application `CADCRAFT_WALL` (format `WALL1`). Les styles sont dans le XRECORD `CADCRAFT_WALLSTYLES` du dictionnaire nommé. Ça survit en DXF et en DWG. Calques `A-WALL` et `A-WALL-PATT`.
  - Traductions ajoutées dans `fr.tsv` et `zh.tsv`. Le test i18n exige le chinois pour chaque libellé de commande.
  - `cargo xtask ci` passe, sauf l'étape wasm, qui n'a pas pu tourner : la cible wasm n'a pas pu être téléchargée.
- **Scripts sur le PC** : `~/.local/bin/maj-cadcraft-murs` (installe `cadcraft-murs`) et le lanceur `~/.local/share/applications/cadcraft-murs.desktop`. Le dépôt est dans `/mnt/Jeux/artcraft/cadcraft`, les polices dans `/mnt/Jeux/artcraft/craft-fonts`. Le modèle à suivre est `~/.local/bin/gridcraft-maj` (worktree détaché de `origin/perso`).
- **Analyse des PR upstream** (6 novembre, **périmée** depuis la mise à jour sur `a3d9348` : à refaire) :
  - 618 PR ouvertes ; 255 s'appliquent seules sur `main` ; 223 candidates une fois les PR plateforme, paquets, docs, langues, CI et refactorisations écartées (#755, impression CUPS, est gardée).
  - Fusionnées dans l'ordre croissant : 146 passent, 77 sont en conflit.
  - `feat/murs` ne fusionne ensuite qu'avec un conflit dans `fr.tsv`.
  - Première casse de compilation : **#997**. Elle redéfinit `GENERATION` et `generation()` dans `crates/fonts/src/ttf.rs`, qui existent déjà sur `main` (commit `2c979eb14`). Il faut l'écarter.
- **Validé par Guillaume** : l'assemblage de `perso` selon la méthode ci-dessus, ce fichier et la règle n° 2.
- **Branche `perso` créée et poussée** (10 novembre). Elle contient pour l'instant : upstream `main` `a3d9348`, `feat/murs`, ce fichier, et une ligne de renvoi vers `FORK.md` en tête d'`AGENTS.md`, placée avant le marqueur `craft-house-rules:start` pour ne pas toucher au bloc des règles. Les PR upstream y seront **fusionnées par-dessus** : pas de réécriture d'historique.

### En cours

1. **Intégrer les PR upstream dans `perso`.**
   - Refaire l'analyse des PR ouvertes sur la base `a3d9348` (certaines sont fusionnées depuis), puis la simulation sans #997.
   - Boucle `cargo check --workspace --all-targets` : retirer chaque PR qui casse, puis lancer les tests en debug et retirer celles qui les font échouer.
   - `feat/murs` est déjà dedans : régler le conflit `fr.tsv` au moment des fusions.
   - Écrire `perso/pr-integrees.tsv` (numéro, titre, intégrée ou écartée, raison).
   - Pousser `perso` à chaque étape, puis proposer le script `cadcraft-maj` (worktree `/mnt/Jeux/artcraft/cadcraft-perso`, binaire et lanceur « CADCraft Perso »).
2. Guillaume teste la PR 1 des murs sur son PC (commande `walladd` / « Mur »).

### Ensuite (chaque étape à présenter et faire valider avant de coder)

1. **PR 2 des murs** : onglet Architecture dans les Jeux d'outils, dialogue Styles de murs, section Mur dans Propriétés. Dans `perso`, elle peut s'appuyer sur la PR #2855 (Jeux d'outils utilisateur).
2. **PR 3 des murs** : raccords L, T et X entre murs, selon les priorités des composants.
3. **Ouvertures** : portes, fenêtres et baies.
4. **Gaines CVC** : étude et maquettes d'abord. Questions posées à Guillaume, toujours sans réponse :
   - VMC logement ou tertiaire (CTA) en premier ?
   - Dimensionnement par vitesse maximale seulement, ou aussi par perte de charge linéaire ?
5. **Export IFC** (BIM).
6. Publier en amont, plus tard : une issue de cadrage chez `storytold/cadcraft`, puis les PR des murs une par une.

## Rappels qui valent partout

- Clean room : aucun code, aucun asset, aucun texte d'Autodesk, ni de code GPL/LGPL/AGPL (LibreDWG, LibreCAD, QCAD, FreeCAD…).
- Aucun secret, aucune adresse e-mail et aucune donnée personnelle dans le dépôt, les commits ou les logs.
- Crédit : par nom d'utilisateur GitHub uniquement, pas d'entrée dans `contributors/people.toml`.
