import { useEffect, useMemo, useRef, useState } from "react"
import { hierarchy, treemap, treemapSquarify, type HierarchyRectangularNode } from "d3-hierarchy"

import type { MapNode } from "@/lib/api"
import { CATEGORY_LABELS, formatBytes, formatPercent } from "@/lib/format"

type Rect = HierarchyRectangularNode<MapNode>

interface Props {
  data: MapNode
  /** Ouvre un dossier (clic). */
  onOpen: (id: number) => void
  /** Sélectionne un fichier (clic). */
  onSelectFile?: (id: number) => void
  className?: string
}

const SERIES = 8

function readVar(el: HTMLElement, name: string): string {
  return getComputedStyle(el).getPropertyValue(name).trim() || "#888"
}

function textColorFor(hex: string): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex)
  if (!m) return "#fff"
  const n = parseInt(m[1], 16)
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => {
    const s = c / 255
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
  })
  const lum = 0.2126 * r + 0.7152 * g + 0.0722 * b
  return lum > 0.35 ? "#111111" : "#ffffff"
}

/** Ellipse un texte pour qu'il tienne dans `max` pixels. */
function fit(ctx: CanvasRenderingContext2D, text: string, max: number): string {
  if (ctx.measureText(text).width <= max) return text
  let lo = 0
  let hi = text.length
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1
    if (ctx.measureText(text.slice(0, mid) + "…").width <= max) lo = mid
    else hi = mid - 1
  }
  return lo > 0 ? text.slice(0, lo) + "…" : ""
}

export function Treemap({ data, onOpen, onSelectFile, className }: Props) {
  const wrapRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [size, setSize] = useState({ w: 0, h: 0 })
  const [hover, setHover] = useState<{ node: Rect; x: number; y: number } | null>(null)
  const [themeTick, setThemeTick] = useState(0)

  useEffect(() => {
    const el = wrapRef.current
    if (!el) return
    const ro = new ResizeObserver(([e]) => setSize({ w: Math.floor(e.contentRect.width), h: Math.floor(e.contentRect.height) }))
    ro.observe(el)
    const mo = new MutationObserver(() => setThemeTick((t) => t + 1))
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] })
    return () => {
      ro.disconnect()
      mo.disconnect()
    }
  }, [])

  // Chaque branche de premier niveau garde sa couleur (la couleur suit l'entité).
  const branchIndex = useMemo(() => {
    const m = new Map<number, number>()
    ;(data.children ?? []).forEach((c, i) => m.set(c.id === 4294967295 ? -1 - i : c.id, i))
    return m
  }, [data])

  const layout = useMemo(() => {
    if (size.w < 10 || size.h < 10) return null
    const root = hierarchy<MapNode>(data, (d) => d.children)
      .sum((d) => (d.children && d.children.length ? 0 : Math.max(d.value, 1)))
      .sort((a, b) => (b.value ?? 0) - (a.value ?? 0))
    return treemap<MapNode>()
      .size([size.w, size.h])
      .tile(treemapSquarify.ratio(1.3))
      .paddingInner(2)
      .paddingOuter((d) => (d.depth === 0 ? 0 : 3))
      .paddingTop((d) => {
        if (d.depth === 0) return 0
        const w = (d.x1 ?? 0) - (d.x0 ?? 0)
        return d.depth <= 2 && w > 70 ? 18 : 3
      })
      .round(true)(root)
  }, [data, size])

  const branchOf = (n: Rect): number => {
    let cur: Rect = n
    while (cur.depth > 1 && cur.parent) cur = cur.parent
    if (cur.depth === 0) return 0
    const key = cur.data.id === 4294967295 ? -1 - (cur.parent?.children?.indexOf(cur) ?? 0) : cur.data.id
    return branchIndex.get(key) ?? SERIES
  }

  useEffect(() => {
    const canvas = canvasRef.current
    const wrap = wrapRef.current
    if (!canvas || !wrap || !layout) return
    const dpr = window.devicePixelRatio || 1
    canvas.width = size.w * dpr
    canvas.height = size.h * dpr
    canvas.style.width = `${size.w}px`
    canvas.style.height = `${size.h}px`
    const ctx = canvas.getContext("2d")!
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, size.w, size.h)

    const palette = Array.from({ length: SERIES }, (_, i) => readVar(wrap, `--series-${i + 1}`))
    const neutral = readVar(wrap, "--series-neutral")
    const surface = readVar(wrap, "--card")
    const fg = readVar(wrap, "--foreground")
    const colorOf = (n: Rect) => (n.data.category === "grouped" ? neutral : (palette[branchOf(n)] ?? neutral))

    ctx.font = "600 11px 'Segoe UI Variable Text', 'Segoe UI', sans-serif"
    ctx.textBaseline = "middle"
    for (const n of layout.descendants()) {
      if (n.depth === 0) continue
      const w = n.x1 - n.x0
      const h = n.y1 - n.y0
      if (w < 1 || h < 1) continue
      const color = colorOf(n)
      const isParent = !!n.children?.length
      ctx.beginPath()
      ctx.roundRect(n.x0, n.y0, w, h, Math.min(4, w / 4, h / 4))
      if (isParent) {
        // Cadre du dossier : teinte légère + en-tête.
        ctx.fillStyle = surface
        ctx.fill()
        ctx.globalAlpha = 0.16 + Math.max(0, 0.1 - n.depth * 0.03)
        ctx.fillStyle = color
        ctx.fill()
        ctx.globalAlpha = 1
        if (w > 70) {
          ctx.fillStyle = fg
          const label = fit(ctx, `${n.data.name}  ${formatBytes(n.data.value)}`, w - 10)
          ctx.fillText(label, n.x0 + 6, n.y0 + 9.5)
        }
      } else {
        ctx.globalAlpha = n.data.category === "grouped" ? 0.55 : n.data.isDir ? 0.95 : 0.8
        ctx.fillStyle = color
        ctx.fill()
        ctx.globalAlpha = 1
        if (w > 54 && h > 30) {
          ctx.fillStyle = textColorFor(color)
          ctx.fillText(fit(ctx, n.data.name, w - 10), n.x0 + 6, n.y0 + 12)
          ctx.globalAlpha = 0.8
          ctx.fillText(fit(ctx, formatBytes(n.data.value), w - 10), n.x0 + 6, n.y0 + 26)
          ctx.globalAlpha = 1
        }
      }
    }
    if (hover) {
      const n = hover.node
      ctx.lineWidth = 2
      ctx.strokeStyle = fg
      ctx.beginPath()
      ctx.roundRect(n.x0 + 1, n.y0 + 1, n.x1 - n.x0 - 2, n.y1 - n.y0 - 2, 4)
      ctx.stroke()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [layout, size, hover, themeTick])

  function hitTest(x: number, y: number): Rect | null {
    if (!layout) return null
    let best: Rect | null = null
    for (const n of layout.descendants()) {
      if (n.depth === 0) continue
      if (x >= n.x0 && x < n.x1 && y >= n.y0 && y < n.y1) {
        if (!best || n.depth > best.depth) best = n
      }
    }
    return best
  }

  const rootValue = data.value || 1

  return (
    <div ref={wrapRef} className={className} style={{ position: "relative" }}>
      <canvas
        ref={canvasRef}
        className="absolute inset-0 cursor-pointer"
        role="img"
        aria-label={`Carte des tailles de ${data.name}`}
        onMouseMove={(e) => {
          const r = e.currentTarget.getBoundingClientRect()
          const n = hitTest(e.clientX - r.left, e.clientY - r.top)
          setHover(n ? { node: n, x: e.clientX - r.left, y: e.clientY - r.top } : null)
        }}
        onMouseLeave={() => setHover(null)}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect()
          const n = hitTest(e.clientX - r.left, e.clientY - r.top)
          if (!n || n.data.category === "grouped") return
          if (n.data.isDir) onOpen(n.data.id)
          else if (n.parent && n.parent.depth > 0) onOpen(n.parent.data.id)
          else onSelectFile?.(n.data.id)
        }}
      />
      {hover && (
        <div
          className="pointer-events-none absolute z-10 max-w-72 rounded-lg border bg-popover px-3 py-2 text-xs shadow-lg"
          style={{
            left: Math.min(hover.x + 14, size.w - 290),
            top: hover.y + 16 > size.h - 70 ? hover.y - 70 : hover.y + 16,
          }}
        >
          <div className="truncate font-medium">{hover.node.data.name}</div>
          <div className="mt-0.5 text-muted-foreground tabular">
            {formatBytes(hover.node.data.value)} • {formatPercent(hover.node.data.value, rootValue)}
          </div>
          <div className="text-muted-foreground">
            {hover.node.data.isDir ? "Dossier — cliquer pour ouvrir" : CATEGORY_LABELS[hover.node.data.category]}
          </div>
        </div>
      )}
    </div>
  )
}
