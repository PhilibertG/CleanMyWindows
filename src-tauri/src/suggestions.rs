//! Propositions de suppression pour libérer de l'espace.
//!
//! Chaque catégorie porte un niveau de sûreté :
//! - `safe`   : régénéré automatiquement (caches, temporaires...) ;
//! - `review` : probablement inutile, mais à vérifier (anciens téléchargements...) ;
//! - `info`   : gros consommateur à traiter via un outil Windows (pas de suppression directe).

use serde::Serialize;

use crate::categories::extension;
use crate::tree::{F_META, Tree};

const DAY: i64 = 86_400;
const MB: u64 = 1024 * 1024;
const MAX_ITEMS: usize = 400;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub alloc: u64,
    pub mtime: i64,
    pub is_dir: bool,
    /// Emplacement système : suppression possible seulement en administrateur.
    pub admin: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn is_system_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.get(1..).is_some_and(|r| r.starts_with(":\\windows\\") || r.starts_with(":\\programdata\\"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub safety: &'static str,
    /// `delete`, `emptyRecycleBin`, `diskCleanup` ou `none`.
    pub action: &'static str,
    pub admin: bool,
    pub total: u64,
    pub count: usize,
    pub truncated: bool,
    pub items: Vec<Item>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advice: Option<&'static str>,
}

struct Ctx<'a> {
    tree: &'a Tree,
    now: i64,
    claimed: Vec<bool>,
}

impl<'a> Ctx<'a> {
    fn new(tree: &'a Tree, now: i64) -> Self {
        Self { tree, now, claimed: vec![false; tree.len()] }
    }

    fn rel(&self, base: u32, rel: &str) -> Option<u32> {
        let mut cur = base;
        for part in rel.split('\\').filter(|p| !p.is_empty()) {
            cur = self.tree.child_by_name(cur, part)?;
        }
        Some(cur)
    }

    /// Vrai si le nœud ou l'un de ses ancêtres est déjà proposé ailleurs.
    fn is_claimed(&self, mut id: u32) -> bool {
        loop {
            if self.claimed[id as usize] {
                return true;
            }
            if id == 0 {
                return false;
            }
            id = self.tree.node(id).parent;
        }
    }

    fn age_days(&self, id: u32) -> i64 {
        (self.now - self.tree.node(id).mtime).max(0) / DAY
    }
}

#[derive(Default)]
struct Bucket {
    items: Vec<(u32, Option<String>)>,
}

impl Bucket {
    fn add(&mut self, ctx: &mut Ctx, id: u32, note: Option<String>) {
        let n = ctx.tree.node(id);
        if n.alloc == 0 || ctx.is_claimed(id) {
            return;
        }
        ctx.claimed[id as usize] = true;
        self.items.push((id, note));
    }

    /// Ajoute chaque enfant d'un dossier (plus facile à trier / sélectionner).
    fn add_children(&mut self, ctx: &mut Ctx, dir: u32, min_age_days: i64) {
        let kids: Vec<u32> = ctx.tree.children(dir).collect();
        for c in kids {
            if ctx.age_days(c) >= min_age_days {
                self.add(ctx, c, None);
            }
        }
    }

    fn finish(
        self,
        ctx: &Ctx,
        meta: (&'static str, &'static str, &'static str, &'static str, &'static str, &'static str, bool),
        advice: Option<&'static str>,
    ) -> Option<Suggestion> {
        let (id, title, description, icon, safety, action) = (meta.0, meta.1, meta.2, meta.3, meta.4, meta.5);
        if self.items.is_empty() {
            return None;
        }
        let tree = ctx.tree;
        let mut items: Vec<(u32, Option<String>)> = self.items;
        items.sort_unstable_by(|a, b| tree.node(b.0).alloc.cmp(&tree.node(a.0).alloc));
        let total: u64 = items.iter().map(|(i, _)| tree.node(*i).alloc).sum();
        if total < MB && safety != "info" {
            return None;
        }
        let count = items.len();
        let out: Vec<Item> = items
            .into_iter()
            .take(MAX_ITEMS)
            .map(|(i, note)| {
                let n = tree.node(i);
                let path = tree.path(i);
                Item {
                    id: i,
                    name: tree.name(i).to_string(),
                    admin: is_system_path(&path),
                    path,
                    alloc: n.alloc,
                    mtime: n.mtime,
                    is_dir: n.is_dir(),
                    note,
                }
            })
            .collect();
        // Droits administrateur nécessaires seulement si un élément est dans un emplacement système.
        let admin = meta.6 && out.iter().any(|i| i.admin);
        Some(Suggestion {
            id,
            title,
            description,
            icon,
            safety,
            action,
            admin,
            total,
            count,
            truncated: count > MAX_ITEMS,
            items: out,
            advice,
        })
    }
}

/// Profils utilisateur présents dans l'analyse (dossiers contenant AppData).
fn profiles(ctx: &Ctx) -> Vec<u32> {
    let t = ctx.tree;
    if t.child_by_name(0, "AppData").is_some() {
        return vec![0];
    }
    let Some(users) = t.child_by_name(0, "Users") else { return Vec::new() };
    t.children(users).filter(|u| t.child_by_name(*u, "AppData").is_some()).collect()
}

const CHROMIUM_ROOTS: &[(&str, &str)] = &[
    (r"AppData\Local\Google\Chrome\User Data", "Chrome"),
    (r"AppData\Local\Google\Chrome Beta\User Data", "Chrome Beta"),
    (r"AppData\Local\Microsoft\Edge\User Data", "Edge"),
    (r"AppData\Local\BraveSoftware\Brave-Browser\User Data", "Brave"),
    (r"AppData\Local\Vivaldi\User Data", "Vivaldi"),
    (r"AppData\Local\Chromium\User Data", "Chromium"),
    (r"AppData\Roaming\Opera Software\Opera Stable", "Opera"),
    (r"AppData\Roaming\Opera Software\Opera GX Stable", "Opera GX"),
    (r"AppData\Local\Opera Software\Opera Stable", "Opera"),
    (r"AppData\Local\Opera Software\Opera GX Stable", "Opera GX"),
];
const CHROMIUM_PROFILE_CACHES: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "DawnCache",
    "DawnGraphiteCache",
    "DawnWebGPUCache",
    r"Service Worker\CacheStorage",
    r"Service Worker\ScriptCache",
];
const CHROMIUM_ROOT_CACHES: &[&str] = &["ShaderCache", "GrShaderCache", "GraphiteDawnCache"];

const APP_CACHES: &[&str] = &[
    r"AppData\Roaming\discord\Cache",
    r"AppData\Roaming\discord\Code Cache",
    r"AppData\Roaming\discord\GPUCache",
    r"AppData\Roaming\Slack\Cache",
    r"AppData\Roaming\Slack\Code Cache",
    r"AppData\Roaming\Slack\GPUCache",
    r"AppData\Roaming\Slack\Service Worker\CacheStorage",
    r"AppData\Roaming\Microsoft\Teams\Cache",
    r"AppData\Roaming\Microsoft\Teams\Code Cache",
    r"AppData\Roaming\Microsoft\Teams\GPUCache",
    r"AppData\Roaming\Microsoft\Teams\Service Worker\CacheStorage",
    r"AppData\Roaming\Code\Cache",
    r"AppData\Roaming\Code\CachedData",
    r"AppData\Roaming\Code\CachedExtensionVSIXs",
    r"AppData\Roaming\Code\Code Cache",
    r"AppData\Roaming\Code\GPUCache",
    r"AppData\Roaming\Code\logs",
    r"AppData\Roaming\Cursor\Cache",
    r"AppData\Roaming\Cursor\CachedData",
    r"AppData\Roaming\Cursor\Code Cache",
    r"AppData\Roaming\Cursor\GPUCache",
    r"AppData\Roaming\Notion\Cache",
    r"AppData\Roaming\Notion\Code Cache",
    r"AppData\Roaming\Figma\Cache",
    r"AppData\Roaming\Figma\Code Cache",
    r"AppData\Local\Spotify\Data",
    r"AppData\Local\Spotify\Storage",
    r"AppData\Local\Microsoft\Windows\INetCache",
    r"AppData\Local\D3DSCache",
    r"AppData\Local\NVIDIA\DXCache",
    r"AppData\Local\NVIDIA\GLCache",
    r"AppData\Local\NVIDIA Corporation\NV_Cache",
    r"AppData\LocalLow\NVIDIA\PerDriverVersion\DXCache",
    r"AppData\Local\AMD\DxCache",
    r"AppData\Local\AMD\DxcCache",
    r"AppData\Local\AMD\GLCache",
    r"AppData\Local\AMD\VkCache",
    r"AppData\Local\Intel\ShaderCache",
    r"AppData\LocalLow\Intel\ShaderCache",
    r"AppData\Local\Steam\htmlcache",
    r"AppData\Local\EpicGamesLauncher\Saved\webcache",
    r"AppData\Local\EpicGamesLauncher\Saved\webcache_4147",
    r"AppData\Local\EpicGamesLauncher\Saved\webcache_4430",
    r"AppData\Local\Battle.net\Cache",
    r"AppData\Local\electron\Cache",
    r"AppData\Local\electron-builder\Cache",
];

const DEV_CACHES: &[(&str, &str)] = &[
    (r"AppData\Local\npm-cache", "Cache npm"),
    (r"AppData\Roaming\npm-cache", "Cache npm"),
    (r"AppData\Local\pnpm-cache", "Cache pnpm"),
    (r"AppData\Local\Yarn\Cache", "Cache Yarn"),
    (r"AppData\Local\pip\cache", "Cache pip"),
    (r"AppData\Local\uv\cache", "Cache uv"),
    (r"AppData\Local\pypoetry\Cache", "Cache Poetry"),
    (r".gradle\caches", "Cache Gradle"),
    (r".gradle\wrapper\dists", "Distributions Gradle"),
    (r"AppData\Local\NuGet\v3-cache", "Cache NuGet"),
    (r"AppData\Local\NuGet\Cache", "Cache NuGet"),
    (r".nuget\packages", "Paquets NuGet (retéléchargés si besoin)"),
    (r".cargo\registry\cache", "Cache Cargo"),
    (r".cargo\registry\src", "Sources Cargo décompressées"),
    (r"AppData\Local\go-build", "Cache de compilation Go"),
    (r".m2\repository", "Dépôt Maven local (retéléchargé si besoin)"),
    (r"AppData\Local\ms-playwright", "Navigateurs Playwright"),
    (r"AppData\Local\Microsoft\vscode-cpptools\ipch", "Cache IntelliSense C++"),
    (r".cache\pip", "Cache pip"),
    (r".cache\torch", "Modèles PyTorch téléchargés"),
    (r".android\avd", "Émulateurs Android"),
    (r"AppData\Local\Android\Sdk\system-images", "Images système Android"),
];

const SYSTEM_EXCLUDED: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "appdata",
    "$recycle.bin",
    "system volume information",
    "windows.old",
    "$windows.~bt",
    "$windows.~ws",
    // Contenus gérés par une application (les modifier à la main les casse).
    "steamapps",
    "steamlibrary",
    "windowsapps",
    "epic games",
    "xboxgames",
    "my games",
    "mobilesync",
];

fn is_excluded_dir(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SYSTEM_EXCLUDED.contains(&lower.as_str())
}

/// Vrai si un ancêtre est un dossier système / d'application (y compris les
/// dossiers cachés d'outils comme `.lmstudio` ou `.vscode`).
fn under_system(tree: &Tree, mut id: u32) -> bool {
    while id != 0 {
        id = tree.node(id).parent;
        if id != 0 {
            let name = tree.name(id);
            if is_excluded_dir(name) || name.starts_with('.') {
                return true;
            }
        }
    }
    false
}

fn has_child(tree: &Tree, dir: u32, name: &str) -> bool {
    tree.child_by_name(dir, name).is_some()
}

fn has_child_ext(tree: &Tree, dir: u32, exts: &[&str]) -> bool {
    tree.children(dir).any(|c| {
        !tree.node(c).is_dir() && extension(tree.name(c)).is_some_and(|e| exts.contains(&e.as_str()))
    })
}

/// Reconnaît un dossier d'artefacts de développement régénérable.
fn dev_artifact(tree: &Tree, dir: u32) -> Option<&'static str> {
    let name = tree.name(dir);
    let parent = tree.node(dir).parent;
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "node_modules" => Some("Dépendances npm (réinstallables avec npm install)"),
        "target" if has_child(tree, parent, "Cargo.toml") => Some("Compilation Rust (cargo build)"),
        "target" if has_child(tree, parent, "pom.xml") => Some("Compilation Maven"),
        ".venv" | "venv" | "env" if has_child(tree, dir, "pyvenv.cfg") => Some("Environnement virtuel Python"),
        "__pycache__" => Some("Bytecode Python"),
        ".next" | ".nuxt" | ".svelte-kit" | ".turbo" | ".parcel-cache" | ".angular" | ".expo" => {
            Some("Cache de build web")
        }
        "obj" | "bin" if has_child_ext(tree, parent, &["csproj", "vbproj", "fsproj"]) => Some("Compilation .NET"),
        "build" if has_child(tree, parent, "build.gradle") || has_child(tree, parent, "build.gradle.kts") => {
            Some("Compilation Gradle")
        }
        ".gradle" if has_child(tree, parent, "settings.gradle") || has_child(tree, parent, "settings.gradle.kts") => {
            Some("Cache Gradle du projet")
        }
        "build" if has_child(tree, parent, "CMakeLists.txt") => Some("Compilation CMake"),
        _ if lower.starts_with("cmake-build-") => Some("Compilation CLion"),
        _ => None,
    }
}

pub fn compute(tree: &Tree, now: i64) -> Vec<Suggestion> {
    let mut ctx = Ctx::new(tree, now);
    let mut out = Vec::new();
    let profs = profiles(&ctx);
    let volume_root = tree.info.is_volume_root;
    let sys = |ctx: &Ctx, rel: &str| if volume_root { ctx.rel(0, rel) } else { None };

    // Corbeille : traitée à part (vidage via le Shell).
    if let Some(rb) = sys(&ctx, "$Recycle.Bin") {
        let mut b = Bucket::default();
        b.add(&mut ctx, rb, None);
        out.extend(b.finish(
            &ctx,
            ("recycle", "Corbeille", "Fichiers supprimés qui occupent encore de l'espace sur ce disque.", "trash", "safe", "emptyRecycleBin", false),
            None,
        ));
    }

    // 1. Fichiers temporaires.
    let mut b = Bucket::default();
    for &p in &profs {
        if let Some(t) = ctx.rel(p, r"AppData\Local\Temp") {
            b.add_children(&mut ctx, t, 1);
        }
    }
    let temp_admin = sys(&ctx, r"Windows\Temp").is_some();
    if let Some(t) = sys(&ctx, r"Windows\Temp") {
        b.add_children(&mut ctx, t, 1);
    }
    out.extend(b.finish(
        &ctx,
        ("temp", "Fichiers temporaires", "Fichiers laissés par les installations et les applications. Ceux modifiés dans les dernières 24 h sont ignorés.", "clock", "safe", "delete", temp_admin),
        Some("Les fichiers en cours d'utilisation seront simplement ignorés."),
    ));

    // 2. Caches des navigateurs.
    let mut b = Bucket::default();
    for &p in &profs {
        for (root, label) in CHROMIUM_ROOTS {
            let Some(r) = ctx.rel(p, root) else { continue };
            let browser = (*label).to_string();
            for c in CHROMIUM_ROOT_CACHES {
                if let Some(id) = ctx.rel(r, c) {
                    b.add(&mut ctx, id, Some(browser.clone()));
                }
            }
            let mut profile_dirs: Vec<u32> = tree
                .children(r)
                .filter(|c| {
                    let n = tree.name(*c);
                    tree.node(*c).is_dir() && (n == "Default" || n.starts_with("Profile ") || n == "Guest Profile")
                })
                .collect();
            profile_dirs.push(r);
            for pd in profile_dirs {
                for c in CHROMIUM_PROFILE_CACHES {
                    if let Some(id) = ctx.rel(pd, c) {
                        b.add(&mut ctx, id, Some(browser.clone()));
                    }
                }
            }
        }
        if let Some(ff) = ctx.rel(p, r"AppData\Local\Mozilla\Firefox\Profiles") {
            let profiles: Vec<u32> = tree.children(ff).collect();
            for pr in profiles {
                for c in ["cache2", "startupCache", "thumbnails", "jumpListCache"] {
                    if let Some(id) = ctx.rel(pr, c) {
                        b.add(&mut ctx, id, Some("Firefox".into()));
                    }
                }
            }
        }
    }
    out.extend(b.finish(
        &ctx,
        ("browser", "Caches des navigateurs", "Pages, images et scripts mis en cache par Chrome, Edge, Firefox, Brave, Opera... Ils se reconstituent au fil de la navigation.", "globe", "safe", "delete", false),
        Some("Fermez vos navigateurs avant le nettoyage pour libérer un maximum d'espace."),
    ));

    // 3. Caches d'applications.
    let mut b = Bucket::default();
    for &p in &profs {
        for rel in APP_CACHES {
            if let Some(id) = ctx.rel(p, rel) {
                let app = match rel.split('\\').nth(2).unwrap_or("Application") {
                    "Code" => "VS Code",
                    "discord" => "Discord",
                    "Microsoft" => "Microsoft",
                    "NVIDIA" | "NVIDIA Corporation" => "NVIDIA (shaders)",
                    "AMD" => "AMD (shaders)",
                    "Intel" => "Intel (shaders)",
                    "D3DSCache" => "DirectX (shaders)",
                    "EpicGamesLauncher" => "Epic Games",
                    other => other,
                }
                .to_string();
                b.add(&mut ctx, id, Some(app));
            }
        }
        if let Some(explorer) = ctx.rel(p, r"AppData\Local\Microsoft\Windows\Explorer") {
            let kids: Vec<u32> = tree.children(explorer).collect();
            for c in kids {
                let n = tree.name(c).to_ascii_lowercase();
                if n.starts_with("thumbcache_") || n.starts_with("iconcache_") {
                    b.add(&mut ctx, c, Some("Miniatures de l'Explorateur".into()));
                }
            }
        }
    }
    out.extend(b.finish(
        &ctx,
        ("appcache", "Caches des applications", "Caches de Discord, Teams, Slack, VS Code, Spotify, des pilotes graphiques (shaders)...", "layers", "safe", "delete", false),
        Some("Fermez les applications concernées avant le nettoyage."),
    ));

    // 4. Rapports d'erreurs et vidages mémoire.
    let mut b = Bucket::default();
    let mut dumps_admin = false;
    for &p in &profs {
        for rel in [r"AppData\Local\CrashDumps", r"AppData\Local\Microsoft\Windows\WER\ReportArchive", r"AppData\Local\Microsoft\Windows\WER\ReportQueue"] {
            if let Some(d) = ctx.rel(p, rel) {
                b.add_children(&mut ctx, d, 0);
            }
        }
    }
    for rel in [r"ProgramData\Microsoft\Windows\WER\ReportArchive", r"ProgramData\Microsoft\Windows\WER\ReportQueue", r"Windows\Minidump", r"Windows\LiveKernelReports"] {
        if let Some(d) = sys(&ctx, rel) {
            dumps_admin = true;
            b.add_children(&mut ctx, d, 0);
        }
    }
    if let Some(d) = sys(&ctx, r"Windows\MEMORY.DMP") {
        dumps_admin = true;
        b.add(&mut ctx, d, None);
    }
    out.extend(b.finish(
        &ctx,
        ("dumps", "Rapports d'erreurs et vidages mémoire", "Rapports de plantage et fichiers de vidage mémoire, utiles seulement pour le diagnostic.", "bug", "safe", "delete", dumps_admin),
        None,
    ));

    // 5. Mises à jour Windows.
    let mut b = Bucket::default();
    for rel in [r"Windows\SoftwareDistribution\Download", r"Windows\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache"] {
        if let Some(d) = sys(&ctx, rel) {
            b.add_children(&mut ctx, d, 3);
        }
    }
    out.extend(b.finish(
        &ctx,
        ("winupdate", "Fichiers de mise à jour Windows", "Mises à jour déjà installées et cache d'optimisation de la distribution.", "download", "safe", "delete", true),
        Some("Nécessite les droits administrateur. Évitez pendant une mise à jour en cours."),
    ));

    // 6. Caches des outils de développement.
    let mut b = Bucket::default();
    for &p in &profs {
        for (rel, label) in DEV_CACHES {
            if let Some(id) = ctx.rel(p, rel) {
                b.add(&mut ctx, id, Some((*label).to_string()));
            }
        }
    }
    out.extend(b.finish(
        &ctx,
        ("devcache", "Caches des outils de développement", "Caches npm, pip, Gradle, NuGet, Cargo, modèles d'IA téléchargés... Ils seront retéléchargés si nécessaire.", "package", "review", "delete", false),
        None,
    ));

    // 7. Artefacts de projets de développement (node_modules, target...).
    let mut b = Bucket::default();
    let mut stack = vec![0u32];
    while let Some(d) = stack.pop() {
        let kids: Vec<u32> = tree.children(d).filter(|c| tree.node(*c).is_dir()).collect();
        for c in kids {
            let name = tree.name(c);
            if is_excluded_dir(name) || tree.node(c).flags & F_META != 0 {
                continue;
            }
            if let Some(label) = dev_artifact(tree, c) {
                let project = tree.node(c).parent;
                let days = ctx.age_days(project);
                let note = format!("{label} — projet « {} », activité il y a {days} j", tree.name(project));
                b.add(&mut ctx, c, Some(note));
                continue;
            }
            // Les dossiers de configuration cachés (.vscode, .nvm...) contiennent des outils installés.
            if name.starts_with('.') {
                continue;
            }
            stack.push(c);
        }
    }
    out.extend(b.finish(
        &ctx,
        ("devartifacts", "Dépendances et builds de projets", "node_modules, target, .venv, bin/obj... Régénérables en réinstallant ou recompilant le projet.", "code", "review", "delete", false),
        Some("Privilégiez les projets inactifs depuis longtemps."),
    ));

    // Modèles d'IA téléchargés (Ollama, LM Studio, Hugging Face...).
    let mut b = Bucket::default();
    for &p in &profs {
        // LM Studio : un dossier par modèle (models\<éditeur>\<modèle>).
        for root in [r".lmstudio\models", r".cache\lm-studio\models"] {
            if let Some(models) = ctx.rel(p, root) {
                let publishers: Vec<u32> = tree.children(models).filter(|c| tree.node(*c).is_dir()).collect();
                for publisher in publishers {
                    let list: Vec<u32> = tree.children(publisher).collect();
                    for m in list {
                        let days = ctx.age_days(m);
                        b.add(&mut ctx, m, Some(format!("Modèle LM Studio, utilisé il y a {days} j")));
                    }
                }
            }
        }
        for (rel, note) in [
            (r".ollama\models", "Modèles Ollama — préférez « ollama rm <modèle> » pour en retirer un seul"),
            (r".cache\huggingface", "Modèles Hugging Face (retéléchargés à la demande)"),
            (r"AppData\Local\nomic.ai\GPT4All", "Modèles GPT4All"),
            (r".cache\gpt4all", "Modèles GPT4All"),
            (r"AppData\Local\Jan\data\models", "Modèles Jan"),
            (r"jan\models", "Modèles Jan"),
            (r".cache\whisper", "Modèles Whisper"),
            (r"stable-diffusion-webui\models", "Modèles Stable Diffusion"),
            (r"ComfyUI\models", "Modèles ComfyUI"),
        ] {
            if let Some(id) = ctx.rel(p, rel) {
                b.add(&mut ctx, id, Some(note.into()));
            }
        }
    }
    out.extend(b.finish(
        &ctx,
        ("aimodels", "Modèles d'IA téléchargés", "Modèles de langage et d'image stockés localement (Ollama, LM Studio, Hugging Face…). Souvent plusieurs Go chacun ; supprimez ceux que vous n'utilisez plus.", "brain", "review", "delete", false),
        Some("Un modèle supprimé devra être retéléchargé pour être réutilisé."),
    ));

    // Sauvegardes d'iPhone / iPad (iTunes, Appareils Apple).
    let mut b = Bucket::default();
    for &p in &profs {
        for rel in [r"Apple\MobileSync\Backup", r"AppData\Roaming\Apple Computer\MobileSync\Backup"] {
            if let Some(backups) = ctx.rel(p, rel) {
                let list: Vec<u32> = tree.children(backups).filter(|c| tree.node(*c).is_dir()).collect();
                for bk in list {
                    let days = ctx.age_days(bk);
                    b.add(&mut ctx, bk, Some(format!("Sauvegarde mise à jour il y a {days} j")));
                }
            }
        }
    }
    out.extend(b.finish(
        &ctx,
        ("mobilebackup", "Sauvegardes d'iPhone et d'iPad", "Copies complètes de vos appareils Apple. Gardez la plus récente de chaque appareil, supprimez les anciennes.", "smartphone", "review", "delete", false),
        Some("Vous pouvez aussi les gérer depuis l'application Appareils Apple / iTunes."),
    ));

    // Disques virtuels (WSL, Docker, machines virtuelles) : à compacter, pas à supprimer.
    let mut b = Bucket::default();
    for &p in &profs {
        let mut stack = vec![p];
        while let Some(d) = stack.pop() {
            let kids: Vec<u32> = tree.children(d).collect();
            for c in kids {
                let n = tree.node(c);
                if n.is_dir() {
                    stack.push(c);
                } else if n.alloc >= 1024 * MB && matches!(extension(tree.name(c)).as_deref(), Some("vhdx" | "vhd" | "vmdk" | "vdi" | "qcow2")) {
                    let lower = ctx.tree.path(c).to_ascii_lowercase();
                    let note = if lower.contains("\\wsl\\") || lower.contains("canonicalgroup") || lower.contains("ext4.vhdx") {
                        "WSL : « wsl --manage <distribution> --set-sparse true » récupère l'espace libre"
                    } else if lower.contains("docker") {
                        "Docker : « docker system prune -a » puis compactage via Docker Desktop"
                    } else {
                        "Disque de machine virtuelle : compactez-le depuis son logiciel"
                    };
                    b.add(&mut ctx, c, Some(note.into()));
                }
            }
        }
    }
    out.extend(b.finish(
        &ctx,
        ("vdisks", "Disques virtuels (WSL, Docker, VM)", "Ces fichiers grossissent sans jamais rétrécir tout seuls. Ne les supprimez pas : compactez-les avec l'outil indiqué.", "hard-drive", "info", "none", false),
        Some("Supprimer un disque virtuel efface la distribution ou la machine virtuelle qu'il contient."),
    ));

    // 8. Anciens téléchargements.
    let mut b = Bucket::default();
    for &p in &profs {
        if let Some(dl) = ctx.rel(p, "Downloads") {
            let kids: Vec<u32> = tree.children(dl).collect();
            for c in kids {
                let days = ctx.age_days(c);
                let ext = extension(tree.name(c)).unwrap_or_default();
                let installer = matches!(ext.as_str(), "exe" | "msi" | "msix" | "iso" | "zip" | "rar" | "7z" | "appx");
                if days >= 90 {
                    b.add(&mut ctx, c, Some(format!("Non modifié depuis {days} jours")));
                } else if installer && days >= 7 {
                    b.add(&mut ctx, c, Some("Installeur ou archive déjà utilisé".into()));
                }
            }
        }
    }
    out.extend(b.finish(
        &ctx,
        ("downloads", "Anciens téléchargements", "Fichiers du dossier Téléchargements non modifiés depuis plus de 3 mois, et installeurs déjà utilisés.", "inbox", "review", "delete", false),
        None,
    ));

    // 9. Images disque et installeurs volumineux.
    let mut b = Bucket::default();
    let mut large_candidates = Vec::new();
    for id in tree.live_ids() {
        let n = tree.node(id);
        if n.is_dir() || n.alloc < 100 * MB || n.flags & F_META != 0 {
            continue;
        }
        let ext = extension(tree.name(id)).unwrap_or_default();
        if matches!(ext.as_str(), "iso" | "img" | "dmg" | "msi" | "msix" | "appx") {
            if !under_system(tree, id) {
                b.add(&mut ctx, id, Some(format!("Image / installeur .{ext}")));
            }
        } else if n.alloc >= 500 * MB {
            large_candidates.push(id);
        }
    }
    out.extend(b.finish(
        &ctx,
        ("installers", "Images disque et installeurs", "Fichiers ISO, IMG et installeurs volumineux, souvent inutiles une fois le logiciel installé.", "disc", "review", "delete", false),
        None,
    ));

    // 10. Gros fichiers anciens.
    let mut b = Bucket::default();
    for id in large_candidates {
        let days = ctx.age_days(id);
        if days >= 180 && tree.node(id).parent != 0 && !under_system(tree, id) {
            b.add(&mut ctx, id, Some(format!("Non modifié depuis {} mois", days / 30)));
        }
    }
    out.extend(b.finish(
        &ctx,
        ("large", "Gros fichiers anciens", "Fichiers de plus de 500 Mo non modifiés depuis plus de 6 mois.", "file", "review", "delete", false),
        None,
    ));

    // 11. Journaux volumineux.
    let mut b = Bucket::default();
    let logs: Vec<u32> = tree
        .live_ids()
        .filter(|id| {
            let n = tree.node(*id);
            !n.is_dir() && n.alloc >= 20 * MB && matches!(extension(tree.name(*id)).as_deref(), Some("log" | "etl" | "trace"))
        })
        .collect();
    for id in logs {
        b.add(&mut ctx, id, None);
    }
    out.extend(b.finish(
        &ctx,
        ("logs", "Journaux volumineux", "Fichiers journaux de plus de 20 Mo.", "scroll", "review", "delete", false),
        None,
    ));

    // 12. Gros fichiers système (informatif).
    let mut b = Bucket::default();
    for (rel, note) in [
        ("hiberfil.sys", "Hibernation : libérable avec « powercfg /h off » (admin)"),
        ("pagefile.sys", "Mémoire virtuelle : réglable dans Paramètres système avancés"),
        ("Windows.old", "Ancienne installation : Paramètres > Stockage > Fichiers temporaires"),
        ("$Windows.~BT", "Fichiers d'installation de mise à niveau"),
        ("System Volume Information", "Points de restauration : Protection du système"),
    ] {
        if let Some(id) = sys(&ctx, rel) {
            if tree.node(id).alloc >= 256 * MB {
                b.add(&mut ctx, id, Some(note.into()));
            }
        }
    }
    if let Some(id) = sys(&ctx, r"Windows\Installer\$PatchCache$") {
        b.add(&mut ctx, id, Some("Cache de correctifs : Nettoyage de disque".into()));
    }
    out.extend(b.finish(
        &ctx,
        ("system", "Gros fichiers système", "Éléments gérés par Windows : ils ne doivent pas être supprimés à la main, mais peuvent être réduits avec les outils Windows.", "shield", "info", "diskCleanup", true),
        Some("Utilisez le Nettoyage de disque de Windows (bouton ci-dessous) pour ces éléments."),
    ));

    out
}
