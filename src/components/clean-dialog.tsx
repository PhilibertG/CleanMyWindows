import { useEffect, useState } from "react"
import { AlertTriangle, RotateCcw, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { api, type CleanMode, type CleanResult } from "@/lib/api"
import { formatBytes, formatNumber } from "@/lib/format"
import { useStore } from "@/lib/store"
import { cn } from "@/lib/utils"

interface Props {
  open: boolean
  onOpenChange: (open: boolean) => void
  ids: number[]
  totalBytes: number
  /** Mode proposé par défaut : Corbeille pour ce qui est « à vérifier ». */
  defaultMode?: CleanMode
  title?: string
  onDone?: (result: CleanResult) => void
}

export function CleanDialog({ open, onOpenChange, ids, totalBytes, defaultMode = "trash", title, onDone }: Props) {
  const { afterClean } = useStore()
  const [mode, setMode] = useState<CleanMode>(defaultMode)
  const [running, setRunning] = useState(false)

  useEffect(() => {
    if (open) setMode(defaultMode)
  }, [open, defaultMode])

  async function run() {
    setRunning(true)
    try {
      const result = await api.clean(ids, mode)
      if (result.removed > 0) {
        toast.success(`${formatBytes(result.freed)} libérés`, {
          description:
            `${formatNumber(result.removed)} élément(s) ${mode === "trash" ? "envoyé(s) à la Corbeille" : "supprimé(s)"}` +
            (result.failedCount ? ` • ${formatNumber(result.failedCount)} ignoré(s) (en cours d'utilisation ou protégés)` : ""),
        })
      } else {
        toast.warning("Aucun élément n'a pu être supprimé", {
          description: result.failed[0] ? `${result.failed[0].path} : ${result.failed[0].error}` : undefined,
        })
      }
      await afterClean()
      onDone?.(result)
      onOpenChange(false)
    } catch (e) {
      toast.error(`Échec du nettoyage : ${e}`)
    } finally {
      setRunning(false)
    }
  }

  const options: { value: CleanMode; icon: typeof Trash2; title: string; text: string }[] = [
    { value: "trash", icon: RotateCcw, title: "Envoyer à la Corbeille", text: "Récupérable. L'espace n'est libéré qu'une fois la Corbeille vidée." },
    { value: "permanent", icon: Trash2, title: "Supprimer définitivement", text: "Libère l'espace immédiatement. Irréversible." },
  ]

  return (
    <AlertDialog open={open} onOpenChange={(o) => !running && onOpenChange(o)}>
      <AlertDialogContent className="sm:max-w-lg">
        <AlertDialogHeader>
          <AlertDialogTitle>{title ?? "Nettoyer la sélection"}</AlertDialogTitle>
          <AlertDialogDescription>
            {formatNumber(ids.length)} élément(s) sélectionné(s) — <span className="font-medium text-foreground">{formatBytes(totalBytes)}</span>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <div className="grid gap-2">
          {options.map((o) => (
            <button
              key={o.value}
              type="button"
              onClick={() => setMode(o.value)}
              className={cn(
                "flex items-start gap-3 rounded-lg border p-3 text-left transition-colors",
                mode === o.value ? "border-primary bg-primary/5 ring-1 ring-primary" : "hover:bg-accent",
              )}
            >
              <o.icon className={cn("mt-0.5 size-4", o.value === "permanent" ? "text-destructive" : "text-primary")} />
              <div>
                <div className="text-sm font-medium">{o.title}</div>
                <div className="text-xs text-muted-foreground">{o.text}</div>
              </div>
            </button>
          ))}
        </div>
        {mode === "permanent" && (
          <div className="flex items-start gap-2 rounded-lg bg-destructive/10 p-3 text-xs text-destructive">
            <AlertTriangle className="mt-0.5 size-4 shrink-0" />
            Les fichiers supprimés définitivement ne pourront pas être restaurés.
          </div>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={running}>Annuler</AlertDialogCancel>
          <Button variant={mode === "permanent" ? "destructive" : "default"} onClick={run} disabled={running || ids.length === 0}>
            {running ? <Spinner /> : <Trash2 />}
            {running ? "Nettoyage…" : mode === "trash" ? "Envoyer à la Corbeille" : "Supprimer"}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
