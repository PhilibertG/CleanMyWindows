import { useEffect, useState } from "react"
import { Copy, ExternalLink, FolderTree, SearchX } from "lucide-react"
import { toast } from "sonner"

import { NodeIcon } from "@/components/icons"
import { PageHeader } from "@/components/page-header"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { api, type NodeInfo } from "@/lib/api"
import { formatAge, formatBytes } from "@/lib/format"
import { useStore } from "@/lib/store"
import { copyPath } from "./explorer"

export function SearchView() {
  const { searchQuery, dataVersion, openInExplorer } = useStore()
  const [results, setResults] = useState<NodeInfo[] | null>(null)

  useEffect(() => {
    let alive = true
    setResults(null)
    const t = setTimeout(() => {
      api.search(searchQuery, 500).then((r) => alive && setResults(r)).catch((e) => toast.error(String(e)))
    }, 150)
    return () => {
      alive = false
      clearTimeout(t)
    }
  }, [searchQuery, dataVersion])

  async function openParent(n: NodeInfo) {
    if (n.isDir) return openInExplorer(n.id)
    const listing = await api.listDir(n.id, 0)
    openInExplorer(listing.breadcrumbs.at(-2)?.id ?? 0)
  }

  return (
    <div className="flex h-full min-h-0 w-full flex-col gap-4">
      <PageHeader
        title={`Recherche : « ${searchQuery} »`}
        description={results ? `${results.length} résultat(s), du plus gros au plus petit` : "Recherche…"}
      />
      {results && results.length === 0 ? (
        <Empty>
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <SearchX />
            </EmptyMedia>
            <EmptyTitle>Aucun résultat</EmptyTitle>
            <EmptyDescription>Aucun fichier ni dossier ne contient « {searchQuery} ».</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <Card className="min-h-0 flex-1 gap-0 overflow-hidden py-0">
          <div className="min-h-0 flex-1 overflow-auto">
            <Table>
              <TableHeader className="sticky top-0 z-10 bg-card">
                <TableRow>
                  <TableHead>Nom</TableHead>
                  <TableHead className="w-28 text-right">Sur disque</TableHead>
                  <TableHead className="w-32">Modifié</TableHead>
                  <TableHead className="w-32" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {!results &&
                  Array.from({ length: 8 }).map((_, i) => (
                    <TableRow key={i}>
                      <TableCell colSpan={4}>
                        <Skeleton className="h-8" />
                      </TableCell>
                    </TableRow>
                  ))}
                {results?.map((r) => (
                  <TableRow key={r.id}>
                    <TableCell className="max-w-0">
                      <div className="flex min-w-0 items-center gap-2">
                        <NodeIcon node={r} />
                        <div className="min-w-0">
                          <div className="truncate font-medium">{r.name}</div>
                          <div className="truncate text-xs text-muted-foreground" title={r.path}>
                            {r.path}
                          </div>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell className="text-right font-medium tabular">{formatBytes(r.alloc)}</TableCell>
                    <TableCell className="text-muted-foreground">{formatAge(r.mtime)}</TableCell>
                    <TableCell>
                      <div className="flex justify-end gap-1">
                        {[
                          { icon: FolderTree, label: "Voir dans CleanMyWindows", run: () => openParent(r) },
                          { icon: ExternalLink, label: "Afficher dans l'Explorateur", run: () => api.reveal(r.path!) },
                          { icon: Copy, label: "Copier le chemin", run: () => copyPath(r.path!) },
                        ].map((a) => (
                          <Tooltip key={a.label}>
                            <TooltipTrigger asChild>
                              <Button variant="ghost" size="icon" className="size-8" onClick={a.run} aria-label={a.label}>
                                <a.icon />
                              </Button>
                            </TooltipTrigger>
                            <TooltipContent>{a.label}</TooltipContent>
                          </Tooltip>
                        ))}
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </Card>
      )}
    </div>
  )
}
