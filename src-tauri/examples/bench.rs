//! Banc d'essai du moteur d'analyse (LECTURE SEULE, aucune suppression).
//! Usage : cargo run --release --example bench -- C:\ [auto|walk|mft|compare]

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use cmw_lib::scan::{self, ScanCtx, ScanMode};
use cmw_lib::tree::Tree;

fn human(b: u64) -> String {
    let units = ["o", "Ko", "Mo", "Go", "To"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.2} {}", units[i])
}

fn run(root: &str, mode: ScanMode, verbose: bool) -> Option<Tree> {
    let ctx = Arc::new(ScanCtx::default());
    let started = Instant::now();
    let c2 = ctx.clone();
    let reporter = std::thread::spawn(move || {
        while c2.phase.load(Ordering::Relaxed) != scan::PHASE_BUILDING {
            std::thread::sleep(Duration::from_millis(1000));
            if verbose {
                let p = scan::snapshot(&c2, started, 0);
                eprintln!("  [{:>5} ms] {} {} fichiers, {} dossiers, {}", p.elapsed_ms, p.phase, p.files, p.dirs, human(p.bytes));
            }
        }
    });
    let res = scan::run(root, mode, &ctx);
    ctx.phase.store(scan::PHASE_BUILDING, Ordering::Relaxed);
    reporter.join().ok();
    match res {
        Ok(tree) => {
            let r = tree.node(0);
            println!("== Méthode {} ({} threads) : {} ms", tree.info.method, tree.info.threads, tree.info.elapsed_ms);
            println!("   Fichiers {} | Dossiers {} | Taille {} | Sur disque {} | Inaccessibles {}",
                r.files, r.dirs, human(r.size), human(r.alloc), tree.info.denied);
            if tree.info.volume_total > 0 {
                let used = tree.info.volume_total - tree.info.volume_free;
                println!("   Volume utilisé {} (écart {})", human(used), human(used.saturating_sub(r.alloc)));
            }
            Some(tree)
        }
        Err(e) => {
            println!("Erreur : {e}");
            None
        }
    }
}

fn top(tree: &Tree, id: u32) -> HashMap<String, (u64, u32)> {
    tree.children(id)
        .map(|c| (tree.name(c).to_lowercase(), (tree.node(c).alloc, tree.node(c).files)))
        .collect()
}

fn compare(a: &Tree, b: &Tree, rel: &str) {
    let (Some(ia), Some(ib)) = (a.resolve_rel(rel), b.resolve_rel(rel)) else {
        println!("  (introuvable : {rel})");
        return;
    };
    println!("-- Comparaison de '{}' (MFT vs parcours)", if rel.is_empty() { "\\" } else { rel });
    let ta = top(a, ia);
    let tb = top(b, ib);
    let mut names: Vec<&String> = ta.keys().chain(tb.keys()).collect();
    names.sort();
    names.dedup();
    names.sort_by_key(|n| std::cmp::Reverse(ta.get(*n).or(tb.get(*n)).map(|v| v.0).unwrap_or(0)));
    for n in names.into_iter().take(15) {
        let va = ta.get(n).copied().unwrap_or_default();
        let vb = tb.get(n).copied().unwrap_or_default();
        let flag = if va.1 != vb.1 || va.0.abs_diff(vb.0) > 1 << 20 { "  <--" } else { "" };
        println!("   {:<32} {:>12} {:>9} f | {:>12} {:>9} f{flag}", n, human(va.0), va.1, human(vb.0), vb.1);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = args.get(1).cloned().unwrap_or_else(|| "C:\\".into());
    match args.get(2).map(String::as_str) {
        Some("suggest") => {
            // Liste les propositions SANS rien supprimer.
            let Some(tree) = run(&root, ScanMode::Auto, false) else { return };
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
            let t = Instant::now();
            let list = cmw_lib::suggestions::compute(&tree, now);
            println!("-- Propositions calculées en {:?}", t.elapsed());
            let mut total = 0;
            for s in &list {
                total += if s.safety == "info" { 0 } else { s.total };
                println!("[{:<6}] {:<40} {:>12} ({} éléments)", s.safety, s.title, human(s.total), s.count);
                for i in s.items.iter().take(4) {
                    println!("           {:>12}  {}{}", human(i.alloc), i.path, i.note.as_deref().map(|n| format!("  — {n}")).unwrap_or_default());
                }
            }
            println!("-- Total récupérable (hors info) : {}", human(total));
            let t = Instant::now();
            let stats = cmw_lib::queries::type_stats(&tree, 10);
            println!("-- Types ({:?}) :", t.elapsed());
            for c in &stats.categories {
                println!("   {:<14} {:>12} {:>9} fichiers", c.category, human(c.alloc), c.count);
            }
            let t = Instant::now();
            let top = cmw_lib::queries::top_files(&tree, 200, None, 0);
            println!("-- Top fichiers ({:?}) : {}", t.elapsed(), top.first().and_then(|f| f.path.clone()).unwrap_or_default());
            let t = Instant::now();
            let map = cmw_lib::queries::treemap(&tree, 0, 3, 0.0015);
            println!("-- Treemap ({:?}) : {} enfants racine, json {} octets", t.elapsed(), map.children.len(), serde_json::to_string(&map).unwrap().len());
            let t = Instant::now();
            let hits = cmw_lib::queries::search(&tree, "node_modules", 50);
            println!("-- Recherche « node_modules » ({:?}) : {} résultats", t.elapsed(), hits.len());
        }
        Some("compare") => {
            let a = run(&root, ScanMode::Mft, false);
            let b = run(&root, ScanMode::Walk, false);
            if let (Some(a), Some(b)) = (a, b) {
                compare(&a, &b, "");
                compare(&a, &b, "Users");
                compare(&a, &b, "Windows");
                let tree = &a;
                println!("-- Top 10 MFT :");
                for c in tree.children(0).take(10) {
                    println!("   {:>12}  {}", human(tree.node(c).alloc), tree.path(c));
                }
            }
        }
        other => {
            let mode = match other {
                Some("mft") => ScanMode::Mft,
                Some("walk") => ScanMode::Walk,
                _ => ScanMode::Auto,
            };
            if let Some(tree) = run(&root, mode, true) {
                println!("Top 10 :");
                for c in tree.children(0).take(10) {
                    println!("  {:>12}  {}", human(tree.node(c).alloc), tree.path(c));
                }
            }
        }
    }
}
