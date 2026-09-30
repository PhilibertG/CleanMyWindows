import { useEffect, useRef, useState } from "react"
import { Files, Folder, HardDrive, Timer, X, Zap } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Progress } from "@/components/ui/progress"
import { Spinner } from "@/components/ui/spinner"
import { formatBytes, formatDuration, formatNumber } from "@/lib/format"
import { useStore } from "@/lib/store"

const PHASES: Record<string, string> = {
  scanning: "Parcours des dossiers",
  mft: "Lecture de la table des fichiers (mode Turbo)",
  building: "Construction de l'arbre",
}

export function ScanningView() {
  const { progress, target, cancelScan, system } = useStore()
  const [rate, setRate] = useState(0)
  const last = useRef<{ files: number; t: number } | null>(null)

  useEffect(() => {
    if (!progress) return
    const now = progress.elapsedMs
    if (last.current && now - last.current.t >= 500) {
      setRate(((progress.files - last.current.files) * 1000) / (now - last.current.t))
      last.current = { files: progress.files, t: now }
    } else if (!last.current) {
      last.current = { files: progress.files, t: now }
    }
  }, [progress])

  const pct = progress?.percent ?? null
  const stats = [
    { icon: Files, label: progress?.phase === "mft" ? "Enregistrements" : "Fichiers", value: formatNumber(progress?.files ?? 0) },
    { icon: Folder, label: "Dossiers", value: formatNumber(progress?.dirs ?? 0) },
    { icon: HardDrive, label: "Analysé", value: formatBytes(progress?.bytes ?? 0) },
    { icon: Timer, label: "Durée", value: formatDuration(progress?.elapsedMs ?? 0) },
  ]

  return (
    <div className="mx-auto flex h-full w-full max-w-3xl flex-col items-center justify-center gap-8 py-10">
      <div className="relative flex size-40 items-center justify-center">
        <div className="hero-gradient absolute inset-0 animate-ping rounded-full opacity-20 [animation-duration:2.4s]" />
        <div className="hero-gradient absolute inset-3 rounded-full opacity-30 blur-md" />
        <div className="hero-gradient relative flex size-28 flex-col items-center justify-center rounded-full text-white shadow-xl">
          {pct !== null ? (
            <span className="text-3xl font-semibold tabular">{Math.floor(pct)}%</span>
          ) : (
            <Spinner className="size-8" />
          )}
        </div>
      </div>

      <div className="text-center">
        <h1 className="text-2xl font-semibold tracking-tight">Analyse de {target}</h1>
        <p className="mt-1 flex items-center justify-center gap-1.5 text-sm text-muted-foreground">
          {progress?.phase === "mft" && <Zap className="size-4 text-primary" />}
          {PHASES[progress?.phase ?? "scanning"]}
          {rate > 0 && progress?.phase === "scanning" && <> • {formatNumber(Math.round(rate))} fichiers/s</>}
        </p>
      </div>

      <div className="w-full space-y-2">
        <Progress value={pct ?? undefined} className={pct === null ? "animate-pulse" : ""} />
        <p className="truncate text-center font-mono text-xs text-muted-foreground" title={progress?.current}>
          {progress?.current || "Préparation…"}
        </p>
      </div>

      <div className="grid w-full grid-cols-2 gap-3 sm:grid-cols-4">
        {stats.map((s) => (
          <Card key={s.label} className="gap-1 py-4">
            <CardContent className="px-4">
              <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                <s.icon className="size-3.5" />
                {s.label}
              </div>
              <div className="mt-1 text-lg font-semibold tabular">{s.value}</div>
            </CardContent>
          </Card>
        ))}
      </div>

      {!system?.elevated && (
        <p className="max-w-lg text-center text-xs text-muted-foreground">
          Astuce : le mode Turbo (administrateur) lit directement la table des fichiers NTFS et accélère fortement l'analyse des
          disques entiers.
        </p>
      )}

      <Button variant="outline" onClick={cancelScan}>
        <X /> Annuler l'analyse
      </Button>
    </div>
  )
}
