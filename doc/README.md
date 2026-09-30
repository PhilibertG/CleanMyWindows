# CleanMyWindows

Analyse de disque ultra-rapide et nettoyage intelligent pour Windows — l'équivalent de CleanMyMac.

- **Analyse de n'importe quel disque ou dossier** (NTFS, exFAT, FAT32, réseau, clé USB).
- **Carte des tailles** (treemap) et explorateur pour voir instantanément ce qui prend de la place.
- **Propositions de suppression** classées par niveau de risque, avec présélection de ce qui est sans danger.
- **Doublons** (comparaison du contenu), **plus gros fichiers**, **types de fichiers**, **recherche**.

## Installation

Installeur : `src-tauri/target/release/bundle/nsis/CleanMyWindows_1.0.0_x64-setup.exe`
(ou l'exécutable portable `src-tauri/target/release/clean-my-windows.exe`).

## Développement

Prérequis : Node 20+, Rust (toolchain MSVC), Visual Studio Build Tools (C++), WebView2 (inclus dans Windows 11).

```bash
npm install
npm run app:dev      # application en mode développement
npm run app:build    # exécutable + installeur NSIS
```

Banc d'essai du moteur (lecture seule, aucune suppression) :

```bash
cd src-tauri
cargo run --release --example bench -- C:\ walk      # parcours parallèle
cargo run --release --example bench -- D:\ compare   # MFT vs parcours (administrateur)
cargo run --release --example bench -- C:\ suggest   # liste les propositions
```

Variables utiles : `CMW_TIMING=1` (durée de chaque phase), `CMW_THREADS=n` (forcer le nombre de threads),
`CMW_TURBO=raw|layout` (forcer une méthode Turbo).

## Architecture

```
src/                     Interface React 19 + Tailwind 4 + shadcn/ui
  views/                 Disques, Analyse, Vue d'ensemble, Explorateur, Plus gros fichiers,
                         Types, Propositions, Doublons, Recherche
  components/            Treemap (canvas + d3-hierarchy), dialogue de nettoyage, sidebar
  lib/                   API Tauri typée, état global, formatage (fr-FR)
src-tauri/src/           Moteur Rust
  scan/walk.rs           Parcours parallèle (NtOpenFile relatif + lecture par lots)
  scan/mft.rs            Mode Turbo 1 : lecture brute de la MFT NTFS
  scan/layout.rs         Mode Turbo 2 : FSCTL_QUERY_FILE_LAYOUT
  tree.rs                Arbre compact en mémoire (~56 o/nœud, noms dans un seul buffer)
  suggestions.rs         Moteur de propositions de suppression
  duplicates.rs          Doublons : taille → empreinte partielle → xxh3-128 complet
  actions.rs             Corbeille, suppression, garde-fous, élévation administrateur
  queries.rs             Listing, treemap, top fichiers, statistiques, recherche
```

### Moteur d'analyse

| Méthode | Quand | Principe |
|---|---|---|
| **Turbo MFT** | Administrateur + volume NTFS entier | Lecture séquentielle de la Master File Table par blocs de 16 Mo, décodage parallèle des enregistrements. Tailles exactes (fichiers creux, compressés, flux alternatifs, métafichiers NTFS). |
| **Turbo table** | Si la lecture brute est refusée | `FSCTL_QUERY_FILE_LAYOUT` : le système de fichiers énumère lui-même la MFT. |
| **Parcours parallèle** | Sinon (ou dossier, autre système de fichiers) | Pool rayon à vol de travail ; chaque dossier est ouvert *relativement* à son parent (`NtOpenFile`) et lu par lots de 256 Ko (`GetFileInformationByHandleEx`), sans aucun `stat` par fichier. 4 threads sur disque dur, jusqu'à 16 sur SSD. |

Mesures sur la machine de développement (C: SSD, 2,2 millions de fichiers, 484 000 dossiers, 409 Go) :

- Parcours parallèle : **9,2 s** (≈ 240 000 fichiers/s), cache chaud ; ~50 s à froid.
- Turbo MFT sur D: : écart avec l'espace réellement utilisé de **19,7 Mo** (précision quasi parfaite).

Le coût du parcours classique est dominé par le noyau Windows (antivirus Defender qui inspecte chaque
ouverture de dossier) : c'est pourquoi le mode Turbo, qui n'ouvre aucun dossier, est proposé dès le lancement.

> Sur les volumes protégés par le filtre Xbox *Gaming Services* (`gameflt`), Windows refuse l'accès direct au
> volume : l'application bascule alors automatiquement sur le parcours parallèle.

Les tailles affichées sont les **tailles sur disque** (espace réellement libérable) ; les fichiers cloud non
téléchargés (OneDrive, Google Drive) comptent donc pour 0. Les jonctions et liens symboliques ne sont pas suivis.

### Propositions de suppression

| Catégorie | Niveau | Exemples |
|---|---|---|
| Corbeille | Sans risque | `$Recycle.Bin` (vidée via le Shell Windows) |
| Fichiers temporaires | Sans risque | `%TEMP%`, `C:\Windows\Temp` (hors fichiers de moins de 24 h) |
| Caches des navigateurs | Sans risque | Chrome, Edge, Brave, Opera (GX), Vivaldi, Firefox |
| Caches des applications | Sans risque | Discord, Teams, Slack, VS Code, Spotify, Steam, shaders NVIDIA/AMD/Intel/DirectX, miniatures |
| Rapports d'erreurs | Sans risque | CrashDumps, WER, Minidump, MEMORY.DMP |
| Mises à jour Windows | Sans risque (admin) | SoftwareDistribution\Download, Delivery Optimization |
| Caches de développement | À vérifier | npm, pnpm, Yarn, pip, uv, Gradle, NuGet, Cargo, Maven, Playwright… |
| Dépendances de projets | À vérifier | `node_modules`, `target`, `.venv`, `bin/obj`, `.next`… avec l'ancienneté du projet |
| Modèles d'IA | À vérifier | Ollama, LM Studio, Hugging Face, GPT4All, Stable Diffusion, ComfyUI |
| Sauvegardes iPhone/iPad | À vérifier | `MobileSync\Backup` |
| Anciens téléchargements | À vérifier | Non modifiés depuis 90 jours, installeurs déjà utilisés |
| Images disque / installeurs | À vérifier | ISO, IMG, MSI > 100 Mo |
| Gros fichiers anciens | À vérifier | > 500 Mo, non modifiés depuis 6 mois |
| Journaux volumineux | À vérifier | `.log`, `.etl` > 20 Mo |
| Disques virtuels | Information | WSL, Docker, VM : commande de compactage indiquée |
| Fichiers système | Information | hiberfil, pagefile, Windows.old, points de restauration → Nettoyage de disque |

### Sécurité

- Les éléments « sans risque » sont présélectionnés ; le reste doit être coché volontairement.
- Le dialogue de nettoyage propose **Corbeille** (par défaut pour « à vérifier ») ou **suppression définitive**.
- Garde-fous côté Rust (`actions::refuse_reason`) : impossible de supprimer la racine d'un disque, un dossier à la
  racine (Windows, Users, Program Files…), un profil utilisateur, AppData, ni un fichier de `C:\Windows` en dehors
  des emplacements de nettoyage connus.
- La recherche de doublons ignore les dossiers système et d'applications, les sauvegardes d'appareils, les
  bibliothèques de jeux, les dépendances, les liens durs et les fichiers cloud non téléchargés.
- Les fichiers verrouillés sont simplement ignorés et signalés.

### Mode Turbo (administrateur)

Le bouton « Activer le mode Turbo » relance l'application via UAC (`runas`). En administrateur, le privilège
`SeBackupPrivilege` est activé pour lire aussi les dossiers protégés (0 dossier inaccessible).
