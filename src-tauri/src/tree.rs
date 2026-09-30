//! Arbre de fichiers compact en mémoire (arène).
//!
//! Chaque nœud tient dans ~56 octets et tous les noms sont stockés dans un seul
//! grand `String`. Les enfants d'un dossier occupent une plage contiguë de
//! `order`, triée par taille sur disque décroissante. Les identifiants sont
//! attribués en largeur d'abord : un parent a toujours un id inférieur à ses
//! enfants, ce qui permet d'agréger les tailles en un seul passage inverse.

use rayon::prelude::*;
use serde::Serialize;

use crate::categories::{CAT_DIR, category_of};

pub const F_DIR: u16 = 1;
pub const F_REPARSE: u16 = 2;
pub const F_HIDDEN: u16 = 4;
pub const F_SYSTEM: u16 = 8;
pub const F_DENIED: u16 = 16;
pub const F_DELETED: u16 = 32;
pub const F_META: u16 = 64;
pub const F_CLOUD: u16 = 128;
pub const F_COMPRESSED: u16 = 256;

#[derive(Clone, Copy, Default, Debug)]
pub struct Node {
    pub parent: u32,
    /// Index de début dans `order` (valide si `count > 0`).
    pub first: u32,
    pub count: u32,
    pub name_off: u32,
    pub name_len: u32,
    pub flags: u16,
    pub cat: u8,
    /// Taille logique (agrégée pour les dossiers).
    pub size: u64,
    /// Taille réellement occupée sur le disque (agrégée pour les dossiers).
    pub alloc: u64,
    /// Date de dernière modification (secondes Unix).
    pub mtime: i64,
    /// Nombre de fichiers (agrégé ; 1 pour un fichier).
    pub files: u32,
    /// Nombre de dossiers (agrégé, dossier lui-même inclus).
    pub dirs: u32,
}

impl Node {
    #[inline]
    pub fn is_dir(&self) -> bool {
        self.flags & F_DIR != 0
    }
    #[inline]
    pub fn is_deleted(&self) -> bool {
        self.flags & F_DELETED != 0
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanInfo {
    pub root_path: String,
    pub method: String,
    pub elapsed_ms: u64,
    pub volume_total: u64,
    pub volume_free: u64,
    pub is_volume_root: bool,
    pub denied: u64,
    pub elevated: bool,
    pub threads: usize,
}

pub struct Tree {
    pub nodes: Vec<Node>,
    pub order: Vec<u32>,
    pub names: String,
    pub info: ScanInfo,
}

pub struct TreeBuilder {
    pub nodes: Vec<Node>,
    pub order: Vec<u32>,
    pub names: String,
}

impl TreeBuilder {
    pub fn with_capacity(nodes: usize, name_bytes: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(nodes),
            order: Vec::with_capacity(nodes),
            names: String::with_capacity(name_bytes),
        }
    }

    /// Ajoute un nœud et renvoie son id. Les enfants d'un même dossier doivent
    /// être ajoutés consécutivement, puis déclarés via [`set_children`].
    #[inline]
    pub fn push(&mut self, parent: u32, name: &str, size: u64, alloc: u64, mtime: i64, flags: u16) -> u32 {
        let id = self.nodes.len() as u32;
        let name_off = self.names.len() as u32;
        self.names.push_str(name);
        let is_dir = flags & F_DIR != 0;
        self.nodes.push(Node {
            parent,
            first: 0,
            count: 0,
            name_off,
            name_len: name.len() as u32,
            flags,
            cat: 0,
            size,
            alloc,
            mtime,
            files: u32::from(!is_dir),
            dirs: u32::from(is_dir),
        });
        self.order.push(id);
        id
    }

    #[inline]
    pub fn set_children(&mut self, id: u32, first: u32, count: u32) {
        let n = &mut self.nodes[id as usize];
        n.first = first;
        n.count = count;
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Agrège les tailles, trie les enfants et calcule les catégories.
    pub fn finish(mut self, info: ScanInfo) -> Tree {
        let nodes = &mut self.nodes;
        // 1. Agrégation ascendante (les enfants ont toujours un id supérieur).
        for id in (1..nodes.len()).rev() {
            let n = nodes[id];
            if n.is_deleted() {
                continue;
            }
            let p = &mut nodes[n.parent as usize];
            p.size += n.size;
            p.alloc += n.alloc;
            p.files += n.files;
            p.dirs += n.dirs;
            // Pour un dossier, mtime devient la date d'activité la plus récente du sous-arbre.
            if n.mtime > p.mtime {
                p.mtime = n.mtime;
            }
        }

        // 2. Catégories (en parallèle).
        let names = &self.names;
        nodes.par_iter_mut().for_each(|n| {
            n.cat = if n.is_dir() {
                CAT_DIR
            } else {
                category_of(&names[n.name_off as usize..(n.name_off + n.name_len) as usize])
            };
        });

        // 3. Tri des enfants par taille décroissante (plages disjointes, en parallèle).
        let mut ranges: Vec<(u32, u32)> = nodes
            .iter()
            .filter(|n| n.count > 1)
            .map(|n| (n.first, n.count))
            .collect();
        ranges.sort_unstable_by_key(|r| r.0);
        let mut slices: Vec<&mut [u32]> = Vec::with_capacity(ranges.len());
        let mut rest: &mut [u32] = &mut self.order;
        let mut consumed = 0u32;
        for (first, count) in ranges {
            let tail = std::mem::take(&mut rest);
            let (_, tail) = tail.split_at_mut((first - consumed) as usize);
            let (slice, tail) = tail.split_at_mut(count as usize);
            slices.push(slice);
            rest = tail;
            consumed = first + count;
        }
        let nodes_ref = &*nodes;
        slices.par_iter_mut().for_each(|s| {
            s.sort_unstable_by(|a, b| nodes_ref[*b as usize].alloc.cmp(&nodes_ref[*a as usize].alloc));
        });

        Tree {
            nodes: self.nodes,
            order: self.order,
            names: self.names,
            info,
        }
    }
}

impl Tree {
    #[inline]
    pub fn name(&self, id: u32) -> &str {
        let n = &self.nodes[id as usize];
        &self.names[n.name_off as usize..(n.name_off + n.name_len) as usize]
    }

    #[inline]
    pub fn node(&self, id: u32) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Enfants non supprimés, du plus gros au plus petit.
    pub fn children(&self, id: u32) -> impl Iterator<Item = u32> + '_ {
        let n = &self.nodes[id as usize];
        let range = if n.count == 0 {
            &[][..]
        } else {
            &self.order[n.first as usize..(n.first + n.count) as usize]
        };
        range
            .iter()
            .copied()
            .filter(move |c| !self.nodes[*c as usize].is_deleted())
    }

    pub fn child_by_name(&self, id: u32, name: &str) -> Option<u32> {
        self.children(id).find(|c| self.name(*c).eq_ignore_ascii_case(name))
    }

    /// Résout un chemin relatif à la racine (`Users\Bob\AppData`).
    pub fn resolve_rel(&self, rel: &str) -> Option<u32> {
        let mut cur = 0u32;
        for part in rel.split('\\').filter(|p| !p.is_empty()) {
            cur = self.child_by_name(cur, part)?;
        }
        Some(cur)
    }

    /// Chemin complet d'un nœud.
    pub fn path(&self, id: u32) -> String {
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != 0 {
            parts.push(self.name(cur));
            cur = self.nodes[cur as usize].parent;
        }
        let mut path = self.name(0).to_string();
        for part in parts.iter().rev() {
            if !path.ends_with('\\') {
                path.push('\\');
            }
            path.push_str(part);
        }
        path
    }

    /// Chaîne des ancêtres, de la racine jusqu'au nœud inclus.
    pub fn ancestry(&self, id: u32) -> Vec<u32> {
        let mut chain = vec![id];
        let mut cur = id;
        while cur != 0 {
            cur = self.nodes[cur as usize].parent;
            chain.push(cur);
        }
        chain.reverse();
        chain
    }

    pub fn is_ancestor(&self, ancestor: u32, mut id: u32) -> bool {
        while id != 0 {
            if id == ancestor {
                return true;
            }
            id = self.nodes[id as usize].parent;
        }
        ancestor == 0
    }

    /// Retire un nœud (et son sous-arbre) de l'arbre après suppression sur disque.
    pub fn remove(&mut self, id: u32) {
        if id == 0 || self.nodes[id as usize].is_deleted() {
            return;
        }
        let removed = self.nodes[id as usize];
        // Marque tout le sous-arbre.
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            let n = self.nodes[cur as usize];
            self.nodes[cur as usize].flags |= F_DELETED;
            if n.count > 0 {
                stack.extend_from_slice(&self.order[n.first as usize..(n.first + n.count) as usize]);
            }
        }
        // Met à jour les ancêtres puis retrie leurs fratries.
        let mut cur = removed.parent;
        loop {
            let p = &mut self.nodes[cur as usize];
            p.size = p.size.saturating_sub(removed.size);
            p.alloc = p.alloc.saturating_sub(removed.alloc);
            p.files = p.files.saturating_sub(removed.files);
            p.dirs = p.dirs.saturating_sub(removed.dirs);
            if cur == 0 {
                break;
            }
            cur = p.parent;
        }
        let mut cur = removed.parent;
        while cur != 0 {
            let parent = self.nodes[cur as usize].parent;
            self.sort_children(parent);
            cur = parent;
        }
    }

    /// Retire plusieurs nœuds en une passe (un seul re-tri par dossier touché).
    pub fn remove_batch(&mut self, ids: &[u32]) {
        let mut touched = std::collections::HashSet::new();
        for &id in ids {
            if id == 0 || self.nodes[id as usize].is_deleted() {
                continue;
            }
            let removed = self.nodes[id as usize];
            let mut stack = vec![id];
            while let Some(cur) = stack.pop() {
                let n = self.nodes[cur as usize];
                if n.is_deleted() {
                    continue;
                }
                self.nodes[cur as usize].flags |= F_DELETED;
                if n.count > 0 {
                    stack.extend_from_slice(&self.order[n.first as usize..(n.first + n.count) as usize]);
                }
            }
            let mut cur = removed.parent;
            loop {
                let p = &mut self.nodes[cur as usize];
                p.size = p.size.saturating_sub(removed.size);
                p.alloc = p.alloc.saturating_sub(removed.alloc);
                p.files = p.files.saturating_sub(removed.files);
                p.dirs = p.dirs.saturating_sub(removed.dirs);
                touched.insert(cur);
                if cur == 0 {
                    break;
                }
                cur = p.parent;
            }
        }
        for id in touched {
            if id != 0 {
                let parent = self.nodes[id as usize].parent;
                self.sort_children(parent);
            }
            self.sort_children(id);
        }
    }

    fn sort_children(&mut self, id: u32) {
        let n = self.nodes[id as usize];
        if n.count > 1 {
            let nodes = &self.nodes;
            self.order[n.first as usize..(n.first + n.count) as usize]
                .sort_unstable_by(|a, b| nodes[*b as usize].alloc.cmp(&nodes[*a as usize].alloc));
        }
    }

    /// Itère sur tous les nœuds vivants (hors racine).
    pub fn live_ids(&self) -> impl Iterator<Item = u32> + '_ {
        (1..self.nodes.len() as u32).filter(|i| !self.nodes[*i as usize].is_deleted())
    }
}
