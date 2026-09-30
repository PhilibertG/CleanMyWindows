import { useState } from "react"

import { CATEGORY_ICONS } from "@/components/icons"
import { PageHeader } from "@/components/page-header"
import { SizeBar } from "@/components/size-bar"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { api, type Category } from "@/lib/api"
import { CATEGORY_LABELS, categoryVar, formatBytes, formatNumber, formatPercent } from "@/lib/format"
import { useAsync, useStore } from "@/lib/store"

export function TypesView() {
  const { dataVersion, summary } = useStore()
  const stats = useAsync(() => api.typeStats(), [dataVersion])
  const [filter, setFilter] = useState("")
  const [category, setCategory] = useState<Category | null>(null)
  const total = summary?.root.alloc ?? 1

  const exts = (stats.data?.extensions ?? []).filter(
    (e) => (!category || e.category === category) && (!filter || e.ext.includes(filter.toLowerCase().replace(/^\./, ""))),
  )
  const maxExt = exts[0]?.alloc ?? 1

  return (
    <div className="flex h-full min-h-0 w-full flex-col gap-4">
      <PageHeader title="Types de fichiers" description="Espace occupé par catégorie et par extension" />

      <div className="grid grid-cols-2 gap-3 md:grid-cols-5">
        {stats.loading && !stats.data && Array.from({ length: 10 }).map((_, i) => <Skeleton key={i} className="h-24 rounded-xl" />)}
        {stats.data?.categories.map((c) => {
          const Icon = CATEGORY_ICONS[c.category]
          const active = category === c.category
          return (
            <button key={c.category} onClick={() => setCategory(active ? null : c.category)} className="text-left">
              <Card className={`gap-2 py-3 transition-colors hover:bg-accent ${active ? "ring-2 ring-primary" : ""}`}>
                <CardContent className="space-y-2 px-4">
                  <div className="flex items-center gap-2 text-sm">
                    <Icon className="size-4" style={{ color: categoryVar(c.category) }} />
                    <span className="truncate">{CATEGORY_LABELS[c.category]}</span>
                  </div>
                  <div className="text-lg font-semibold">{formatBytes(c.alloc)}</div>
                  <SizeBar value={c.alloc} max={total} color={categoryVar(c.category)} />
                  <div className="text-xs text-muted-foreground tabular">
                    {formatNumber(c.count)} fichiers • {formatPercent(c.alloc, total)}
                  </div>
                </CardContent>
              </Card>
            </button>
          )
        })}
      </div>

      <Card className="min-h-0 flex-1 gap-0 overflow-hidden pb-0">
        <CardHeader className="flex flex-row items-center justify-between gap-3 pb-4">
          <CardTitle className="flex items-center gap-2">
            Extensions
            {category && (
              <Badge variant="secondary" className="cursor-pointer" onClick={() => setCategory(null)}>
                {CATEGORY_LABELS[category]} ✕
              </Badge>
            )}
          </CardTitle>
          <Input value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Filtrer (ex. mp4)" className="w-56" />
        </CardHeader>
        <div className="min-h-0 flex-1 overflow-auto border-t">
          <Table>
            <TableHeader className="sticky top-0 z-10 bg-card">
              <TableRow>
                <TableHead>Extension</TableHead>
                <TableHead className="w-48">Catégorie</TableHead>
                <TableHead className="w-28 text-right">Fichiers</TableHead>
                <TableHead className="w-28 text-right">Sur disque</TableHead>
                <TableHead className="w-56">Part</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {exts.map((e) => (
                <TableRow key={e.ext}>
                  <TableCell className="font-mono text-sm">{e.ext.startsWith("(") ? e.ext : `.${e.ext}`}</TableCell>
                  <TableCell>
                    <span className="flex items-center gap-2 text-sm">
                      <span className="size-2.5 rounded-sm" style={{ background: categoryVar(e.category) }} />
                      {CATEGORY_LABELS[e.category]}
                    </span>
                  </TableCell>
                  <TableCell className="text-right text-muted-foreground tabular">{formatNumber(e.count)}</TableCell>
                  <TableCell className="text-right font-medium tabular">{formatBytes(e.alloc)}</TableCell>
                  <TableCell>
                    <div className="flex items-center gap-2">
                      <SizeBar value={e.alloc} max={maxExt} color={categoryVar(e.category)} />
                      <span className="w-14 shrink-0 text-right text-xs text-muted-foreground tabular">{formatPercent(e.alloc, total)}</span>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      </Card>
    </div>
  )
}
