//! CleanMyWindows — backend Tauri.

pub mod actions;
pub mod categories;
pub mod duplicates;
pub mod queries;
pub mod scan;
pub mod suggestions;
pub mod tree;
pub mod volumes;
pub mod win;

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::actions::{CleanMode, CleanResult};
use crate::duplicates::{DupCtx, DupResult};
use crate::scan::{ScanCtx, ScanMode};
use crate::tree::{ScanInfo, Tree};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Default)]
struct AppState {
    tree: Arc<RwLock<Option<Tree>>>,
    scan: Mutex<Option<Arc<ScanCtx>>>,
    dup: Mutex<Option<Arc<DupCtx>>>,
}

type CmdResult<T> = Result<T, String>;

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Exécute une lecture sur l'arbre dans un thread bloquant.
async fn with_tree<T, F>(state: &State<'_, AppState>, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Tree) -> T + Send + 'static,
{
    let tree = state.tree.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = tree.read();
        let t = guard.as_ref().ok_or("Aucune analyse disponible")?;
        Ok(f(t))
    })
    .await
    .map_err(|e| e.to_string())?
}

// --- Système -----------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemInfo {
    elevated: bool,
    system_drive: String,
    cpus: usize,
    version: String,
}

#[tauri::command]
fn get_system_info(app: AppHandle) -> SystemInfo {
    SystemInfo {
        elevated: win::is_elevated(),
        system_drive: format!("{}:\\", volumes::system_letter()),
        cpus: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        version: app.package_info().version.to_string(),
    }
}

#[tauri::command]
async fn list_volumes() -> CmdResult<Vec<volumes::Volume>> {
    tauri::async_runtime::spawn_blocking(volumes::list).await.map_err(|e| e.to_string())
}

// --- Analyse -----------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Summary {
    info: ScanInfo,
    root: queries::NodeInfo,
    nodes: usize,
}

fn summary(tree: &Tree) -> Summary {
    Summary { info: tree.info.clone(), root: queries::info(tree, 0, true), nodes: tree.len() }
}

#[tauri::command]
fn start_scan(app: AppHandle, state: State<'_, AppState>, path: String, mode: ScanMode) -> CmdResult<()> {
    let ctx = Arc::new(ScanCtx::default());
    {
        let mut cur = state.scan.lock();
        if let Some(prev) = cur.as_ref() {
            prev.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        *cur = Some(ctx.clone());
    }
    let tree_slot = state.tree.clone();
    let root = scan::normalize_root(&path);
    std::thread::Builder::new()
        .name("cmw-scan".into())
        .spawn(move || {
            let started = Instant::now();
            let expected = scan::expected_bytes(&root);
            let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
            // Émission régulière de la progression.
            let reporter = {
                let (app, ctx, done) = (app.clone(), ctx.clone(), done.clone());
                std::thread::spawn(move || {
                    while !done.load(std::sync::atomic::Ordering::Relaxed) {
                        let _ = app.emit("scan:progress", scan::snapshot(&ctx, started, expected));
                        std::thread::sleep(Duration::from_millis(120));
                    }
                })
            };
            let result = scan::run(&root, mode, &ctx);
            done.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = reporter.join();
            match result {
                Ok(tree) => {
                    let s = summary(&tree);
                    // Libère l'ancien arbre avant d'installer le nouveau.
                    *tree_slot.write() = Some(tree);
                    let _ = app.emit("scan:done", s);
                }
                Err(e) => {
                    let _ = app.emit("scan:error", e);
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn cancel_scan(state: State<'_, AppState>) {
    if let Some(ctx) = state.scan.lock().as_ref() {
        ctx.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[tauri::command]
async fn get_summary(state: State<'_, AppState>) -> CmdResult<Summary> {
    with_tree(&state, summary).await
}

#[tauri::command]
async fn list_dir(state: State<'_, AppState>, id: u32, limit: Option<usize>) -> CmdResult<queries::Listing> {
    with_tree(&state, move |t| {
        let id = if (id as usize) < t.len() && !t.node(id).is_deleted() { id } else { 0 };
        queries::listing(t, id, limit.unwrap_or(500))
    })
    .await
}

#[tauri::command]
async fn get_treemap(state: State<'_, AppState>, id: u32, depth: Option<u32>, min_ratio: Option<f64>) -> CmdResult<queries::MapNode> {
    with_tree(&state, move |t| {
        let id = if (id as usize) < t.len() { id } else { 0 };
        queries::treemap(t, id, depth.unwrap_or(3), min_ratio.unwrap_or(0.0015))
    })
    .await
}

#[tauri::command]
async fn get_top_files(state: State<'_, AppState>, limit: Option<usize>, category: Option<String>, under: Option<u32>) -> CmdResult<Vec<queries::NodeInfo>> {
    with_tree(&state, move |t| queries::top_files(t, limit.unwrap_or(200).min(5000), category.as_deref(), under.unwrap_or(0))).await
}

#[tauri::command]
async fn get_type_stats(state: State<'_, AppState>) -> CmdResult<queries::TypeStats> {
    with_tree(&state, |t| queries::type_stats(t, 300)).await
}

#[tauri::command]
async fn get_age_stats(state: State<'_, AppState>) -> CmdResult<Vec<queries::AgeBucket>> {
    with_tree(&state, |t| queries::age_stats(t, now())).await
}

#[tauri::command]
async fn search(state: State<'_, AppState>, query: String, limit: Option<usize>) -> CmdResult<Vec<queries::NodeInfo>> {
    with_tree(&state, move |t| queries::search(t, &query, limit.unwrap_or(300))).await
}

#[tauri::command]
async fn get_suggestions(state: State<'_, AppState>) -> CmdResult<Vec<suggestions::Suggestion>> {
    with_tree(&state, |t| suggestions::compute(t, now())).await
}

// --- Doublons ----------------------------------------------------------------

#[tauri::command]
fn find_duplicates(app: AppHandle, state: State<'_, AppState>, min_size: u64) -> CmdResult<()> {
    let ctx = Arc::new(DupCtx::default());
    {
        let mut cur = state.dup.lock();
        if let Some(prev) = cur.as_ref() {
            prev.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        *cur = Some(ctx.clone());
    }
    let tree = state.tree.clone();
    std::thread::spawn(move || {
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reporter = {
            let (app, ctx, done) = (app.clone(), ctx.clone(), done.clone());
            std::thread::spawn(move || {
                while !done.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = app.emit("dup:progress", duplicates::progress(&ctx));
                    std::thread::sleep(Duration::from_millis(200));
                }
            })
        };
        let result: Result<DupResult, String> = {
            let guard = tree.read();
            match guard.as_ref() {
                Some(t) => duplicates::find(t, min_size, &ctx),
                None => Err("Aucune analyse disponible".into()),
            }
        };
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = reporter.join();
        match result {
            Ok(r) => {
                let _ = app.emit("dup:done", r);
            }
            Err(e) => {
                let _ = app.emit("dup:error", e);
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn cancel_duplicates(state: State<'_, AppState>) {
    if let Some(ctx) = state.dup.lock().as_ref() {
        ctx.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

// --- Actions -----------------------------------------------------------------

#[tauri::command]
async fn clean(state: State<'_, AppState>, ids: Vec<u32>, mode: CleanMode) -> CmdResult<CleanResult> {
    let tree = state.tree.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (targets, refused) = {
            let guard = tree.read();
            let t = guard.as_ref().ok_or("Aucune analyse disponible")?;
            actions::plan(t, &ids, mode)
        };
        let mut result = CleanResult { failed_count: refused.len(), failed: refused, ..Default::default() };
        let removed = actions::execute(&targets, mode, &mut result);
        let mut guard = tree.write();
        if let Some(t) = guard.as_mut() {
            let before = t.node(0).alloc;
            t.remove_batch(&removed);
            result.freed = before.saturating_sub(t.node(0).alloc);
        }
        result.removed = removed.len();
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn empty_recycle_bin(state: State<'_, AppState>) -> CmdResult<u64> {
    let tree = state.tree.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = {
            let guard = tree.read();
            let t = guard.as_ref().ok_or("Aucune analyse disponible")?;
            let letter = win::drive_letter(&t.info.root_path).ok_or("Lecteur inconnu")?;
            format!("{letter}:\\")
        };
        actions::empty_recycle_bin(&root)?;
        let mut guard = tree.write();
        let mut freed = 0;
        if let Some(t) = guard.as_mut() {
            if t.info.is_volume_root {
                if let Some(rb) = t.child_by_name(0, "$Recycle.Bin") {
                    // Conserve les dossiers par utilisateur, retire leur contenu.
                    let ids: Vec<u32> = t
                        .children(rb)
                        .flat_map(|sid| t.children(sid).filter(|c| !t.name(*c).eq_ignore_ascii_case("desktop.ini")).collect::<Vec<_>>())
                        .collect();
                    let before = t.node(0).alloc;
                    t.remove_batch(&ids);
                    freed = before.saturating_sub(t.node(0).alloc);
                }
            }
        }
        Ok(freed)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn reveal_path(path: String) -> CmdResult<()> {
    actions::reveal(&path)
}

#[tauri::command]
fn open_path(path: String) -> CmdResult<()> {
    actions::open(&path)
}

#[tauri::command]
fn open_disk_cleanup(letter: String) -> CmdResult<()> {
    actions::disk_cleanup(&letter)
}

#[tauri::command]
fn restart_as_admin(app: AppHandle) -> CmdResult<()> {
    actions::restart_as_admin()?;
    app.exit(0);
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                let title = if win::is_elevated() { "CleanMyWindows — Administrateur" } else { "CleanMyWindows" };
                let _ = w.set_title(title);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_system_info,
            list_volumes,
            start_scan,
            cancel_scan,
            get_summary,
            list_dir,
            get_treemap,
            get_top_files,
            get_type_stats,
            get_age_stats,
            search,
            get_suggestions,
            find_duplicates,
            cancel_duplicates,
            clean,
            empty_recycle_bin,
            reveal_path,
            open_path,
            open_disk_cleanup,
            restart_as_admin,
        ])
        .run(tauri::generate_context!())
        .expect("Erreur au lancement de CleanMyWindows");
}
