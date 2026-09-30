import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import { toast } from "sonner"

import {
  api,
  on,
  type DupProgress,
  type DupResult,
  type ScanMode,
  type ScanProgress,
  type Summary,
  type SystemInfo,
  type Volume,
} from "./api"
import { formatBytes, formatDuration, formatNumber } from "./format"

export type View = "home" | "overview" | "explorer" | "largest" | "types" | "suggestions" | "duplicates" | "search"
export type ScanStatus = "idle" | "scanning" | "done" | "error"

interface Store {
  system: SystemInfo | null
  volumes: Volume[]
  refreshVolumes: () => Promise<void>
  status: ScanStatus
  target: string
  progress: ScanProgress | null
  summary: Summary | null
  error: string | null
  startScan: (path: string, mode?: ScanMode) => Promise<void>
  cancelScan: () => Promise<void>
  view: View
  setView: (v: View) => void
  explorerId: number
  openInExplorer: (id: number) => void
  searchQuery: string
  setSearchQuery: (q: string) => void
  /** Incrémenté après chaque nettoyage : les vues rechargent leurs données. */
  dataVersion: number
  afterClean: () => Promise<void>
  recoverable: number
  setRecoverable: (n: number) => void
  dup: { status: "idle" | "running" | "done" | "error"; progress: DupProgress | null; result: DupResult | null }
  startDuplicates: (minSize: number) => Promise<void>
  cancelDuplicates: () => Promise<void>
}

const Ctx = createContext<Store | null>(null)

export function StoreProvider({ children }: { children: ReactNode }) {
  const [system, setSystem] = useState<SystemInfo | null>(null)
  const [volumes, setVolumes] = useState<Volume[]>([])
  const [status, setStatus] = useState<ScanStatus>("idle")
  const [target, setTarget] = useState("")
  const [progress, setProgress] = useState<ScanProgress | null>(null)
  const [summary, setSummary] = useState<Summary | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [view, setView] = useState<View>("home")
  const [explorerId, setExplorerId] = useState(0)
  const [searchQuery, setSearchQuery] = useState("")
  const [dataVersion, setDataVersion] = useState(0)
  const [recoverable, setRecoverable] = useState(0)
  const [dup, setDup] = useState<Store["dup"]>({ status: "idle", progress: null, result: null })
  const statusRef = useRef(status)
  statusRef.current = status

  const refreshVolumes = useCallback(async () => {
    try {
      setVolumes(await api.listVolumes())
    } catch (e) {
      toast.error(`Impossible de lister les disques : ${e}`)
    }
  }, [])

  useEffect(() => {
    api.systemInfo().then(setSystem).catch(() => {})
    refreshVolumes()
    const subs = [
      on<ScanProgress>("scan:progress", (p) => {
        if (statusRef.current === "scanning") setProgress(p)
      }),
      on<Summary>("scan:done", (s) => {
        setSummary(s)
        setStatus("done")
        setExplorerId(0)
        setRecoverable(0)
        setDup({ status: "idle", progress: null, result: null })
        setDataVersion((v) => v + 1)
        setView("overview")
        toast.success("Analyse terminée", {
          description: `${formatNumber(s.root.files)} fichiers • ${formatBytes(s.root.alloc)} en ${formatDuration(s.info.elapsedMs)}`,
        })
      }),
      on<string>("scan:error", (e) => {
        setError(e)
        setStatus((prev) => (prev === "scanning" ? "error" : prev))
        if (!e.includes("annulée")) toast.error(e)
      }),
      on<DupProgress>("dup:progress", (p) => setDup((d) => (d.status === "running" ? { ...d, progress: p } : d))),
      on<DupResult>("dup:done", (r) => setDup({ status: "done", progress: null, result: r })),
      on<string>("dup:error", (e) => {
        setDup({ status: "error", progress: null, result: null })
        if (!e.includes("annulée")) toast.error(e)
      }),
    ]
    return () => {
      subs.forEach((p) => p.then((un) => un()))
    }
  }, [refreshVolumes])

  const startScan = useCallback(async (path: string, mode: ScanMode = "auto") => {
    setTarget(path)
    setProgress(null)
    setError(null)
    setStatus("scanning")
    try {
      await api.startScan(path, mode)
    } catch (e) {
      setStatus("error")
      setError(String(e))
      toast.error(String(e))
    }
  }, [])

  const cancelScan = useCallback(async () => {
    await api.cancelScan()
    setStatus(summary ? "done" : "idle")
    setView(summary ? "overview" : "home")
  }, [summary])

  const openInExplorer = useCallback((id: number) => {
    setExplorerId(id)
    setView("explorer")
  }, [])

  const afterClean = useCallback(async () => {
    try {
      setSummary(await api.summary())
    } catch {
      /* pas d'analyse */
    }
    setDataVersion((v) => v + 1)
    refreshVolumes()
  }, [refreshVolumes])

  const startDuplicates = useCallback(async (minSize: number) => {
    setDup({ status: "running", progress: null, result: null })
    try {
      await api.findDuplicates(minSize)
    } catch (e) {
      setDup({ status: "error", progress: null, result: null })
      toast.error(String(e))
    }
  }, [])

  const cancelDuplicates = useCallback(async () => {
    await api.cancelDuplicates()
    setDup({ status: "idle", progress: null, result: null })
  }, [])

  const value = useMemo<Store>(
    () => ({
      system,
      volumes,
      refreshVolumes,
      status,
      target,
      progress,
      summary,
      error,
      startScan,
      cancelScan,
      view,
      setView,
      explorerId,
      openInExplorer,
      searchQuery,
      setSearchQuery,
      dataVersion,
      afterClean,
      recoverable,
      setRecoverable,
      dup,
      startDuplicates,
      cancelDuplicates,
    }),
    [system, volumes, refreshVolumes, status, target, progress, summary, error, startScan, cancelScan, view, explorerId, openInExplorer, searchQuery, dataVersion, afterClean, recoverable, dup, startDuplicates, cancelDuplicates],
  )
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}

export function useStore(): Store {
  const s = useContext(Ctx)
  if (!s) throw new Error("useStore hors du StoreProvider")
  return s
}

/** Charge des données asynchrones et les recharge quand `deps` change. */
export function useAsync<T>(fn: () => Promise<T>, deps: unknown[]): { data: T | null; loading: boolean; error: string | null; reload: () => void } {
  const [data, setData] = useState<T | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [tick, setTick] = useState(0)
  useEffect(() => {
    let alive = true
    setLoading(true)
    fn()
      .then((d) => alive && (setData(d), setError(null)))
      .catch((e) => alive && setError(String(e)))
      .finally(() => alive && setLoading(false))
    return () => {
      alive = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, tick])
  return { data, loading, error, reload: () => setTick((t) => t + 1) }
}
