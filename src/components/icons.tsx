import {
  AppWindow,
  Archive,
  Brain,
  Bug,
  HardDrive,
  Smartphone,
  Clock,
  Code2,
  Disc3,
  Download,
  File,
  FileAudio,
  FileCode2,
  FileImage,
  FileText,
  FileVideo,
  Folder,
  FolderLock,
  Globe,
  Inbox,
  Layers,
  Link2,
  Package,
  ScrollText,
  Settings2,
  Shield,
  Trash2,
  type LucideIcon,
} from "lucide-react"

import type { Category, NodeInfo } from "@/lib/api"
import { categoryVar } from "@/lib/format"
import { cn } from "@/lib/utils"

export const CATEGORY_ICONS: Record<Category | "grouped", LucideIcon> = {
  images: FileImage,
  videos: FileVideo,
  audio: FileAudio,
  documents: FileText,
  archives: Archive,
  applications: AppWindow,
  code: FileCode2,
  system: Settings2,
  temporary: Clock,
  other: File,
  folder: Folder,
  grouped: Layers,
}

export const SUGGESTION_ICONS: Record<string, LucideIcon> = {
  trash: Trash2,
  clock: Clock,
  globe: Globe,
  layers: Layers,
  bug: Bug,
  download: Download,
  package: Package,
  code: Code2,
  inbox: Inbox,
  disc: Disc3,
  file: File,
  scroll: ScrollText,
  shield: Shield,
  brain: Brain,
  smartphone: Smartphone,
  "hard-drive": HardDrive,
}

export function NodeIcon({ node, className }: { node: Pick<NodeInfo, "isDir" | "category" | "denied" | "link">; className?: string }) {
  if (node.isDir) {
    const Icon = node.denied ? FolderLock : node.link ? Link2 : Folder
    return <Icon className={cn("size-4 shrink-0 text-primary/80", className)} />
  }
  const Icon = CATEGORY_ICONS[node.category] ?? File
  return <Icon className={cn("size-4 shrink-0", className)} style={{ color: categoryVar(node.category) }} />
}
