import { open as openDialog } from "@tauri-apps/plugin-dialog"
import { FolderSearch, HardDrive, RefreshCw, Usb, Network, Zap, ScanSearch, Gauge } from "lucide-react"
import { toast } from "sonner"

import { DiskGauge } from "@/components/size-bar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { api, type Volume } from "@/lib/api"
import { formatBytes, formatPercent } from "@/lib/format"
import { useStore } from "@/lib/store"

function driveName(v: Volume) {
  const base = v.label || (v.kind === "removable" ? "Disque amovible" : v.kind === "network" ? "Lecteur réseau" : "Disque local")
  return `${base} (${v.letter}:)`
}

function DriveIcon({ v }: { v: Volume }) {
  const Icon = v.kind === "removable" ? Usb : v.kind === "network" ? Network : HardDrive
  return (
    <div className="flex size-11 items-center justify-center rounded-xl bg-primary/10 text-primary">
      <Icon className="size-5" />
    </div>
  )
}

export function HomeView() {
  const { volumes, startScan, refreshVolumes, system, summary, setView } = useStore()
  const totalUsed = volumes.reduce((s, v) => s + (v.total - v.free), 0)
  const totalSize = volumes.reduce((s, v) => s + v.total, 0)

  async function pickFolder() {
    try {
      const dir = await openDialog({ directory: true, multiple: false, title: "Choisir un dossier à analyser" })
      if (typeof dir === "string") startScan(dir)
    } catch (e) {
      toast.error(String(e))
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6">
      <section className="hero-gradient relative overflow-hidden rounded-2xl p-8 text-white shadow-lg">
        <div className="pointer-events-none absolute -top-24 -right-16 size-72 rounded-full bg-white/10 blur-2xl" />
        <div className="pointer-events-none absolute -bottom-32 left-1/3 size-72 rounded-full bg-white/10 blur-3xl" />
        <div className="relative flex flex-wrap items-end justify-between gap-6">
          <div className="max-w-xl">
            <h1 className="text-3xl font-semibold tracking-tight">Faites de la place sur vos disques</h1>
            <p className="mt-2 text-white/85">
              CleanMyWindows analyse chaque fichier en quelques secondes, vous montre ce qui prend de la place et vous propose quoi
              supprimer en toute sécurité.
            </p>
          </div>
          <div className="text-right">
            <div className="text-4xl font-semibold">{formatBytes(totalUsed)}</div>
            <div className="text-sm text-white/80">utilisés sur {formatBytes(totalSize)} • {volumes.length} disque(s)</div>
          </div>
        </div>
      </section>

      {system && !system.elevated && (
        <Card className="border-primary/30 bg-primary/5 py-4">
          <CardContent className="flex flex-wrap items-center gap-4 px-5">
            <div className="flex size-10 items-center justify-center rounded-full bg-primary/15 text-primary">
              <Zap className="size-5" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="font-medium">Mode Turbo disponible</div>
              <div className="text-sm text-muted-foreground">
                En administrateur, CleanMyWindows lit directement la table des fichiers NTFS (jusqu'à 10× plus rapide) et analyse
                aussi les dossiers protégés, pour un résultat exact.
              </div>
            </div>
            <Button onClick={() => api.restartAsAdmin().catch((e) => toast.error(String(e)))}>
              <Zap /> Relancer en mode Turbo
            </Button>
          </CardContent>
        </Card>
      )}

      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">Disques</h2>
        <div className="flex gap-2">
          {summary && (
            <Button variant="outline" size="sm" onClick={() => setView("overview")}>
              <Gauge /> Résultats de {summary.info.rootPath}
            </Button>
          )}
          <Button variant="ghost" size="sm" onClick={refreshVolumes}>
            <RefreshCw /> Actualiser
          </Button>
        </div>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
        {volumes.length === 0 &&
          Array.from({ length: 3 }).map((_, i) => <Skeleton key={i} className="h-52 rounded-xl" />)}
        {volumes.map((v) => {
          const used = v.total - v.free
          return (
            <Card key={v.root} className="gap-4 transition-shadow hover:shadow-md">
              <CardHeader className="flex flex-row items-start gap-3">
                <DriveIcon v={v} />
                <div className="min-w-0 flex-1">
                  <CardTitle className="truncate">{driveName(v)}</CardTitle>
                  <CardDescription className="mt-1 flex flex-wrap gap-1.5">
                    {v.isSystem && <Badge variant="secondary">Système</Badge>}
                    {v.isSsd !== null && <Badge variant="outline">{v.isSsd ? "SSD" : "HDD"}</Badge>}
                    {v.filesystem && <Badge variant="outline">{v.filesystem}</Badge>}
                  </CardDescription>
                </div>
              </CardHeader>
              <CardContent className="space-y-2">
                <DiskGauge used={used} total={v.total} />
                <div className="flex justify-between text-sm">
                  <span className="text-muted-foreground">
                    <span className="font-medium text-foreground">{formatBytes(v.free)}</span> libres
                  </span>
                  <span className="text-muted-foreground tabular">
                    {formatBytes(used)} / {formatBytes(v.total)} ({formatPercent(used, v.total)})
                  </span>
                </div>
              </CardContent>
              <CardFooter>
                <Button className="w-full" onClick={() => startScan(v.root)}>
                  <ScanSearch /> Analyser {v.letter}:
                </Button>
              </CardFooter>
            </Card>
          )
        })}
        <Card className="justify-center gap-3 border-dashed">
          <CardHeader className="items-center text-center">
            <div className="mx-auto flex size-11 items-center justify-center rounded-xl bg-muted text-muted-foreground">
              <FolderSearch className="size-5" />
            </div>
            <CardTitle className="mt-2">Un dossier précis</CardTitle>
            <CardDescription>Analysez uniquement un dossier (Téléchargements, projets, disque externe…)</CardDescription>
          </CardHeader>
          <CardFooter>
            <Button variant="outline" className="w-full" onClick={pickFolder}>
              <FolderSearch /> Choisir un dossier…
            </Button>
          </CardFooter>
        </Card>
      </div>
    </div>
  )
}
