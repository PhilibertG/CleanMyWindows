import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import App from "./App"
import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"
import { StoreProvider } from "@/lib/store"
import "./index.css"

// Pas de menu contextuel du navigateur (sauf dans les champs de saisie).
document.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement
  if (!t.closest("input, textarea")) e.preventDefault()
})

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <StoreProvider>
      <TooltipProvider delayDuration={300}>
        <App />
        <Toaster richColors position="bottom-right" />
      </TooltipProvider>
    </StoreProvider>
  </StrictMode>,
)
