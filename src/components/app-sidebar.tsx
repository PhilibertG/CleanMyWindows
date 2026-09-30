import {
  ChartPie,
  Copy,
  FileStack,
  FolderTree,
  HardDrive,
  LayoutDashboard,
  Moon,
  ShieldCheck,
  Sparkles,
  Sun,
  Zap,
  type LucideIcon,
} from "lucide-react"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
} from "@/components/ui/sidebar"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { api } from "@/lib/api"
import { formatBytes } from "@/lib/format"
import { useStore, type View } from "@/lib/store"
import { useTheme } from "@/lib/theme"

const ANALYSIS: { view: View; label: string; icon: LucideIcon }[] = [
  { view: "overview", label: "Vue d'ensemble", icon: LayoutDashboard },
  { view: "explorer", label: "Explorateur", icon: FolderTree },
  { view: "largest", label: "Plus gros fichiers", icon: FileStack },
  { view: "types", label: "Types de fichiers", icon: ChartPie },
]

export function AppSidebar() {
  const { view, setView, summary, status, recoverable, system, dup } = useStore()
  const { resolved, toggle } = useTheme()
  const hasScan = !!summary

  return (
    <Sidebar collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" onClick={() => setView("home")} tooltip="CleanMyWindows">
              <div className="hero-gradient flex aspect-square size-8 items-center justify-center rounded-lg text-white shadow-sm">
                <Sparkles className="size-4" />
              </div>
              <div className="grid flex-1 text-left leading-tight">
                <span className="truncate font-semibold">CleanMyWindows</span>
                <span className="truncate text-xs text-muted-foreground">Analyse &amp; nettoyage</span>
              </div>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>

      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu>
              <SidebarMenuItem>
                <SidebarMenuButton isActive={view === "home"} onClick={() => setView("home")} tooltip="Disques">
                  <HardDrive />
                  <span>Disques</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>

        <SidebarGroup>
          <SidebarGroupLabel className="truncate">
            {summary ? summary.info.rootPath : "Analyse"}
          </SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {ANALYSIS.map((item) => (
                <SidebarMenuItem key={item.view}>
                  <SidebarMenuButton
                    isActive={view === item.view}
                    disabled={!hasScan || status === "scanning"}
                    onClick={() => setView(item.view)}
                    tooltip={item.label}
                  >
                    <item.icon />
                    <span>{item.label}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>

        <SidebarGroup>
          <SidebarGroupLabel>Nettoyage</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              <SidebarMenuItem>
                <SidebarMenuButton
                  isActive={view === "suggestions"}
                  disabled={!hasScan || status === "scanning"}
                  onClick={() => setView("suggestions")}
                  tooltip="Propositions de suppression"
                >
                  <Sparkles />
                  <span>Propositions</span>
                </SidebarMenuButton>
                {hasScan && recoverable > 0 && <SidebarMenuBadge className="tabular">{formatBytes(recoverable, 0)}</SidebarMenuBadge>}
              </SidebarMenuItem>
              <SidebarMenuItem>
                <SidebarMenuButton
                  isActive={view === "duplicates"}
                  disabled={!hasScan || status === "scanning"}
                  onClick={() => setView("duplicates")}
                  tooltip="Doublons"
                >
                  <Copy />
                  <span>Doublons</span>
                </SidebarMenuButton>
                {dup.result && dup.result.wasted > 0 && (
                  <SidebarMenuBadge className="tabular">{formatBytes(dup.result.wasted, 0)}</SidebarMenuBadge>
                )}
              </SidebarMenuItem>
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>

      <SidebarFooter>
        <SidebarMenu>
          <SidebarMenuItem>
            {system?.elevated ? (
              <SidebarMenuButton tooltip="Mode Turbo actif (administrateur)" className="cursor-default hover:bg-transparent">
                <ShieldCheck className="text-success" />
                <span className="flex items-center gap-2">
                  Mode Turbo <Badge variant="secondary" className="h-5 px-1.5 text-[10px]">Admin</Badge>
                </span>
              </SidebarMenuButton>
            ) : (
              <Tooltip>
                <TooltipTrigger asChild>
                  <SidebarMenuButton
                    onClick={() => api.restartAsAdmin().catch((e) => toast.error(String(e)))}
                    className="text-primary"
                  >
                    <Zap />
                    <span>Activer le mode Turbo</span>
                  </SidebarMenuButton>
                </TooltipTrigger>
                <TooltipContent side="right" className="max-w-64">
                  Relance en administrateur : lecture directe de la table des fichiers NTFS (analyse en quelques secondes) et accès aux dossiers protégés.
                </TooltipContent>
              </Tooltip>
            )}
          </SidebarMenuItem>
          <SidebarMenuItem>
            <Button variant="ghost" size="sm" className="w-full justify-start gap-2 px-2 group-data-[collapsible=icon]:justify-center" onClick={toggle}>
              {resolved === "dark" ? <Sun className="size-4" /> : <Moon className="size-4" />}
              <span className="group-data-[collapsible=icon]:hidden">{resolved === "dark" ? "Thème clair" : "Thème sombre"}</span>
            </Button>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  )
}
