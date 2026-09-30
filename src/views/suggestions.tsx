import { useEffect, useMemo, useState } from "react"
import { AlertTriangle, CheckCircle2, ChevronDown, ExternalLink, Info, Lock, PartyPopper, ShieldCheck, Sparkles, Trash2, Wrench } from "lucide-react"
import { toast } from "sonner"

import { CleanDialog } from "@/components/clean-dialog"
import { SUGGESTION_ICONS } from "@/components/icons"
import { PageHeader } from "@/components/page-header"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Skeleton } from "@/components/ui/skeleton"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { api, type Suggestion } from "@/lib/api"
import { formatAge, formatBytes, formatNumber } from "@/lib/format"
import { useAsync, useStore } from "@/lib/store"
import { cn } from "@/lib/utils"

const SAFETY = {
  safe: { label: "Sans risque", icon: ShieldCheck, className: "bg-success/15 text-success border-success/30" },
  review: { label: "À vérifier", icon: AlertTriangle, className: "bg-warning/15 text-warning border-warning/30" },
  info: { label: "Outil Windows", icon: Info, className: "bg-muted text-muted-foreground" },
} as const

function selectable(s: Suggestion) {
  return s.action === "delete"
}

export function SuggestionsView() {
  const { dataVersion, summary, system, setRecoverable, afterClean } = useStore()
  const sugg = useAsync(() => api.suggestions(), [dataVersion])
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [open, setOpen] = useState<Set<string>>(new Set())
  const [cleaning, setCleaning] = useState(false)
  const [confirmBin, setConfirmBin] = useState(false)

  const list = sugg.data ?? []
  const itemSize = useMemo(() => {
    const m = new Map<number, number>()
    list.forEach((s) => s.items.forEach((i) => m.set(i.id, i.alloc)))
    return m
  }, [list])

  // Présélection des catégories sans risque (comme un nettoyage intelligent).
  useEffect(() => {
    if (!sugg.data) return
    setRecoverable(sugg.data.filter((s) => s.safety !== "info").reduce((a, s) => a + s.total, 0))
    setSelected(
      new Set(
        sugg.data
          .filter((s) => s.safety === "safe" && selectable(s))
          .flatMap((s) => s.items.filter((i) => system?.elevated || !i.admin).map((i) => i.id)),
      ),
    )
  }, [sugg.data, setRecoverable, system?.elevated])

  const selectedBytes = [...selected].reduce((a, id) => a + (itemSize.get(id) ?? 0), 0)
  const safeIds = new Set(list.filter((s) => s.safety === "safe").flatMap((s) => s.items.map((i) => i.id)))
  const onlySafe = [...selected].every((id) => safeIds.has(id))
  const totalSafe = list.filter((s) => s.safety === "safe").reduce((a, s) => a + s.total, 0)
  const totalReview = list.filter((s) => s.safety === "review").reduce((a, s) => a + s.total, 0)

  function setMany(ids: number[], on: boolean) {
    setSelected((prev) => {
      const next = new Set(prev)
      ids.forEach((id) => (on ? next.add(id) : next.delete(id)))
      return next
    })
  }

  function toggleOpen(id: string) {
    setOpen((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  async function emptyBin() {
    try {
      const freed = await api.emptyRecycleBin()
      toast.success("Corbeille vidée", { description: `${formatBytes(freed)} libérés` })
      await afterClean()
    } catch (e) {
      toast.error(String(e))
    }
  }

  const letter = summary?.info.rootPath.slice(0, 1) ?? "C"

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-5 pb-24">
      <PageHeader
        title="Propositions de suppression"
        description="CleanMyWindows a repéré ces éléments inutiles ou régénérables. Vérifiez, cochez, nettoyez."
      />

      <div className="grid gap-3 sm:grid-cols-3">
        <Card className="hero-gradient gap-1 border-0 py-4 text-white">
          <CardContent className="px-5">
            <div className="flex items-center gap-1.5 text-xs text-white/80">
              <Sparkles className="size-3.5" /> Récupérable au total
            </div>
            <div className="mt-1 text-2xl font-semibold">{sugg.data ? formatBytes(totalSafe + totalReview) : "…"}</div>
          </CardContent>
        </Card>
        <Card className="gap-1 py-4">
          <CardContent className="px-5">
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <ShieldCheck className="size-3.5 text-success" /> Sans risque
            </div>
            <div className="mt-1 text-2xl font-semibold">{sugg.data ? formatBytes(totalSafe) : "…"}</div>
          </CardContent>
        </Card>
        <Card className="gap-1 py-4">
          <CardContent className="px-5">
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <AlertTriangle className="size-3.5 text-warning" /> À vérifier
            </div>
            <div className="mt-1 text-2xl font-semibold">{sugg.data ? formatBytes(totalReview) : "…"}</div>
          </CardContent>
        </Card>
      </div>

      {sugg.loading && !sugg.data && Array.from({ length: 5 }).map((_, i) => <Skeleton key={i} className="h-20 rounded-xl" />)}

      {sugg.data && list.length === 0 && (
        <Empty className="border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <PartyPopper />
            </EmptyMedia>
            <EmptyTitle>Rien à nettoyer</EmptyTitle>
            <EmptyDescription>Aucun élément superflu n'a été détecté dans cette analyse.</EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}

      {list.map((s) => {
        const Icon = SUGGESTION_ICONS[s.icon] ?? Sparkles
        const safety = SAFETY[s.safety]
        const locked = (i: { admin: boolean }) => i.admin && !system?.elevated
        const ids = s.items.filter((i) => !locked(i)).map((i) => i.id)
        const selCount = ids.filter((id) => selected.has(id)).length
        const checked = selCount === 0 ? false : selCount === ids.length ? true : "indeterminate"
        const adminCount = s.items.filter(locked).length
        const needsAdmin = adminCount > 0
        const isOpen = open.has(s.id)
        return (
          <Collapsible key={s.id} open={isOpen} onOpenChange={() => toggleOpen(s.id)}>
            <Card className={cn("gap-0 overflow-hidden py-0 transition-shadow", isOpen && "shadow-md")}>
              <div className="flex items-center gap-4 px-5 py-4">
                {selectable(s) ? (
                  <Checkbox checked={checked} onCheckedChange={(v) => setMany(ids, !!v)} aria-label={`Sélectionner ${s.title}`} />
                ) : (
                  <span className="size-4" />
                )}
                <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-primary">
                  <Icon className="size-5" />
                </div>
                <CollapsibleTrigger className="min-w-0 flex-1 text-left">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium">{s.title}</span>
                    <Badge variant="outline" className={cn("gap-1", safety.className)}>
                      <safety.icon className="size-3" />
                      {safety.label}
                    </Badge>
                    {needsAdmin && (
                      <Badge variant="outline" className="gap-1">
                        <Lock className="size-3" />
                        {adminCount === s.items.length ? "Admin requis" : `${adminCount} élément(s) en admin`}
                      </Badge>
                    )}
                  </div>
                  <p className="mt-0.5 line-clamp-2 text-sm text-muted-foreground">{s.description}</p>
                </CollapsibleTrigger>
                <div className="text-right">
                  <div className="text-lg font-semibold tabular">{formatBytes(s.total)}</div>
                  <div className="text-xs text-muted-foreground">
                    {formatNumber(s.count)} élément(s)
                    {selCount > 0 && selCount < ids.length && ` • ${selCount} coché(s)`}
                  </div>
                </div>
                <CollapsibleTrigger asChild>
                  <Button variant="ghost" size="icon" aria-label="Détails">
                    <ChevronDown className={cn("transition-transform", isOpen && "rotate-180")} />
                  </Button>
                </CollapsibleTrigger>
              </div>

              {s.action === "emptyRecycleBin" && (
                <div className="flex items-center justify-between gap-3 border-t bg-muted/30 px-5 py-2.5">
                  <span className="text-sm text-muted-foreground">Vide la Corbeille de ce disque via Windows.</span>
                  <Button size="sm" variant="outline" onClick={() => setConfirmBin(true)}>
                    <Trash2 /> Vider la Corbeille
                  </Button>
                </div>
              )}
              {s.action === "none" && s.advice && (
                <div className="flex items-center gap-2 border-t bg-muted/30 px-5 py-2.5 text-sm text-muted-foreground">
                  <Info className="size-4 shrink-0" /> {s.advice}
                </div>
              )}
              {s.action === "diskCleanup" && (
                <div className="flex items-center justify-between gap-3 border-t bg-muted/30 px-5 py-2.5">
                  <span className="text-sm text-muted-foreground">{s.advice}</span>
                  <Button size="sm" variant="outline" onClick={() => api.diskCleanup(letter).catch((e) => toast.error(String(e)))}>
                    <Wrench /> Nettoyage de disque
                  </Button>
                </div>
              )}

              <CollapsibleContent>
                <div className="border-t">
                  {s.advice && s.action === "delete" && (
                    <div className="flex items-center gap-2 bg-muted/30 px-5 py-2 text-xs text-muted-foreground">
                      <Info className="size-3.5 shrink-0" /> {s.advice}
                      {needsAdmin && " Relancez en mode Turbo (administrateur) pour ces éléments."}
                    </div>
                  )}
                  <div className="max-h-96 overflow-auto">
                    {s.items.map((i) => (
                      <div key={i.id} className="flex items-center gap-3 border-t px-5 py-2 first:border-t-0 hover:bg-accent/50">
                        {selectable(s) && locked(i) ? (
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <Lock className="size-4 shrink-0 text-muted-foreground" />
                            </TooltipTrigger>
                            <TooltipContent>Emplacement système : activez le mode Turbo (administrateur) pour le nettoyer</TooltipContent>
                          </Tooltip>
                        ) : selectable(s) ? (
                          <Checkbox checked={selected.has(i.id)} onCheckedChange={(v) => setMany([i.id], !!v)} aria-label={`Sélectionner ${i.name}`} />
                        ) : (
                          <span className="size-4" />
                        )}
                        <div className="min-w-0 flex-1">
                          <div className="truncate text-sm font-medium">{i.name}</div>
                          <div className="truncate text-xs text-muted-foreground" title={i.path}>
                            {i.note ? <span className="text-foreground/70">{i.note} — </span> : null}
                            {i.path}
                          </div>
                        </div>
                        <span className="hidden w-28 text-right text-xs text-muted-foreground md:block">{formatAge(i.mtime)}</span>
                        <span className="w-20 text-right text-sm font-medium tabular">{formatBytes(i.alloc)}</span>
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <Button variant="ghost" size="icon" className="size-7" onClick={() => api.reveal(i.path)} aria-label="Afficher dans l'Explorateur">
                              <ExternalLink className="size-3.5" />
                            </Button>
                          </TooltipTrigger>
                          <TooltipContent>Afficher dans l'Explorateur</TooltipContent>
                        </Tooltip>
                      </div>
                    ))}
                    {s.truncated && (
                      <div className="border-t px-5 py-2 text-center text-xs text-muted-foreground">
                        + {formatNumber(s.count - s.items.length)} éléments plus petits (relancez le nettoyage pour les traiter)
                      </div>
                    )}
                  </div>
                </div>
              </CollapsibleContent>
            </Card>
          </Collapsible>
        )
      })}

      {selected.size > 0 && (
        <div className="sticky bottom-4 z-20 mx-auto flex w-full max-w-3xl items-center justify-between gap-4 rounded-2xl border bg-popover/95 px-5 py-3 shadow-xl backdrop-blur">
          <div className="flex items-center gap-3">
            <CheckCircle2 className="size-5 text-primary" />
            <div>
              <div className="font-semibold">{formatBytes(selectedBytes)} à libérer</div>
              <div className="text-xs text-muted-foreground">{formatNumber(selected.size)} élément(s) sélectionné(s)</div>
            </div>
          </div>
          <div className="flex gap-2">
            <Button variant="ghost" onClick={() => setSelected(new Set())}>
              Tout décocher
            </Button>
            <Button size="lg" onClick={() => setCleaning(true)}>
              <Sparkles /> Nettoyer
            </Button>
          </div>
        </div>
      )}

      <CleanDialog
        open={cleaning}
        onOpenChange={setCleaning}
        ids={[...selected]}
        totalBytes={selectedBytes}
        defaultMode={onlySafe ? "permanent" : "trash"}
        title="Nettoyer les éléments sélectionnés"
      />

      <AlertDialog open={confirmBin} onOpenChange={setConfirmBin}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Vider la Corbeille ?</AlertDialogTitle>
            <AlertDialogDescription>
              Tous les éléments de la Corbeille du disque {letter}: seront supprimés définitivement.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Annuler</AlertDialogCancel>
            <AlertDialogAction onClick={emptyBin}>Vider la Corbeille</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
