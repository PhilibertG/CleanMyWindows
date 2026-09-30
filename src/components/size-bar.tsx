import { cn } from "@/lib/utils"

/** Barre de proportion fine, extrémité arrondie ancrée à gauche. */
export function SizeBar({ value, max, color, className }: { value: number; max: number; color?: string; className?: string }) {
  const pct = max > 0 ? Math.max((value / max) * 100, value > 0 ? 0.8 : 0) : 0
  return (
    <div className={cn("h-1.5 w-full overflow-hidden rounded-full bg-muted", className)}>
      <div className="h-full rounded-full transition-[width] duration-500" style={{ width: `${pct}%`, background: color ?? "var(--primary)" }} />
    </div>
  )
}

/** Jauge d'occupation d'un disque : analysé / non analysé / libre. */
export function DiskGauge({ used, total, scanned }: { used: number; total: number; scanned?: number }) {
  const usedPct = total > 0 ? (used / total) * 100 : 0
  const scannedPct = scanned !== undefined && total > 0 ? (Math.min(scanned, used) / total) * 100 : usedPct
  const danger = usedPct > 90
  return (
    <div className="relative h-2.5 w-full overflow-hidden rounded-full bg-muted">
      {scanned !== undefined && (
        <div className="absolute inset-y-0 left-0 rounded-full bg-muted-foreground/30" style={{ width: `${usedPct}%` }} />
      )}
      <div
        className={cn("absolute inset-y-0 left-0 rounded-full transition-[width] duration-700", danger ? "bg-destructive" : "hero-gradient")}
        style={{ width: `${scannedPct}%` }}
      />
    </div>
  )
}
