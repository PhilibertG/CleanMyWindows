import { useEffect, useState } from "react"
import { Search } from "lucide-react"

import { AppSidebar } from "@/components/app-sidebar"
import { Input } from "@/components/ui/input"
import { Separator } from "@/components/ui/separator"
import { SidebarInset, SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar"
import { useStore, type View } from "@/lib/store"
import { DuplicatesView } from "@/views/duplicates"
import { ExplorerView } from "@/views/explorer"
import { HomeView } from "@/views/home"
import { LargestView } from "@/views/largest"
import { OverviewView } from "@/views/overview"
import { ScanningView } from "@/views/scanning"
import { SearchView } from "@/views/search"
import { SuggestionsView } from "@/views/suggestions"
import { TypesView } from "@/views/types"

const TITLES: Record<View, string> = {
  home: "Disques",
  overview: "Vue d'ensemble",
  explorer: "Explorateur",
  largest: "Plus gros fichiers",
  types: "Types de fichiers",
  suggestions: "Propositions",
  duplicates: "Doublons",
  search: "Recherche",
}

/** Vues qui gèrent leur propre défilement (tableaux plein écran). */
const FULL_HEIGHT: View[] = ["explorer", "largest", "types", "search"]

function SearchBox() {
  const { summary, status, setSearchQuery, setView, view } = useStore()
  const [value, setValue] = useState("")
  useEffect(() => {
    if (view !== "search") setValue("")
  }, [view])
  if (!summary || status === "scanning") return null
  return (
    <form
      className="relative ml-auto w-full max-w-sm"
      onSubmit={(e) => {
        e.preventDefault()
        if (value.trim().length >= 2) {
          setSearchQuery(value.trim())
          setView("search")
        }
      }}
    >
      <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
      <Input value={value} onChange={(e) => setValue(e.target.value)} placeholder="Rechercher un fichier ou un dossier…" className="h-8 pl-8" />
    </form>
  )
}

export default function App() {
  const { view, status, summary } = useStore()
  const scanning = status === "scanning"
  const needsScan = view !== "home" && !summary
  const current: View = needsScan ? "home" : view
  const full = !scanning && FULL_HEIGHT.includes(current)

  return (
    <SidebarProvider className="h-svh">
      <AppSidebar />
      <SidebarInset className="min-h-0 overflow-hidden">
        <header className="flex h-12 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger className="-ml-1" />
          <Separator orientation="vertical" className="mr-2 data-[orientation=vertical]:h-4" />
          <span className="text-sm font-medium">{scanning ? "Analyse en cours" : TITLES[current]}</span>
          <SearchBox />
        </header>
        <main className={full ? "flex min-h-0 flex-1 flex-col p-6" : "min-h-0 flex-1 overflow-y-auto p-6"}>
          {scanning ? (
            <ScanningView />
          ) : current === "home" ? (
            <HomeView />
          ) : current === "overview" ? (
            <OverviewView />
          ) : current === "explorer" ? (
            <ExplorerView />
          ) : current === "largest" ? (
            <LargestView />
          ) : current === "types" ? (
            <TypesView />
          ) : current === "suggestions" ? (
            <SuggestionsView />
          ) : current === "duplicates" ? (
            <DuplicatesView />
          ) : (
            <SearchView />
          )}
        </main>
      </SidebarInset>
    </SidebarProvider>
  )
}
