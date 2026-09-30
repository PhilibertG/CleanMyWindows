import { Fragment, useEffect, useMemo, useState } from "react"
import { ArrowUp, Copy, ExternalLink, EyeOff, FolderOpen, Lock, Map as MapIcon, Cloud, Link2, ShieldAlert, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { CleanDialog } from "@/components/clean-dialog"
import { NodeIcon } from "@/components/icons"
import { SizeBar } from "@/components/size-bar"
import { Treemap } from "@/components/treemap"
import { Badge } from "@/components/ui/badge"
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { api, type NodeInfo } from "@/lib/api"
import { formatAge, formatBytes, formatDate, formatNumber, formatPercent } from "@/lib/format"
import { useAsync, useStore } from "@/lib/store"

function joinPath(dir: string, name: string) {
  return dir.endsWith("\\") ? dir + name : `${dir}\\${name}`
}

export function Flags({ node }: { node: NodeInfo }) {
  const flags = [
    node.denied && { icon: Lock, label: "Accès refusé : contenu non analysé" },
    node.link && { icon: Link2, label: "Lien symbolique / jonction (non suivi)" },
    node.cloud && { icon: Cloud, label: "Fichier cloud non téléchargé (n'occupe pas d'espace)" },
    node.system && { icon: ShieldAlert, label: "Élément système" },
    node.hidden && { icon: EyeOff, label: "Élément caché" },
  ].filter(Boolean) as { icon: typeof Lock; label: string }[]
  return (
    <>
      {flags.map((f) => (
        <Tooltip key={f.label}>
          <TooltipTrigger asChild>
            <f.icon className="size-3.5 shrink-0 text-muted-foreground" />
          </TooltipTrigger>
          <TooltipContent>{f.label}</TooltipContent>
        </Tooltip>
      ))}
    </>
  )
}

export function copyPath(path: string) {
  navigator.clipboard.writeText(path).then(() => toast.success("Chemin copié"), () => toast.error("Copie impossible"))
}

export function ExplorerView() {
  const { explorerId, openInExplorer, dataVersion } = useStore()
  const [showMap, setShowMap] = useState(true)
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [cleanIds, setCleanIds] = useState<number[] | null>(null)
  const listing = useAsync(() => api.listDir(explorerId, 1000), [explorerId, dataVersion])
  const map = useAsync(() => (showMap ? api.treemap(explorerId, 3, 0.002) : Promise.resolve(null)), [explorerId, dataVersion, showMap])

  useEffect(() => setSelected(new Set()), [explorerId, dataVersion])

  const data = listing.data
  const byId = useMemo(() => new Map(data?.children.map((c) => [c.id, c]) ?? []), [data])
  const selectedBytes = [...selected].reduce((s, id) => s + (byId.get(id)?.alloc ?? 0), 0)
  const cleanBytes = (cleanIds ?? []).reduce((s, id) => s + (byId.get(id)?.alloc ?? 0), 0)

  function toggle(id: number, on: boolean) {
    setSelected((prev) => {
      const next = new Set(prev)
      if (on) next.add(id)
      else next.delete(id)
      return next
    })
  }

  const parentId = data && data.breadcrumbs.length > 1 ? data.breadcrumbs[data.breadcrumbs.length - 2].id : null
  const allChecked = !!data && data.children.length > 0 && selected.size === data.children.length

  return (
    <div className="flex h-full min-h-0 w-full flex-col gap-4">
      <div className="flex flex-wrap items-center gap-3">
        <Button variant="outline" size="icon" disabled={parentId === null} onClick={() => parentId !== null && openInExplorer(parentId)} aria-label="Dossier parent">
          <ArrowUp />
        </Button>
        <Breadcrumb className="min-w-0 flex-1">
          <BreadcrumbList className="flex-nowrap overflow-hidden">
            {data?.breadcrumbs.map((b, i) => (
              <Fragment key={b.id}>
                {i > 0 && <BreadcrumbSeparator />}
                <BreadcrumbItem className="min-w-0">
                  {i === data.breadcrumbs.length - 1 ? (
                    <BreadcrumbPage className="truncate font-medium">{b.name}</BreadcrumbPage>
                  ) : (
                    <BreadcrumbLink className="max-w-40 cursor-pointer truncate" onClick={() => openInExplorer(b.id)}>
                      {b.name}
                    </BreadcrumbLink>
                  )}
                </BreadcrumbItem>
              </Fragment>
            ))}
          </BreadcrumbList>
        </Breadcrumb>
        {data && (
          <div className="text-sm text-muted-foreground tabular">
            <span className="font-semibold text-foreground">{formatBytes(data.node.alloc)}</span> • {formatNumber(data.node.files)} fichiers
          </div>
        )}
        <label className="flex items-center gap-2 text-sm">
          <MapIcon className="size-4 text-muted-foreground" /> Carte
          <Switch checked={showMap} onCheckedChange={setShowMap} />
        </label>
        {data?.node.path && (
          <Button variant="outline" size="sm" onClick={() => api.open(data.node.path!).catch((e) => toast.error(String(e)))}>
            <FolderOpen /> Ouvrir
          </Button>
        )}
      </div>

      {showMap && (
        <Card className="h-[38vh] min-h-56 shrink-0 overflow-hidden p-1.5">
          {map.data ? (
            map.data.value > 0 ? (
              <Treemap data={map.data} onOpen={openInExplorer} className="h-full w-full" />
            ) : (
              <div className="flex h-full items-center justify-center text-sm text-muted-foreground">Ce dossier est vide</div>
            )
          ) : (
            <Skeleton className="h-full w-full" />
          )}
        </Card>
      )}

      <Card className="min-h-0 flex-1 gap-0 overflow-hidden py-0">
        <div className="min-h-0 flex-1 overflow-auto">
          <Table>
            <TableHeader className="sticky top-0 z-10 bg-card">
              <TableRow>
                <TableHead className="w-10">
                  <Checkbox
                    checked={allChecked}
                    onCheckedChange={(v) => setSelected(v ? new Set(data?.children.map((c) => c.id)) : new Set())}
                    aria-label="Tout sélectionner"
                  />
                </TableHead>
                <TableHead>Nom</TableHead>
                <TableHead className="w-28 text-right">Sur disque</TableHead>
                <TableHead className="w-48">Part du dossier</TableHead>
                <TableHead className="w-24 text-right">Fichiers</TableHead>
                <TableHead className="w-32">Modifié</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {listing.loading && !data &&
                Array.from({ length: 8 }).map((_, i) => (
                  <TableRow key={i}>
                    <TableCell colSpan={6}>
                      <Skeleton className="h-5" />
                    </TableCell>
                  </TableRow>
                ))}
              {data?.children.map((c) => {
                const path = joinPath(data.node.path ?? "", c.name)
                return (
                  <ContextMenu key={c.id}>
                    <ContextMenuTrigger asChild>
                      <TableRow
                        data-state={selected.has(c.id) ? "selected" : undefined}
                        className="cursor-pointer"
                        onClick={() => c.isDir && c.hasChildren && openInExplorer(c.id)}
                        onDoubleClick={() => !c.isDir && api.reveal(path)}
                      >
                        <TableCell onClick={(e) => e.stopPropagation()}>
                          <Checkbox checked={selected.has(c.id)} onCheckedChange={(v) => toggle(c.id, !!v)} aria-label={`Sélectionner ${c.name}`} />
                        </TableCell>
                        <TableCell className="max-w-0">
                          <div className="flex min-w-0 items-center gap-2">
                            <NodeIcon node={c} />
                            <span className="truncate" title={c.name}>
                              {c.name}
                            </span>
                            <Flags node={c} />
                          </div>
                        </TableCell>
                        <TableCell className="text-right font-medium tabular">{formatBytes(c.alloc)}</TableCell>
                        <TableCell>
                          <div className="flex items-center gap-2">
                            <SizeBar value={c.alloc} max={data.node.alloc} />
                            <span className="w-12 shrink-0 text-right text-xs text-muted-foreground tabular">{formatPercent(c.alloc, data.node.alloc)}</span>
                          </div>
                        </TableCell>
                        <TableCell className="text-right text-muted-foreground tabular">{c.isDir ? formatNumber(c.files) : "—"}</TableCell>
                        <TableCell className="text-muted-foreground" title={formatDate(c.mtime)}>
                          {formatAge(c.mtime)}
                        </TableCell>
                      </TableRow>
                    </ContextMenuTrigger>
                    <ContextMenuContent className="w-60">
                      {c.isDir && c.hasChildren && (
                        <ContextMenuItem onClick={() => openInExplorer(c.id)}>
                          <FolderOpen /> Parcourir dans CleanMyWindows
                        </ContextMenuItem>
                      )}
                      <ContextMenuItem onClick={() => api.reveal(path).catch((e) => toast.error(String(e)))}>
                        <ExternalLink /> Afficher dans l'Explorateur
                      </ContextMenuItem>
                      <ContextMenuItem onClick={() => copyPath(path)}>
                        <Copy /> Copier le chemin
                      </ContextMenuItem>
                      <ContextMenuSeparator />
                      <ContextMenuItem variant="destructive" onClick={() => setCleanIds([c.id])}>
                        <Trash2 /> Supprimer…
                      </ContextMenuItem>
                    </ContextMenuContent>
                  </ContextMenu>
                )
              })}
              {data && data.children.length === 0 && (
                <TableRow>
                  <TableCell colSpan={6} className="py-10 text-center text-muted-foreground">
                    {data.node.denied ? "Accès refusé : ce dossier n'a pas pu être analysé." : "Dossier vide"}
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          {data && data.totalChildren > data.children.length && (
            <p className="p-3 text-center text-xs text-muted-foreground">
              {formatNumber(data.totalChildren - data.children.length)} éléments plus petits non affichés
            </p>
          )}
        </div>
        {selected.size > 0 && (
          <div className="flex items-center justify-between gap-3 border-t bg-muted/40 px-4 py-2.5">
            <span className="text-sm">
              <Badge variant="secondary" className="mr-2 tabular">{selected.size}</Badge>
              sélectionné(s) — <span className="font-semibold">{formatBytes(selectedBytes)}</span>
            </span>
            <div className="flex gap-2">
              <Button variant="ghost" size="sm" onClick={() => setSelected(new Set())}>
                Désélectionner
              </Button>
              <Button variant="destructive" size="sm" onClick={() => setCleanIds([...selected])}>
                <Trash2 /> Supprimer la sélection
              </Button>
            </div>
          </div>
        )}
      </Card>

      <CleanDialog
        open={cleanIds !== null}
        onOpenChange={(o) => !o && setCleanIds(null)}
        ids={cleanIds ?? []}
        totalBytes={cleanBytes}
        defaultMode="trash"
        onDone={() => setSelected(new Set())}
      />
    </div>
  )
}
