//! Requêtes de lecture sur l'arbre (exploration, treemap, top fichiers, types).

use std::collections::{BinaryHeap, HashMap};
use std::cmp::Reverse;

use serde::Serialize;

use crate::categories::{CAT_DIR, CATEGORY_COUNT, category_name, extension};
use crate::tree::{F_CLOUD, F_DENIED, F_HIDDEN, F_META, F_REPARSE, F_SYSTEM, Tree};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo {
    pub id: u32,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub alloc: u64,
    pub mtime: i64,
    pub files: u32,
    pub dirs: u32,
    pub category: &'static str,
    pub has_children: bool,
    pub hidden: bool,
    pub system: bool,
    pub denied: bool,
    pub link: bool,
    pub cloud: bool,
    pub meta: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

pub fn info(tree: &Tree, id: u32, with_path: bool) -> NodeInfo {
    let n = tree.node(id);
    NodeInfo {
        id,
        name: tree.name(id).to_string(),
        is_dir: n.is_dir(),
        size: n.size,
        alloc: n.alloc,
        mtime: n.mtime,
        files: n.files,
        dirs: n.dirs.saturating_sub(u32::from(n.is_dir())),
        category: category_name(n.cat),
        has_children: tree.children(id).next().is_some(),
        hidden: n.flags & F_HIDDEN != 0,
        system: n.flags & F_SYSTEM != 0,
        denied: n.flags & F_DENIED != 0,
        link: n.flags & F_REPARSE != 0,
        cloud: n.flags & F_CLOUD != 0,
        meta: n.flags & F_META != 0,
        path: with_path.then(|| tree.path(id)),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Crumb {
    pub id: u32,
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub node: NodeInfo,
    pub breadcrumbs: Vec<Crumb>,
    pub children: Vec<NodeInfo>,
    pub total_children: usize,
}

pub fn listing(tree: &Tree, id: u32, limit: usize) -> Listing {
    let breadcrumbs = tree
        .ancestry(id)
        .into_iter()
        .map(|a| Crumb { id: a, name: tree.name(a).to_string() })
        .collect();
    let all: Vec<u32> = tree.children(id).collect();
    Listing {
        node: info(tree, id, true),
        breadcrumbs,
        total_children: all.len(),
        children: all.iter().take(limit).map(|c| info(tree, *c, false)).collect(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapNode {
    pub id: u32,
    pub name: String,
    pub value: u64,
    pub category: &'static str,
    pub is_dir: bool,
    /// Nombre d'éléments regroupés (pour le bloc « autres »).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grouped: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<MapNode>,
}

/// Sous-arbre pour la carte des tailles : on ne descend que dans les éléments
/// suffisamment gros pour être visibles, le reste est regroupé.
pub fn treemap(tree: &Tree, id: u32, depth: u32, min_ratio: f64) -> MapNode {
    let root_alloc = tree.node(id).alloc.max(1);
    let min = (root_alloc as f64 * min_ratio) as u64;
    let mut budget = 3000usize;
    build_map(tree, id, depth, min, &mut budget)
}

fn build_map(tree: &Tree, id: u32, depth: u32, min: u64, budget: &mut usize) -> MapNode {
    let n = tree.node(id);
    let mut node = MapNode {
        id,
        name: tree.name(id).to_string(),
        value: n.alloc,
        category: category_name(n.cat),
        is_dir: n.is_dir(),
        grouped: None,
        children: Vec::new(),
    };
    if depth == 0 || !n.is_dir() {
        return node;
    }
    let mut rest = 0u64;
    let mut rest_count = 0u32;
    for c in tree.children(id) {
        let cn = tree.node(c);
        if cn.alloc == 0 {
            continue;
        }
        if cn.alloc >= min && *budget > 0 {
            *budget -= 1;
            node.children.push(build_map(tree, c, depth - 1, min, budget));
        } else {
            rest += cn.alloc;
            rest_count += 1;
        }
    }
    if rest > 0 && !node.children.is_empty() {
        node.children.push(MapNode {
            id: u32::MAX,
            name: format!("{rest_count} autres éléments"),
            value: rest,
            category: "grouped",
            is_dir: false,
            grouped: Some(rest_count),
            children: Vec::new(),
        });
    }
    node
}

/// Les `limit` plus gros fichiers, éventuellement filtrés par catégorie.
pub fn top_files(tree: &Tree, limit: usize, category: Option<&str>, under: u32) -> Vec<NodeInfo> {
    let mut heap: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::with_capacity(limit + 1);
    for id in tree.live_ids() {
        let n = tree.node(id);
        if n.is_dir() || n.alloc == 0 {
            continue;
        }
        if let Some(cat) = category {
            if category_name(n.cat) != cat {
                continue;
            }
        }
        let candidate = heap.len() < limit || heap.peek().is_some_and(|Reverse((min, _))| n.alloc > *min);
        if !candidate || (under != 0 && !tree.is_ancestor(under, id)) {
            continue;
        }
        if heap.len() >= limit {
            heap.pop();
        }
        heap.push(Reverse((n.alloc, id)));
    }
    let mut ids: Vec<(u64, u32)> = heap.into_iter().map(|Reverse(v)| v).collect();
    ids.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    ids.into_iter().map(|(_, id)| info(tree, id, true)).collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtStat {
    pub ext: String,
    pub category: &'static str,
    pub count: u64,
    pub alloc: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryStat {
    pub category: &'static str,
    pub count: u64,
    pub alloc: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeStats {
    pub categories: Vec<CategoryStat>,
    pub extensions: Vec<ExtStat>,
}

pub fn type_stats(tree: &Tree, limit: usize) -> TypeStats {
    let mut cats = [(0u64, 0u64); CATEGORY_COUNT];
    let mut exts: HashMap<String, (u8, u64, u64)> = HashMap::new();
    for id in tree.live_ids() {
        let n = tree.node(id);
        if n.is_dir() || n.cat == CAT_DIR {
            continue;
        }
        let c = &mut cats[n.cat as usize];
        c.0 += 1;
        c.1 += n.alloc;
        let ext = extension(tree.name(id)).unwrap_or_else(|| "(sans extension)".into());
        let e = exts.entry(ext).or_insert((n.cat, 0, 0));
        e.1 += 1;
        e.2 += n.alloc;
    }
    let mut categories: Vec<CategoryStat> = cats
        .iter()
        .enumerate()
        .map(|(i, (count, alloc))| CategoryStat { category: category_name(i as u8), count: *count, alloc: *alloc })
        .filter(|c| c.count > 0)
        .collect();
    categories.sort_unstable_by(|a, b| b.alloc.cmp(&a.alloc));
    let mut extensions: Vec<ExtStat> = exts
        .into_iter()
        .map(|(ext, (cat, count, alloc))| ExtStat { ext, category: category_name(cat), count, alloc })
        .collect();
    extensions.sort_unstable_by(|a, b| b.alloc.cmp(&a.alloc));
    extensions.truncate(limit);
    TypeStats { categories, extensions }
}

/// Recherche par nom (insensible à la casse), résultats triés par taille.
pub fn search(tree: &Tree, query: &str, limit: usize) -> Vec<NodeInfo> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let ascii = q.is_ascii();
    let mut hits: Vec<(u64, u32)> = Vec::new();
    for id in tree.live_ids() {
        let name = tree.name(id);
        let found = if ascii {
            name.len() >= q.len()
                && name.as_bytes().windows(q.len()).any(|w| w.eq_ignore_ascii_case(q.as_bytes()))
        } else {
            name.to_lowercase().contains(&q)
        };
        if found {
            hits.push((tree.node(id).alloc, id));
        }
    }
    hits.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    hits.truncate(limit);
    hits.into_iter().map(|(_, id)| info(tree, id, true)).collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgeBucket {
    pub label: &'static str,
    pub count: u64,
    pub alloc: u64,
}

/// Répartition de l'espace selon l'ancienneté de dernière modification.
pub fn age_stats(tree: &Tree, now: i64) -> Vec<AgeBucket> {
    const DAY: i64 = 86_400;
    let limits: [(&str, i64); 5] = [
        ("Moins d'un mois", 30 * DAY),
        ("1 à 6 mois", 182 * DAY),
        ("6 mois à 1 an", 365 * DAY),
        ("1 à 3 ans", 3 * 365 * DAY),
        ("Plus de 3 ans", i64::MAX),
    ];
    let mut out: Vec<AgeBucket> = limits.iter().map(|(l, _)| AgeBucket { label: l, count: 0, alloc: 0 }).collect();
    for id in tree.live_ids() {
        let n = tree.node(id);
        if n.is_dir() {
            continue;
        }
        let age = (now - n.mtime).max(0);
        let idx = limits.iter().position(|(_, max)| age < *max).unwrap_or(4);
        out[idx].count += 1;
        out[idx].alloc += n.alloc;
    }
    out
}
