import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

export type ScanMode = "auto" | "walk" | "mft"
export type CleanMode = "trash" | "permanent"

export interface SystemInfo {
  elevated: boolean
  systemDrive: string
  cpus: number
  version: string
}

export interface Volume {
  root: string
  letter: string
  label: string
  filesystem: string
  kind: "fixed" | "removable" | "network" | "cdrom" | "ramdisk"
  total: number
  free: number
  isSystem: boolean
  isSsd: boolean | null
}

export interface ScanProgress {
  phase: "scanning" | "mft" | "building"
  files: number
  dirs: number
  bytes: number
  denied: number
  current: string
  elapsedMs: number
  percent: number | null
}

export interface ScanInfo {
  rootPath: string
  method: "mft" | "layout" | "parallel"
  elapsedMs: number
  volumeTotal: number
  volumeFree: number
  isVolumeRoot: boolean
  denied: number
  elevated: boolean
  threads: number
}

export type Category =
  | "images"
  | "videos"
  | "audio"
  | "documents"
  | "archives"
  | "applications"
  | "code"
  | "system"
  | "temporary"
  | "other"
  | "folder"

export interface NodeInfo {
  id: number
  name: string
  isDir: boolean
  size: number
  alloc: number
  mtime: number
  files: number
  dirs: number
  category: Category
  hasChildren: boolean
  hidden: boolean
  system: boolean
  denied: boolean
  link: boolean
  cloud: boolean
  meta: boolean
  path?: string
}

export interface Summary {
  info: ScanInfo
  root: NodeInfo
  nodes: number
}

export interface Listing {
  node: NodeInfo
  breadcrumbs: { id: number; name: string }[]
  children: NodeInfo[]
  totalChildren: number
}

export interface MapNode {
  id: number
  name: string
  value: number
  category: Category | "grouped"
  isDir: boolean
  grouped?: number
  children?: MapNode[]
}

export interface TypeStats {
  categories: { category: Category; count: number; alloc: number }[]
  extensions: { ext: string; category: Category; count: number; alloc: number }[]
}

export interface AgeBucket {
  label: string
  count: number
  alloc: number
}

export type Safety = "safe" | "review" | "info"

export interface SuggestionItem {
  id: number
  name: string
  path: string
  alloc: number
  mtime: number
  isDir: boolean
  admin: boolean
  note?: string
}

export interface Suggestion {
  id: string
  title: string
  description: string
  icon: string
  safety: Safety
  action: "delete" | "emptyRecycleBin" | "diskCleanup" | "none"
  admin: boolean
  total: number
  count: number
  truncated: boolean
  items: SuggestionItem[]
  advice?: string
}

export interface CleanResult {
  freed: number
  removed: number
  failedCount: number
  failed: { path: string; error: string }[]
}

export interface DupProgress {
  phase: "sizes" | "partial" | "full"
  done: number
  total: number
  bytes: number
}

export interface DupGroup {
  hash: string
  size: number
  alloc: number
  wasted: number
  files: { id: number; name: string; path: string; mtime: number }[]
}

export interface DupResult {
  groups: DupGroup[]
  totalGroups: number
  wasted: number
  scannedFiles: number
}

export const api = {
  systemInfo: () => invoke<SystemInfo>("get_system_info"),
  listVolumes: () => invoke<Volume[]>("list_volumes"),
  startScan: (path: string, mode: ScanMode = "auto") => invoke<void>("start_scan", { path, mode }),
  cancelScan: () => invoke<void>("cancel_scan"),
  summary: () => invoke<Summary>("get_summary"),
  listDir: (id: number, limit = 500) => invoke<Listing>("list_dir", { id, limit }),
  treemap: (id: number, depth = 3, minRatio = 0.0015) => invoke<MapNode>("get_treemap", { id, depth, minRatio }),
  topFiles: (limit = 300, category?: string, under?: number) =>
    invoke<NodeInfo[]>("get_top_files", { limit, category: category ?? null, under: under ?? null }),
  typeStats: () => invoke<TypeStats>("get_type_stats"),
  ageStats: () => invoke<AgeBucket[]>("get_age_stats"),
  search: (query: string, limit = 300) => invoke<NodeInfo[]>("search", { query, limit }),
  suggestions: () => invoke<Suggestion[]>("get_suggestions"),
  findDuplicates: (minSize: number) => invoke<void>("find_duplicates", { minSize }),
  cancelDuplicates: () => invoke<void>("cancel_duplicates"),
  clean: (ids: number[], mode: CleanMode) => invoke<CleanResult>("clean", { ids, mode }),
  emptyRecycleBin: () => invoke<number>("empty_recycle_bin"),
  reveal: (path: string) => invoke<void>("reveal_path", { path }),
  open: (path: string) => invoke<void>("open_path", { path }),
  diskCleanup: (letter: string) => invoke<void>("open_disk_cleanup", { letter }),
  restartAsAdmin: () => invoke<void>("restart_as_admin"),
}

export function on<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => handler(e.payload))
}
