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

*Dernière mise à jour : 10 novembre 2026 (analyse des PR sur a3d9348).*

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
   - **Fait le 10 novembre, validé par Guillaume : les 162 PR sont fusionnées dans `perso` et poussées, avec `perso/pr-integrees.tsv`.**
   - Script lu sur le PC le 10 novembre : `maj-cadcraft-murs` compile seulement `origin/feat/murs`, rebasé sur `main` dans une branche locale `murs-local`, puis installe `~/.local/bin/cadcraft-murs`. Il ne contient donc pas les PR de `perso`. **Validé et écrit le 10 novembre** sur le PC : `~/.local/bin/cadcraft-maj` (options `--tests`, `--aide`). Il crée lui-même le lanceur à chaque installation. Première compilation à lancer par Guillaume dans un terminal (la session ne peut pas compiler sur son PC). Le script fait : un script `cadcraft-maj` sur le modèle de `gridcraft-maj` (worktree détaché `/mnt/Jeux/artcraft/cadcraft-perso` sur `origin/perso`, binaire `~/.local/bin/cadcraft-perso`, lanceur « CADCraft Perso »). « CADCraft Murs » reste installé tant qu'il ne décide pas de le retirer.
   - Analyse refaite le 10 novembre sur `a3d9348` :
     - Liste des PR ouvertes : par `git ls-remote upstream 'refs/pull/*'` (une PR ouverte a une référence `refs/pull/N/merge`) : 653 PR. L'API GitHub de `storytold/cadcraft` n'était pas accessible depuis la session, donc pas de titres ni de statut brouillon : le tableau donne le dernier commit de chaque PR.
     - 298 s'appliquent seules sur `main`, 355 sont en conflit. Écartées par catégorie : 17 plateforme/paquets/CI, 4 plateforme (titre), 5 langues, 4 docs, 4 refactorisation, #997. Restent 263 candidates.
     - Fusion cumulée par ordre croissant sur `perso` : 162 intégrées, 82 en conflit avec les PR fusionnées avant elles, 19 retirées parce qu'elles cassent la compilation ou les tests : #1084 #1099 #1229 #1263 #1335 #1445 #1452 #1619 #1632 #1692 #1720 #1816 #1873 #2111 #2301 #2451 #2790 #2808 #2826 (raisons dans le tableau).
     - Résultat : `cargo check --workspace --all-targets` passe, `cargo test --workspace` en debug : 4 488 tests, 0 échec.
     - Méthode : les `.tsv` de traduction sont fusionnés en mode `union` (`echo '*.tsv merge=union' > .git/info/attributes`), puis un commit `perso:` retire les doublons (`es.tsv`). `Cargo.lock` : seule la version du paquet change (0.17.0 → 0.19.0), cargo le fait seul.
     - Plusieurs PR retirées pour une seule ligne (ex. #1452 : traduction chinoise manquante ; #1692 : import en double) pourraient revenir avec un petit commit de correction `perso:`. À proposer.
     - Le tableau complet (`perso/pr-integrees.tsv`) sera ajouté avec les fusions.
   - Ensuite : proposer le script `cadcraft-maj` (worktree `/mnt/Jeux/artcraft/cadcraft-perso`, binaire et lanceur « CADCraft Perso »).
2. Guillaume teste la PR 1 des murs sur son PC (commande `walladd` / « Mur »).

### Ensuite (chaque étape à présenter et faire valider avant de coder)

1. **PR 2 des murs** : onglet Architecture dans les Jeux d'outils, dialogue Styles de murs, section Mur dans Propriétés. Dans `perso`, elle peut s'appuyer sur la PR #2855 (Jeux d'outils utilisateur).
2. **PR 3 des murs** : raccords L, T et X entre murs, selon les priorités des composants.
3. **Ouvertures** : portes, fenêtres et baies.
4. **Gaines CVC** : étude en cours (10 novembre). Réponses de Guillaume :
   - **Tertiaire (CTA)** en premier, la VMC logement plus tard.
   - Dimensionnement par **vitesse et perte de charge** (linéaire + singulières, circuit le plus défavorisé).
   - Débits : **valeur proposée selon le type de local** (Code du travail R4222-6 : 25 m³/h/occupant en bureau, 30 en restauration/vente/réunion, 45 en atelier léger, 60 sinon ; sanitaires en débit par local, à vérifier dans le texte), **modifiable à la main**.
   - **Gaines circulaires d'abord**, rectangulaires ensuite.
   - Découpage proposé, en attente de validation : (1) moteur de calcul seul, sans dessin, dans `crates/doc` ; (2) locaux et bouches ; (3) tracé du réseau et accessoires ; (4) note de calcul et métré par local ; (5) rectangulaires, puis autres réseaux (frigo, condensats, eau glacée).
5. **Export IFC** (BIM).
6. Publier en amont, plus tard : une issue de cadrage chez `storytold/cadcraft`, puis les PR des murs une par une.

## Rappels qui valent partout

- Clean room : aucun code, aucun asset, aucun texte d'Autodesk, ni de code GPL/LGPL/AGPL (LibreDWG, LibreCAD, QCAD, FreeCAD…).
- Aucun secret, aucune adresse e-mail et aucune donnée personnelle dans le dépôt, les commits ou les logs.
- Crédit : par nom d'utilisateur GitHub uniquement, pas d'entrée dans `contributors/people.toml`.
