import { useEffect, useMemo, useState } from "react"
import { Copy, ExternalLink, FolderTree, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { CleanDialog } from "@/components/clean-dialog"
import { NodeIcon } from "@/components/icons"
import { PageHeader } from "@/components/page-header"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger } from "@/components/ui/context-menu"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { api, type Category } from "@/lib/api"
import { CATEGORY_LABELS, CATEGORY_ORDER, categoryVar, formatAge, formatBytes, formatDate } from "@/lib/format"
import { useAsync, useStore } from "@/lib/store"
import { copyPath, Flags } from "./explorer"

function parentPath(path: string) {
  const i = path.lastIndexOf("\\")
  return i > 2 ? path.slice(0, i) : path.slice(0, 3)
}

export function LargestView() {
  const { dataVersion, openInExplorer } = useStore()
  const [category, setCategory] = useState<Category | "all">("all")
  const [limit, setLimit] = useState("300")
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [cleanIds, setCleanIds] = useState<number[] | null>(null)
  const files = useAsync(() => api.topFiles(Number(limit), category === "all" ? undefined : category), [category, limit, dataVersion])

  useEffect(() => setSelected(new Set()), [category, limit, dataVersion])
  const byId = useMemo(() => new Map(files.data?.map((f) => [f.id, f]) ?? []), [files.data])
  const total = files.data?.reduce((s, f) => s + f.alloc, 0) ?? 0
  const sum = (ids: Iterable<number>) => [...ids].reduce((s, id) => s + (byId.get(id)?.alloc ?? 0), 0)

  return (
    <div className="flex h-full min-h-0 w-full flex-col gap-4">
      <PageHeader
        title="Plus gros fichiers"
        description={files.data ? `${files.data.length} fichiers — ${formatBytes(total)} au total` : "Chargement…"}
        actions={
          <>
            <Select value={category} onValueChange={(v) => setCategory(v as Category | "all")}>
              <SelectTrigger className="w-56">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">Tous les types</SelectItem>
                {CATEGORY_ORDER.map((c) => (
                  <SelectItem key={c} value={c}>
                    <span className="size-2.5 rounded-sm" style={{ background: categoryVar(c) }} />
                    {CATEGORY_LABELS[c]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Select value={limit} onValueChange={setLimit}>
              <SelectTrigger className="w-32">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {["100", "300", "1000", "3000"].map((l) => (
                  <SelectItem key={l} value={l}>
                    Top {l}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </>
        }
      />

      <Card className="min-h-0 flex-1 gap-0 overflow-hidden py-0">
        <div className="min-h-0 flex-1 overflow-auto">
          <Table>
            <TableHeader className="sticky top-0 z-10 bg-card">
              <TableRow>
                <TableHead className="w-10">
                  <Checkbox
                    checked={!!files.data?.length && selected.size === files.data.length}
                    onCheckedChange={(v) => setSelected(v ? new Set(files.data?.map((f) => f.id)) : new Set())}
                    aria-label="Tout sélectionner"
                  />
                </TableHead>
                <TableHead>Fichier</TableHead>
                <TableHead className="w-28 text-right">Sur disque</TableHead>
                <TableHead className="w-44">Type</TableHead>
                <TableHead className="w-32">Modifié</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {files.loading && !files.data &&
                Array.from({ length: 10 }).map((_, i) => (
                  <TableRow key={i}>
                    <TableCell colSpan={5}>
                      <Skeleton className="h-8" />
                    </TableCell>
                  </TableRow>
                ))}
              {files.data?.map((f) => (
                <ContextMenu key={f.id}>
                  <ContextMenuTrigger asChild>
                    <TableRow data-state={selected.has(f.id) ? "selected" : undefined} onDoubleClick={() => api.reveal(f.path!)}>
                      <TableCell>
                        <Checkbox
                          checked={selected.has(f.id)}
                          onCheckedChange={(v) =>
                            setSelected((prev) => {
                              const next = new Set(prev)
                              if (v) next.add(f.id)
                              else next.delete(f.id)
                              return next
                            })
                          }
                          aria-label={`Sélectionner ${f.name}`}
                        />
                      </TableCell>
                      <TableCell className="max-w-0">
                        <div className="flex min-w-0 items-center gap-2">
                          <NodeIcon node={f} />
                          <div className="min-w-0">
                            <div className="flex items-center gap-1.5">
                              <span className="truncate font-medium">{f.name}</span>
                              <Flags node={f} />
                            </div>
                            <div className="truncate text-xs text-muted-foreground" title={f.path}>
                              {parentPath(f.path ?? "")}
                            </div>
                          </div>
                        </div>
                      </TableCell>
                      <TableCell className="text-right font-medium tabular">{formatBytes(f.alloc)}</TableCell>
                      <TableCell>
                        <Badge variant="outline" className="gap-1.5 font-normal">
                          <span className="size-2 rounded-full" style={{ background: categoryVar(f.category) }} />
                          {CATEGORY_LABELS[f.category]}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-muted-foreground" title={formatDate(f.mtime)}>
                        {formatAge(f.mtime)}
                      </TableCell>
                    </TableRow>
                  </ContextMenuTrigger>
                  <ContextMenuContent className="w-60">
                    <ContextMenuItem onClick={() => api.reveal(f.path!).catch((e) => toast.error(String(e)))}>
                      <ExternalLink /> Afficher dans l'Explorateur
                    </ContextMenuItem>
                    <ContextMenuItem
                      onClick={async () => {
                        // Le fil d'Ariane du fichier donne l'id de son dossier parent.
                        const listing = await api.listDir(f.id, 0)
                        openInExplorer(listing.breadcrumbs.at(-2)?.id ?? 0)
                      }}
                    >
                      <FolderTree /> Voir le dossier dans CleanMyWindows
                    </ContextMenuItem>
                    <ContextMenuItem onClick={() => copyPath(f.path!)}>
                      <Copy /> Copier le chemin
                    </ContextMenuItem>
                    <ContextMenuSeparator />
                    <ContextMenuItem variant="destructive" onClick={() => setCleanIds([f.id])}>
                      <Trash2 /> Supprimer…
                    </ContextMenuItem>
                  </ContextMenuContent>
                </ContextMenu>
              ))}
            </TableBody>
          </Table>
        </div>
        {selected.size > 0 && (
          <div className="flex items-center justify-between gap-3 border-t bg-muted/40 px-4 py-2.5">
            <span className="text-sm">
              <Badge variant="secondary" className="mr-2 tabular">{selected.size}</Badge>
              sélectionné(s) — <span className="font-semibold">{formatBytes(sum(selected))}</span>
            </span>
            <Button variant="destructive" size="sm" onClick={() => setCleanIds([...selected])}>
              <Trash2 /> Supprimer la sélection
            </Button>
          </div>
        )}
      </Card>

      <CleanDialog
        open={cleanIds !== null}
        onOpenChange={(o) => !o && setCleanIds(null)}
        ids={cleanIds ?? []}
        totalBytes={sum(cleanIds ?? [])}
        onDone={() => setSelected(new Set())}
      />
    </div>
  )
}
