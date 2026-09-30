import { useEffect } from "react"
import { ArrowRight, Clock, Files, Folder, HardDrive, RefreshCw, ShieldAlert, Sparkles, Zap } from "lucide-react"

import { NodeIcon } from "@/components/icons"
import { PageHeader } from "@/components/page-header"
import { SizeBar } from "@/components/size-bar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { api } from "@/lib/api"
import { CATEGORY_LABELS, categoryVar, formatBytes, formatDuration, formatNumber, formatPercent } from "@/lib/format"
import { useAsync, useStore } from "@/lib/store"

const METHOD: Record<string, { label: string; turbo: boolean }> = {
  mft: { label: "Turbo — lecture MFT", turbo: true },
  layout: { label: "Turbo — table des fichiers", turbo: true },
  parallel: { label: "Parcours parallèle", turbo: false },
}

function Stat({ icon: Icon, label, value, hint }: { icon: typeof Files; label: string; value: string; hint?: string }) {
  return (
    <Card className="gap-1 py-4">
      <CardContent className="px-5">
        <div className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          <Icon className="size-3.5" /> {label}
        </div>
        <div className="mt-1.5 text-2xl font-semibold">{value}</div>
        {hint && <div className="text-xs text-muted-foreground">{hint}</div>}
      </CardContent>
    </Card>
  )
}

export function OverviewView() {
  const { summary, dataVersion, openInExplorer, setView, startScan, recoverable, setRecoverable } = useStore()
  const top = useAsync(() => api.listDir(0, 8), [dataVersion])
  const types = useAsync(() => api.typeStats(), [dataVersion])
  const ages = useAsync(() => api.ageStats(), [dataVersion])
  const sugg = useAsync(() => api.suggestions(), [dataVersion])

  useEffect(() => {
    if (sugg.data) setRecoverable(sugg.data.filter((s) => s.safety !== "info").reduce((a, s) => a + s.total, 0))
  }, [sugg.data, setRecoverable])

  if (!summary) return null
  const { info, root } = summary
  const method = METHOD[info.method] ?? METHOD.parallel
  const used = info.volumeTotal - info.volumeFree
  const unscanned = info.isVolumeRoot ? Math.max(used - root.alloc, 0) : 0
  const maxCat = types.data?.categories[0]?.alloc ?? 1
  const maxAge = Math.max(...(ages.data?.map((a) => a.alloc) ?? [1]))
  const safeTotal = sugg.data?.filter((s) => s.safety === "safe").reduce((a, s) => a + s.total, 0) ?? 0

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6">
      <PageHeader
        title={info.rootPath}
        description={
          <span className="flex flex-wrap items-center gap-2">
            <Badge variant={method.turbo ? "default" : "secondary"} className="gap-1">
              {method.turbo && <Zap className="size-3" />}
              {method.label}
            </Badge>
            <span>
              Analysé en {formatDuration(info.elapsedMs)}
              {info.threads > 0 && ` • ${info.threads} threads`}
            </span>
          </span>
        }
        actions={
          <Button variant="outline" onClick={() => startScan(info.rootPath)}>
            <RefreshCw /> Relancer l'analyse
          </Button>
        }
      />

      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        <Stat icon={HardDrive} label="Espace occupé analysé" value={formatBytes(root.alloc)} hint={`Taille des données : ${formatBytes(root.size)}`} />
        <Stat icon={Files} label="Fichiers" value={formatNumber(root.files)} />
        <Stat icon={Folder} label="Dossiers" value={formatNumber(root.dirs)} hint={info.denied ? `${formatNumber(info.denied)} inaccessible(s)` : undefined} />
        <Stat icon={Clock} label="Vitesse" value={`${formatNumber(Math.round((root.files * 1000) / Math.max(info.elapsedMs, 1)))} /s`} hint="fichiers analysés par seconde" />
      </div>

      <Card className="hero-gradient border-0 text-white">
        <CardContent className="flex flex-wrap items-center gap-5 px-6">
          <div className="flex size-12 items-center justify-center rounded-2xl bg-white/15">
            <Sparkles className="size-6" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="text-sm text-white/80">Espace récupérable identifié</div>
            {sugg.loading ? (
              <Skeleton className="mt-1 h-8 w-40 bg-white/20" />
            ) : (
              <div className="text-3xl font-semibold">{formatBytes(recoverable)}</div>
            )}
            <div className="text-sm text-white/80">dont {formatBytes(safeTotal)} sans aucun risque (caches, temporaires…)</div>
          </div>
          <Button size="lg" variant="secondary" onClick={() => setView("suggestions")}>
            Voir les propositions <ArrowRight />
          </Button>
        </CardContent>
      </Card>

      {info.isVolumeRoot && info.volumeTotal > 0 && (
        <Card>
          <CardHeader>
            <CardTitle>Occupation du disque</CardTitle>
            <CardDescription>
              {formatBytes(used)} utilisés sur {formatBytes(info.volumeTotal)} — {formatBytes(info.volumeFree)} libres
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex h-4 w-full gap-0.5 overflow-hidden rounded-full bg-muted">
              <div className="hero-gradient h-full rounded-l-full" style={{ width: `${(root.alloc / info.volumeTotal) * 100}%` }} />
              {unscanned > 0 && <div className="h-full bg-muted-foreground/40" style={{ width: `${(unscanned / info.volumeTotal) * 100}%` }} />}
            </div>
            <div className="flex flex-wrap gap-x-6 gap-y-1 text-sm">
              <span className="flex items-center gap-2">
                <span className="hero-gradient size-2.5 rounded-full" /> Analysé : {formatBytes(root.alloc)}
              </span>
              {unscanned > 0 && (
                <Tooltip>
                  <TooltipTrigger className="flex items-center gap-2">
                    <span className="size-2.5 rounded-full bg-muted-foreground/40" /> Non visible : {formatBytes(unscanned)}
                    <ShieldAlert className="size-3.5 text-muted-foreground" />
                  </TooltipTrigger>
                  <TooltipContent className="max-w-72">
                    Métadonnées NTFS, points de restauration, dossiers protégés et fichiers d'autres utilisateurs.
                    {!info.elevated && " Le mode Turbo (administrateur) permet d'en analyser la plus grande partie."}
                  </TooltipContent>
                </Tooltip>
              )}
              <span className="flex items-center gap-2">
                <span className="size-2.5 rounded-full border bg-muted" /> Libre : {formatBytes(info.volumeFree)}
              </span>
            </div>
          </CardContent>
        </Card>
      )}

      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Dossiers les plus volumineux</CardTitle>
            <CardAction>
              <Button variant="ghost" size="sm" onClick={() => openInExplorer(0)}>
                Explorer <ArrowRight />
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent className="space-y-1">
            {top.loading && !top.data && Array.from({ length: 6 }).map((_, i) => <Skeleton key={i} className="h-10" />)}
            {top.data?.children.map((c) => (
              <button
                key={c.id}
                onClick={() => (c.isDir ? openInExplorer(c.id) : openInExplorer(0))}
                className="grid w-full grid-cols-[1fr_auto] items-center gap-x-3 gap-y-1 rounded-md px-2 py-1.5 text-left hover:bg-accent"
              >
                <span className="flex min-w-0 items-center gap-2 text-sm">
                  <NodeIcon node={c} />
                  <span className="truncate">{c.name}</span>
                </span>
                <span className="text-sm tabular text-muted-foreground">{formatBytes(c.alloc)}</span>
                <SizeBar value={c.alloc} max={top.data!.children[0].alloc} className="col-span-2" />
              </button>
            ))}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Répartition par type</CardTitle>
            <CardAction>
              <Button variant="ghost" size="sm" onClick={() => setView("types")}>
                Détails <ArrowRight />
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent className="space-y-2.5">
            {types.loading && !types.data && Array.from({ length: 6 }).map((_, i) => <Skeleton key={i} className="h-8" />)}
            {types.data?.categories.map((c) => (
              <div key={c.category} className="space-y-1">
                <div className="flex items-center justify-between text-sm">
                  <span className="flex items-center gap-2">
                    <span className="size-2.5 rounded-sm" style={{ background: categoryVar(c.category) }} />
                    {CATEGORY_LABELS[c.category]}
                  </span>
                  <span className="tabular text-muted-foreground">
                    {formatBytes(c.alloc)} <span className="text-xs">• {formatPercent(c.alloc, root.alloc)}</span>
                  </span>
                </div>
                <SizeBar value={c.alloc} max={maxCat} color={categoryVar(c.category)} />
              </div>
            ))}
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Ancienneté des fichiers</CardTitle>
          <CardDescription>Espace occupé selon la date de dernière modification</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-5 items-end gap-3">
            {ages.data?.map((a) => (
              <div key={a.label} className="flex flex-col items-center gap-2">
                <span className="text-sm font-medium tabular">{formatBytes(a.alloc, 0)}</span>
                <div className="flex h-32 w-full items-end">
                  <div className="w-full rounded-t-md bg-primary/80" style={{ height: `${Math.max((a.alloc / maxAge) * 100, 2)}%` }} />
                </div>
                <span className="text-center text-xs text-muted-foreground">{a.label}</span>
                <span className="text-[11px] text-muted-foreground tabular">{formatNumber(a.count)} fichiers</span>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
