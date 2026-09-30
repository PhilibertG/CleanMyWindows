import type { Category } from "./api"

const UNITS = ["o", "Ko", "Mo", "Go", "To", "Po"]
const nf = new Intl.NumberFormat("fr-FR")

export function formatBytes(bytes: number, digits = 1): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 o"
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), UNITS.length - 1)
  const v = bytes / 1024 ** i
  const d = i === 0 ? 0 : v >= 100 ? 0 : digits
  return `${v.toLocaleString("fr-FR", { minimumFractionDigits: d, maximumFractionDigits: d })} ${UNITS[i]}`
}

export function formatNumber(n: number): string {
  return nf.format(n)
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`
  const s = ms / 1000
  if (s < 60) return `${s.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} s`
  const m = Math.floor(s / 60)
  return `${m} min ${Math.round(s % 60)} s`
}

const rtf = new Intl.RelativeTimeFormat("fr-FR", { numeric: "auto" })

export function formatAge(unixSeconds: number): string {
  if (!unixSeconds) return "—"
  const diff = unixSeconds - Date.now() / 1000
  const abs = Math.abs(diff)
  if (abs < 3600) return rtf.format(Math.round(diff / 60), "minute")
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), "hour")
  if (abs < 86400 * 30) return rtf.format(Math.round(diff / 86400), "day")
  if (abs < 86400 * 365) return rtf.format(Math.round(diff / (86400 * 30)), "month")
  return rtf.format(Math.round(diff / (86400 * 365)), "year")
}

export function formatDate(unixSeconds: number): string {
  if (!unixSeconds) return "—"
  return new Date(unixSeconds * 1000).toLocaleDateString("fr-FR", { day: "2-digit", month: "short", year: "numeric" })
}

export function percent(part: number, total: number): number {
  return total > 0 ? (part / total) * 100 : 0
}

export function formatPercent(part: number, total: number): string {
  const p = percent(part, total)
  if (p > 0 && p < 0.1) return "< 0,1 %"
  return `${p.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`
}

export const CATEGORY_LABELS: Record<Category | "grouped", string> = {
  images: "Images",
  videos: "Vidéos",
  audio: "Audio",
  documents: "Documents",
  archives: "Archives et images disque",
  applications: "Applications et jeux",
  code: "Code et développement",
  system: "Fichiers système",
  temporary: "Temporaires et journaux",
  other: "Autres",
  folder: "Dossiers",
  grouped: "Éléments regroupés",
}

/**
 * Couleurs des catégories : les 8 teintes catégorielles validées (ordre fixe),
 * « système » et « autres » en neutres. Définies en CSS (--cat-*) pour le mode sombre.
 */
export const CATEGORY_ORDER: Category[] = [
  "applications",
  "archives",
  "code",
  "images",
  "videos",
  "audio",
  "documents",
  "temporary",
  "system",
  "other",
]

export function categoryVar(cat: Category | "grouped"): string {
  return `var(--cat-${cat})`
}
