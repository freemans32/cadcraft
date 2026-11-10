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

*Dernière mise à jour : 10 novembre 2026 (raccords de murs L, T, X).*

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
   - **Fait le 10 novembre, validé par Guillaume** : commit `feat/murs` `76fa82f` « walls: join walls where they meet (L, T and X junctions) », fusionné dans `feat/gaines` puis `perso` (sur `perso`, `cargo test --workspace` : 4 545 tests, 0 échec).
     - `crates/doc/src/wall/joint.rs` : chaque couche de chaque segment est un quadrilatère convexe ; on lui soustrait les couches des autres murs de priorité plus forte (numéro plus petit) ; à priorité égale, le mur le plus récent cède la zone commune (`ForeignBand.yields`, hachurée une seule fois) et aucun trait ne les sépare. Les traits sont tracés là où la matière change (rien, couche plus faible d'un autre mur, autre couche du même mur une seule fois). Une extrémité libre est amenée jusqu'au parement opposé du mur qu'elle touche (`EndCut`), qu'elle soit trop courte ou trop longue.
     - `WallParams.joints` (absent si le mur est seul : ancien dessin inchangé, les murs existants restent reconnus) garde les coupes et les contours des voisins dans les coordonnées du bloc : le mur se redessine à partir de ses seules données.
     - `Drawing::cleanup_walls(space)` ; recalcul automatique après WALLADD, WALLEDIT, WALLSTYLEEDIT ; commande `WALLCLEANUP` (bouton « Jonctions de murs » de l'onglet Architecture) après MOVE, COPY, ERASE.
     - Cœur des cloisons standard passé en priorité 810 (au-dessus des parements à 800) pour qu'une cloison s'arrête sur le parement du mur rencontré.
     - Vérifié : tests unitaires (L, T cloison, T brique, X, mur tourné, mur déplacé, données étendues), tests d'intégration (WALLADD, WALLCLEANUP après MOVE, WALLEDIT, annulation), aller-retour DXF/DWG de murs raccordés, captures d'écran (rendu `ui.render`). Le rendu `ui.render` coupe le motif ZIGZAG aux bords d'une fenêtre : défaut d'affichage existant, sans rapport.
     - Pas fait : recalcul automatique après MOVE/STRETCH (utiliser WALLCLEANUP), murs en arc, désactiver le raccord d'une extrémité.
   - **Correction des angles en L, 10 novembre, validée par Guillaume** (signalé : brique qui traversait la laine, angle vide) : commit `feat/murs` `6c1e3f9` « walls: wrap each layer around an L corner », fusionné dans `feat/gaines` (`d202aff`) et `perso` (`96a2c1a`).
     - Cause : chaque couche allait jusqu'au parement opposé de l'autre mur ; quand la brique est à l'intérieur de l'angle (tour tracé dans ce sens), elle coupait la laine et le BA13 de l'autre mur et l'angle faisait une croix.
     - Correction : quand une extrémité libre rencontre l'extrémité libre d'un autre mur (angle en L), chaque couche s'arrête sur le bord extérieur de la couche de même priorité correspondante de l'autre mur, comptée depuis l'extérieur de l'angle (`WallJoints.start_layers` / `end_layers`, absents sinon). Les couches s'emboîtent en L, quel que soit le côté de la brique et l'ordre de tracé. T et X inchangés. Anciens dessins : relancer WALLCLEANUP.
     - Limite connue : deux murs tracés en sens contraires (brique dehors sur l'un, dedans sur l'autre) donnent un angle en escalier ; il faut retracer l'un des deux dans l'autre sens ou changer sa justification.
     - Vérifié : les 72 combinaisons (sens, justification, ordre) rendues avec `cadcraft-cli` ; nouveau test qui échoue sans la correction ; `cargo xtask ci` sur `feat/murs` : fmt, clippy, i18n, assets, layers OK, tests OK sauf `mtext::tests::strips_formatting` (échec déjà présent sur l'officiel) ; wasm non vérifié (cible absente de la session). Accrochage (F3) sur les murs vérifié par test : extrémité, milieu, proche, insertion fonctionnent.
     - Erreur corrigée : le premier commit poussé portait une ligne d'attribution d'agent ; amendé et repoussé en force sur `feat/murs` avec l'accord de Guillaume.
3. **Ouvertures** : portes, fenêtres et baies.
4. **Gaines CVC** : étapes 1 et 2 faites (10 novembre). Réponses de Guillaume :
   - **Tertiaire (CTA)** en premier, la VMC logement plus tard.
   - Dimensionnement par **vitesse et perte de charge** (linéaire + singulières, circuit le plus défavorisé).
   - Débits : **valeur proposée selon le type de local** (Code du travail R4222-6 : 25 m³/h/occupant en bureau, 30 en restauration/vente/réunion, 45 en atelier léger, 60 sinon ; sanitaires en débit par local, à vérifier dans le texte), **modifiable à la main**.
   - **Gaines circulaires d'abord**, rectangulaires ensuite.
   - **Outil Bouche** (validé le 10 novembre, à l'étape 2) : diffuseur plafond carré 600×600 4 voies, diffuseur circulaire, grille rectangulaire (mur ou plafond), grille ou fente linéaire, bouche d'extraction circulaire ; ces types suffisent pour l'instant. Dessinées à l'échelle. Taille tapée, ou proposée d'après le **débit max par taille** (pas la vitesse au col). Placement au clic avec orientation, accrochage aux dalles, répartition automatique dans un local (nombre ou grille, débit partagé). Étiquette type, taille, débit ; le piquage sert de départ à la gaine. Bibliothèque de modèles modifiable, sans catalogue de fabricant.
   - Découpage validé : (1) moteur de calcul ; (2) locaux et bouches ; (3) tracé du réseau et accessoires ; (4) note de calcul et métré par local ; (5) rectangulaires, puis autres réseaux (frigo, condensats, eau glacée). Valeurs par défaut validées : 6 / 4 / 2,5 m/s (principal / antenne / raccordement), 1 Pa/m max, spiralé acier galvanisé (rugosité 0,15 mm).
   - **Étape 1 faite le 10 novembre** : nouvelle bibliothèque `crates/hvac` (`cadcraft-hvac`, couche L0, déclarée dans `xtask/src/layers.rs`), sur la branche `feat/gaines` (commit `96532e3`, basé sur upstream `a3d9348`), fusionnée dans `perso`. Fichiers en anglais comme le reste du dépôt : `air.rs`, `rates.rs` (R4222-6 par occupant, R4212-6 sanitaires), `sizes.rs` (série Ø63 à Ø1250), `friction.rs` (Darcy-Weisbach + Colebrook), `fittings.rs` (ζ indicatifs : coude 90° 0,3, 45° 0,2, té passage 0,2, té dérivation 1,0, réduction 0,1), `network.rs` (arbre de tronçons, débits, diamètre = plus petit qui respecte vitesse ET Pa/m, critère qui a décidé, pertes, circuit défavorisé, pression CTA), `tests.rs` (13 tests ; valeurs de référence calculées à part en Python par dichotomie sur Colebrook ; exemple 3 bureaux de 250 m³/h → CTA 750 m³/h, 37,1 Pa).
     - Vérifié : tests, `cargo fmt`, `clippy -D warnings`, `xtask layers`, `xtask assets`. Pas vérifié : la compilation wasm (la cible n'a pas pu être installée dans la session ; le code n'utilise que std et serde).
     - Pas encore d'entrée dans `[workspace.dependencies]` du `Cargo.toml` racine : à ajouter à l'étape 2, quand `engine` en aura besoin.
   - **Étape 2 faite le 10 novembre** (plan validé par Guillaume « pour un premier jet ») : commit `feat/gaines` « hvac: spaces, air terminals and their models », fusionné dans `perso`.
     - `crates/doc/src/ventilation/` : `space.rs` (local : contour, usage, occupants, débit forcé ; étiquette nom / surface m² / débit, calque `M-SPACE`, xdata `CADCRAFT_SPACE`), `terminal.rs` (5 types : diffuseur carré 4 voies, circulaire, grille rectangulaire, fente linéaire, extraction ; dessin à l'échelle, étiquette modèle + débit, calque `M-HVAC-DIFF`, xdata `CADCRAFT_AIRTERMINAL` ; choix du plus petit modèle dont le débit max couvre le débit), `library.rs` (12 modèles génériques : SQ600-160/200, RD160/200/250, RG300x150/400x150/500x200, LN1000-2, EX100/125/160, 15 Pa), `distribute.rs` (répartition par nombre ou grille dans le contour, formes en L comprises), `xdata.rs` (paramètres JSON en xdata, comme les murs), `tests.rs`.
     - `Drawing.air_terminal_models`, enregistrée dans l'XRECORD `CADCRAFT_AIRTERMINALMODELS` (DXF et DWG, test `crates/io/tests/it/ventilation_roundtrip.rs`).
     - `crates/engine/src/cmd/hvac.rs` : `SPACEADD`, `AIRTERMINALADD` (option Distribute : un nombre ou « 2x3 », débit du local partagé), `AIRTERMINALMODELEDIT` (JSON seulement, comme WALLSTYLEEDIT) ; menu Dessin › CVC ; `LastUsed.air_terminal`. Tests : `crates/engine/tests/it/hvac.rs`. Libellés fr et zh.
     - Fusion dans `perso` : conflits seulement là où murs et gaines ajoutent des lignes au même endroit (lib.rs de doc, dxf_ext, objects, sections, cmd.rs, last_used.rs, Cargo.toml d'engine), résolus en gardant les deux. Ensuite `cargo test --workspace` sur `perso` : tout passe.
     - Vérifié : `cargo test --workspace` sur `feat/gaines` (3 762 réussis ; seul échec `cadcraft-fonts` `mtext::tests::strips_formatting`, qui échoue aussi sur l'officiel `a3d9348`), fmt, clippy -D warnings, layers. Pas vérifié : wasm, et l'essai dans l'interface (pas d'écran dans la session).
     - Laissé pour plus tard : accrochage aux dalles de faux plafond, onglet du panneau de gauche, mise à jour des étiquettes quand on modifie un local ou une bouche.
   - **Premier essai de Guillaume (10 novembre)** : un local tracé affiche 70,7 m² / 25 m³/h alors que c'est une salle de sport à 1 200 m³/h ; il ne savait pas comment le corriger. Réponse : option Flow (ou Use Workshop + 20 occupants) avant le contour. Manque constaté : rien ne permet de modifier un objet déjà posé.
   - **Demandé par Guillaume, étude présentée le 10 novembre** : modifier les objets posés, pour les locaux, les bouches, les murs et plus tard les gaines :
     - (A) commandes `SPACEEDIT`, `AIRTERMINALEDIT`, `WALLEDIT` (JSON + ligne de commande) qui régénèrent le bloc en gardant la poignée et la position ;
     - (B) double-clic sur un local (étiquette ou contour), une bouche ou un mur : sélectionne l'objet et ouvre la palette Propriétés, au lieu de REFEDIT (aujourd'hui `handle_double_click` dans `crates/ui-egui/src/canvas.rs` lance REFEDIT sur toute référence de bloc) ;
     - (C) sections dédiées dans la palette Propriétés (`properties_section`, `crates/ui-egui/src/palettes.rs`), comme la maquette « fenêtre sélectionnée » de l'étude des murs (artifact SASGo3MXGJcDwdHNRy9whf, écran Ouvertures) ;
     - (D) onglets « Architecture » et « CVC » dans les Jeux d'outils à gauche (aujourd'hui Drafting / Modeling, `toolsets` dans `palettes.rs`), icônes dessinées en code.
     - Choix de Guillaume : le double-clic ouvre la palette Propriétés (pas de fenêtre dédiée) ; maquettes d'abord. Fait le 10 novembre : 3 écrans ajoutés aux maquettes des murs (artifact SASGo3MXGJcDwdHNRy9whf, rangée « Modifier les objets posés ») : local sélectionné + onglet CVC, 4 bouches sélectionnées, mur sélectionné + onglet Architecture. Guillaume a validé (« on commence avec ça, on teste et on voit ensuite »).
     - **Fait le 10 novembre** (A + B + C + D) :
       - `feat/murs` (commit `3a4955b`) : `WALLEDIT` (style, justification, hauteur ; refuse un mur modifié à la main ou mis à l'échelle), groupe « Mur » de la palette Propriétés (`crates/ui-egui/src/palettes/objects.rs`), double-clic (`parametric_at` dans `canvas/pick.rs`, avant TEXTEDIT et avant le REFEDIT d'une PR intégrée dans `perso`), 2e ligne d'onglets des Jeux d'outils avec « Architecture » (`palettes/building.rs` : styles en boutons avec leurs couches, « Modifier le mur »). `apps/cadcraft/src/workspace.rs` accepte les nouveaux onglets (`palettes::toolset_tabs()`).
       - `feat/gaines` (fusion de `feat/murs` puis commit `76fb9cf`) : `Drawing::regenerate_space` / `regenerate_air_terminal`, `SPACEEDIT` (nom, usage, occupants, appareils, usage collectif, débit ; `flow: null` = Code du travail), `AIRTERMINALEDIT` (modèle ou `auto`, type, débit, sens, rotation, sur plusieurs bouches à la fois), groupes « Local » (avec « Bouches du local » : soufflé / requis) et « Bouche » dans Propriétés, onglet « CVC » (Local, 5 types de bouches, Répartir dans un local, Modifier…, Gaines « à venir »).
       - Écart avec la maquette : le champ « Débit » du local montre le débit retenu et se modifie directement (0 ou vide = Code du travail), au lieu d'un champ « Débit imposé » vide.
       - Vérifié : tests unitaires et d'intégration des deux branches, tests d'interface (egui_kittest) pour le double-clic et la palette, captures d'écran de l'application réelle (Xvfb, rendu logiciel) en anglais et en français. Les valeurs en lecture seule de la palette sont peintes (pas lisibles par les tests) : vérifiées sur capture.
       - Fusion dans `perso` : conflits de `arch.rs` et `walls.rs` (anciennes versions des murs), un test en double dans `crates/io/src/tests.rs` (`wall_styles_xrecord_…`, supprimé), et l'ordre du double-clic remis avant REFEDIT. Puis `cargo test --workspace` sur `perso` : tout passe.
       - Limites connues : l'en-tête de la palette dit « Référence de bloc » au lieu de « Mur », « Local » ou « Bouche » ; le gestionnaire de styles de murs reste en JSON.
   - Ensuite, étape (3), à présenter avant de coder : tracé du réseau de gaines et accessoires, en s'appuyant sur `cadcraft-hvac`.
5. **Export IFC** (BIM).
6. Publier en amont, plus tard : une issue de cadrage chez `storytold/cadcraft`, puis les PR des murs une par une.

## Rappels qui valent partout

- Clean room : aucun code, aucun asset, aucun texte d'Autodesk, ni de code GPL/LGPL/AGPL (LibreDWG, LibreCAD, QCAD, FreeCAD…).
- Aucun secret, aucune adresse e-mail et aucune donnée personnelle dans le dépôt, les commits ou les logs.
- Crédit : par nom d'utilisateur GitHub uniquement, pas d'entrée dans `contributors/people.toml`.
