import { useEffect, useMemo, useState } from "react"
import { Copy, ExternalLink, Play, Wand2, X, Crown } from "lucide-react"

import { CleanDialog } from "@/components/clean-dialog"
import { PageHeader } from "@/components/page-header"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Progress } from "@/components/ui/progress"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Spinner } from "@/components/ui/spinner"
import { api } from "@/lib/api"
import { formatBytes, formatDate, formatNumber } from "@/lib/format"
import { useStore } from "@/lib/store"

const MB = 1024 * 1024
const PHASES = { sizes: "Regroupement par taille", partial: "Comparaison rapide (début et fin des fichiers)", full: "Vérification complète du contenu" }

export function DuplicatesView() {
  const { dup, startDuplicates, cancelDuplicates } = useStore()
  const [minSize, setMinSize] = useState(String(MB))
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [removed, setRemoved] = useState<Set<number>>(new Set())
  const [cleaning, setCleaning] = useState(false)

  useEffect(() => {
    setSelected(new Set())
    setRemoved(new Set())
  }, [dup.result])

  const groups = useMemo(
    () =>
      (dup.result?.groups ?? [])
        .map((g) => ({ ...g, files: g.files.filter((f) => !removed.has(f.id)) }))
        .filter((g) => g.files.length > 1),
    [dup.result, removed],
  )
  const sizeOf = useMemo(() => {
    const m = new Map<number, number>()
    groups.forEach((g) => g.files.forEach((f) => m.set(f.id, g.alloc)))
    return m
  }, [groups])
  const wasted = groups.reduce((a, g) => a + g.alloc * (g.files.length - 1), 0)
  const selectedBytes = [...selected].reduce((a, id) => a + (sizeOf.get(id) ?? 0), 0)

  function autoSelect() {
    // Garde le plus ancien de chaque groupe (généralement l'original).
    setSelected(new Set(groups.flatMap((g) => g.files.slice(1).map((f) => f.id))))
  }

  function toggle(id: number, on: boolean) {
    setSelected((prev) => {
      const next = new Set(prev)
      if (on) next.add(id)
      else next.delete(id)
      return next
    })
  }

  const progress = dup.progress
  const pct = progress && progress.total > 0 ? ((progress.phase === "full" ? progress.bytes : progress.done) / progress.total) * 100 : undefined

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-5 pb-24">
      <PageHeader
        title="Fichiers en double"
        description="Fichiers au contenu strictement identique (comparaison octet par octet via empreinte xxh3)."
        actions={
          dup.status !== "running" && (
            <>
              <Select value={minSize} onValueChange={setMinSize}>
                <SelectTrigger className="w-44">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={String(100 * 1024)}>Plus de 100 Ko</SelectItem>
                  <SelectItem value={String(MB)}>Plus de 1 Mo</SelectItem>
                  <SelectItem value={String(10 * MB)}>Plus de 10 Mo</SelectItem>
                  <SelectItem value={String(100 * MB)}>Plus de 100 Mo</SelectItem>
                </SelectContent>
              </Select>
              <Button onClick={() => startDuplicates(Number(minSize))}>
                <Play /> {dup.result ? "Relancer" : "Rechercher"}
              </Button>
            </>
          )
        }
      />

      {dup.status === "idle" && !dup.result && (
        <Empty className="border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Copy />
            </EmptyMedia>
            <EmptyTitle>Rechercher les doublons</EmptyTitle>
            <EmptyDescription>
              Les fichiers de même taille sont comparés en lisant leur contenu. Les dossiers système et d'applications, les
              sauvegardes d'appareils, les bibliothèques de jeux, les dépendances de projets, les liens durs et les fichiers cloud
              non téléchargés sont ignorés.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button onClick={() => startDuplicates(Number(minSize))}>
              <Play /> Lancer la recherche
            </Button>
          </EmptyContent>
        </Empty>
      )}

      {dup.status === "running" && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Spinner /> {progress ? PHASES[progress.phase] : "Préparation…"}
            </CardTitle>
            <CardDescription className="tabular">
              {progress?.phase === "full"
                ? `${formatBytes(progress.bytes)} lus sur ${formatBytes(progress.total)}`
                : progress
                  ? `${formatNumber(progress.done)} / ${formatNumber(progress.total)} fichiers candidats`
                  : ""}
            </CardDescription>
          </CardHeader>
          <CardContent className="flex items-center gap-4">
            <Progress value={pct} className="flex-1" />
            <Button variant="outline" size="sm" onClick={cancelDuplicates}>
              <X /> Annuler
            </Button>
          </CardContent>
        </Card>
      )}

      {dup.result && dup.status === "done" && (
        <>
          <Card className="gap-2 py-4">
            <CardContent className="flex flex-wrap items-center gap-4 px-5">
              <div className="flex-1">
                <div className="text-2xl font-semibold">{formatBytes(wasted)}</div>
                <div className="text-sm text-muted-foreground">
                  occupés par des copies — {formatNumber(groups.length)} groupe(s) sur {formatNumber(dup.result.scannedFiles)} fichiers candidats
                  {dup.result.totalGroups > dup.result.groups.length && ` (les ${dup.result.groups.length} plus importants affichés)`}
                </div>
              </div>
              <Button variant="outline" onClick={autoSelect} disabled={groups.length === 0}>
                <Wand2 /> Sélection automatique
              </Button>
            </CardContent>
          </Card>

          {groups.length === 0 && (
            <Empty className="border">
              <EmptyHeader>
                <EmptyTitle>Aucun doublon</EmptyTitle>
                <EmptyDescription>Aucun fichier identique n'a été trouvé avec ces critères.</EmptyDescription>
              </EmptyHeader>
            </Empty>
          )}

          {groups.map((g) => (
            <Card key={g.hash} className="gap-0 py-0">
              <div className="flex items-center justify-between gap-3 border-b px-5 py-3">
                <div className="min-w-0">
                  <div className="truncate font-medium">{g.files[0].name}</div>
                  <div className="text-xs text-muted-foreground">
                    {g.files.length} copies de {formatBytes(g.size)} — {formatBytes(g.alloc * (g.files.length - 1))} récupérables
                  </div>
                </div>
                <Badge variant="secondary" className="tabular">×{g.files.length}</Badge>
              </div>
              {g.files.map((f, idx) => (
                <div key={f.id} className="flex items-center gap-3 px-5 py-2 hover:bg-accent/50">
                  <Checkbox checked={selected.has(f.id)} onCheckedChange={(v) => toggle(f.id, !!v)} aria-label={`Sélectionner ${f.path}`} />
                  <div className="min-w-0 flex-1 truncate text-sm" title={f.path}>
                    {f.path}
                  </div>
                  {idx === 0 && (
                    <Badge variant="outline" className="gap-1 text-xs">
                      <Crown className="size-3" /> Plus ancien
                    </Badge>
                  )}
                  <span className="w-28 text-right text-xs text-muted-foreground">{formatDate(f.mtime)}</span>
                  <Button variant="ghost" size="icon" className="size-7" onClick={() => api.reveal(f.path)} aria-label="Afficher dans l'Explorateur">
                    <ExternalLink className="size-3.5" />
                  </Button>
                </div>
              ))}
            </Card>
          ))}
        </>
      )}

      {selected.size > 0 && (
        <div className="sticky bottom-4 z-20 mx-auto flex w-full max-w-3xl items-center justify-between gap-4 rounded-2xl border bg-popover/95 px-5 py-3 shadow-xl backdrop-blur">
          <div>
            <div className="font-semibold">{formatBytes(selectedBytes)} à libérer</div>
            <div className="text-xs text-muted-foreground">{formatNumber(selected.size)} copie(s) sélectionnée(s)</div>
          </div>
          <Button size="lg" onClick={() => setCleaning(true)}>
            Supprimer les copies
          </Button>
        </div>
      )}

      <CleanDialog
        open={cleaning}
        onOpenChange={setCleaning}
        ids={[...selected]}
        totalBytes={selectedBytes}
        title="Supprimer les copies sélectionnées"
        onDone={() => {
          setRemoved((prev) => new Set([...prev, ...selected]))
          setSelected(new Set())
        }}
      />
    </div>
  )
}
